use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    AnyBoundNodeId, BoundDependencyGuard, BoundDependencyRequirement, BoundDependencySubject,
    BoundExpression, BoundExpressionId, BoundUnit, CheckedMemoryOperations,
    CheckedSemanticSelections, DependencyContractInstantiationError, LastUse, LiveAcrossScope,
    LiveAcrossSuspension, LivenessFacts, SemanticSelection, StorageAccessRoot, StorageBinding,
    StoragePlan,
};
use bray_symbols::{CallableSignatureFact, TypeData};

use crate::dependency::selected_call_contract;
use crate::storage::local_initialization_bindings;
use crate::{
    CheckerInfrastructureError, CheckerOutcome, CheckerRequestContext, CheckerSemanticFactProvider,
    CheckerUnitView,
};

use super::build::{ControlFlowGraphBuildOutcome, build_storage_control_flow_graph};
use super::fixed_point::{FixedPointDomain, FixedPointOutcome, FlowDirection, solve_fixed_point};
use super::model::{
    AnalysisBlock, AnalysisEdge, AnalysisEdgeKind, AnalysisOperation, AnalysisOperationKind,
    AnalysisScopeExitPhase, ControlFlowGraph,
};
use super::reachability::{ReachabilityResult, analyze_reachability};
use super::storage_index::index_storage_roots;

pub(crate) fn analyze_storage_liveness<C>(
    request: CheckerUnitView<'_, C>,
    selections: &CheckedSemanticSelections,
    storage: &StoragePlan,
    memory: &CheckedMemoryOperations,
) -> CheckerOutcome<LivenessFacts>
where
    C: CheckerRequestContext + CheckerSemanticFactProvider<CallableSignatureFact> + ?Sized,
{
    if selections.unit() != request.unit().unit()
        || selections.kind() != request.unit().key().kind()
        || storage.unit() != request.unit().unit()
        || storage.kind() != request.unit().key().kind()
    {
        return CheckerOutcome::InfrastructureFailure(
            CheckerInfrastructureError::InvalidLivenessFacts,
        );
    }

    let graph = match build_storage_control_flow_graph(request, storage, selections) {
        ControlFlowGraphBuildOutcome::Complete(graph) => graph,
        ControlFlowGraphBuildOutcome::Cancelled => {
            return CheckerOutcome::Cancelled;
        }
    };

    if !graph.is_well_formed() {
        panic!("checker control-flow graph violated its construction invariants");
    }

    let effects = match OperationEffects::from_checked_inputs(request, selections, storage, memory)
    {
        Ok(effects) => effects,
        Err(error) => return CheckerOutcome::InfrastructureFailure(error),
    };

    let Some(reachability) = analyze_reachability(&graph, request) else {
        return CheckerOutcome::Cancelled;
    };

    let domain = LivenessDomain::new(&graph, &reachability, &effects);

    let result = match solve_fixed_point(&graph, &domain, &request) {
        FixedPointOutcome::Complete(result) => result,
        FixedPointOutcome::Cancelled => {
            return CheckerOutcome::Cancelled;
        }
        FixedPointOutcome::ConvergenceInvariantViolated => {
            panic!("finite liveness analysis exceeded its convergence bound")
        }
    };

    let facts = collect_facts(
        &graph,
        &reachability,
        &effects,
        domain.universe(),
        &result,
        storage.kind(),
    );

    match facts {
        Ok(facts) => CheckerOutcome::without_diagnostics(facts),
        Err(_) => {
            CheckerOutcome::InfrastructureFailure(CheckerInfrastructureError::InvalidLivenessFacts)
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct OperationEffect {
    uses: BTreeSet<BoundDependencySubject>,
    definitions: BTreeSet<BoundDependencySubject>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct OperationEffects {
    by_node: BTreeMap<AnyBoundNodeId, OperationEffect>,
    universe: BTreeSet<BoundDependencySubject>,
    recovered_nodes: BTreeSet<AnyBoundNodeId>,
}

impl OperationEffects {
    fn from_checked_inputs<C>(
        request: CheckerUnitView<'_, C>,
        selections: &CheckedSemanticSelections,
        storage: &StoragePlan,
        memory: &CheckedMemoryOperations,
    ) -> Result<Self, CheckerInfrastructureError>
    where
        C: CheckerRequestContext + CheckerSemanticFactProvider<CallableSignatureFact> + ?Sized,
    {
        let mut effects = Self::from_storage_plan(request.unit(), storage, memory);

        effects.retain_call_input_dependencies(request.unit());
        effects.add_selected_call_dependencies(request, selections, storage)?;
        effects.retain_local_borrow_dependencies(request, storage)?;

        Ok(effects)
    }

    fn from_storage_plan(
        unit: &BoundUnit,
        storage: &StoragePlan,
        memory: &CheckedMemoryOperations,
    ) -> Self {
        let mut effects = Self::default();

        for (identity, provenance) in storage.identity_entries() {
            let subject = BoundDependencySubject::Storage(identity);

            effects.universe.insert(subject);

            let Some(node) = provenance.definition_node() else {
                continue;
            };

            effects
                .by_node
                .entry(node)
                .or_default()
                .definitions
                .insert(subject);
        }

        for plan in storage.access_plans() {
            let subject = BoundDependencySubject::StorageAccess(plan.access());
            let node = AnyBoundNodeId::Expression(plan.expression());
            let effect = effects.by_node.entry(node).or_default();

            effect.uses.insert(subject);
            effect.definitions.insert(subject);
            effects.universe.insert(subject);

            let Some(access) = storage.access(plan.access()) else {
                effects.recovered_nodes.insert(node);

                continue;
            };

            if access.is_recovered() {
                effects.recovered_nodes.insert(node);
            }

            for subject in access_root_subjects(storage, access.root()) {
                effect.uses.insert(subject);
                effects.universe.insert(subject);
            }
        }

        for (capability, planned) in storage.borrow_capability_entries() {
            let subject = BoundDependencySubject::BorrowCapability(capability);

            effects.universe.insert(subject);

            let Some(expression) = planned.expression() else {
                continue;
            };

            let node = AnyBoundNodeId::Expression(expression);
            let effect = effects.by_node.entry(node).or_default();

            effect.definitions.insert(subject);

            effect
                .uses
                .insert(BoundDependencySubject::StorageAccess(planned.access()));

            if let Some(access) = storage.access(planned.access()) {
                effect
                    .uses
                    .extend(access_root_subjects(storage, access.root()));
            }

            if let Some(parent) = planned.parent() {
                let parent = BoundDependencySubject::BorrowCapability(parent);

                effect.uses.insert(parent);
                effects.universe.insert(parent);
            }
        }

        let await_uses = unit
            .tree()
            .expressions()
            .filter_map(|(expression, node)| match node {
                BoundExpression::Await(awaited) => Some((
                    expression,
                    effects.subtree_subjects(unit, awaited.operand()),
                )),
                _ => None,
            })
            .collect::<Vec<_>>();

        for (expression, subjects) in await_uses {
            effects.extend_uses(expression, subjects);
        }

        for operation in memory.operations() {
            let subjects = operation
                .arguments()
                .iter()
                .flat_map(|argument| effects.subtree_subjects(unit, *argument))
                .collect::<BTreeSet<_>>();

            effects.extend_uses(operation.expression(), subjects);
        }

        effects
    }

    fn retain_call_input_dependencies(&mut self, unit: &BoundUnit) {
        let call_inputs = unit
            .tree()
            .expressions()
            .filter_map(|(expression, node)| {
                let BoundExpression::Call(call) = node else {
                    return None;
                };

                let subjects = std::iter::once(call.callee())
                    .chain(
                        call.arguments()
                            .iter()
                            .map(bray_bound_tree::BoundArgument::expression),
                    )
                    .flat_map(|input| self.subtree_subjects(unit, input))
                    .collect::<BTreeSet<_>>();

                Some((expression, subjects))
            })
            .collect::<Vec<_>>();

        for (expression, subjects) in call_inputs {
            self.extend_uses(expression, subjects);
        }
    }

    fn add_selected_call_dependencies<C>(
        &mut self,
        request: CheckerUnitView<'_, C>,
        selections: &CheckedSemanticSelections,
        storage: &StoragePlan,
    ) -> Result<(), CheckerInfrastructureError>
    where
        C: CheckerRequestContext + CheckerSemanticFactProvider<CallableSignatureFact> + ?Sized,
    {
        for entry in selections.entries() {
            let SemanticSelection::Call(call) = entry.selection() else {
                continue;
            };

            let contract = match selected_call_contract(request, storage, entry.expression(), call)
            {
                Ok(contract) => contract,
                Err(DependencyContractInstantiationError::Resolution(
                    CheckerInfrastructureError::InvalidSemanticSelectionInput,
                )) => {
                    self.recovered_nodes
                        .insert(AnyBoundNodeId::Expression(entry.expression()));

                    continue;
                }
                Err(DependencyContractInstantiationError::Resolution(error)) => return Err(error),
                Err(DependencyContractInstantiationError::ForeignUnit) => {
                    return Err(CheckerInfrastructureError::InvalidLivenessFacts);
                }
            };

            let subjects = dependency_subjects(contract.requirements(), storage);

            self.universe.extend(subjects.iter().copied());
            self.extend_uses(entry.expression(), subjects);
        }

        Ok(())
    }

    fn retain_local_borrow_dependencies<C>(
        &mut self,
        request: CheckerUnitView<'_, C>,
        storage: &StoragePlan,
    ) -> Result<(), CheckerInfrastructureError>
    where
        C: CheckerRequestContext + ?Sized,
    {
        let (accesses_by_root, types_by_root) = index_storage_roots(storage);

        for (initializer, bindings) in local_initialization_bindings(request, storage) {
            let subjects = self.subtree_subjects(request.unit(), initializer);

            for binding in bindings {
                let (root, ty) = match binding {
                    StorageBinding::Identity(identity) => {
                        (Some(identity), types_by_root.get(&identity).copied())
                    }
                    StorageBinding::Access(access) => (
                        storage.root_identity(access),
                        storage.access(access).map(|access| access.reached_type()),
                    ),
                };

                let Some(root) = root else {
                    continue;
                };

                let Some(ty) = ty else {
                    continue;
                };

                if !type_is_borrow(request, ty)? {
                    continue;
                }

                for expression in accesses_by_root.get(&root).into_iter().flatten() {
                    self.extend_uses(*expression, subjects.iter().copied());
                }
            }
        }

        Ok(())
    }

    fn extend_uses(
        &mut self,
        expression: BoundExpressionId,
        subjects: impl IntoIterator<Item = BoundDependencySubject>,
    ) {
        self.by_node
            .entry(AnyBoundNodeId::Expression(expression))
            .or_default()
            .uses
            .extend(subjects);
    }

    fn subtree_subjects(
        &self,
        unit: &BoundUnit,
        root: BoundExpressionId,
    ) -> BTreeSet<BoundDependencySubject> {
        let mut subjects = BTreeSet::new();
        let mut pending = vec![root];

        while let Some(expression) = pending.pop() {
            let node = AnyBoundNodeId::Expression(expression);

            if let Some(effect) = self.by_node.get(&node) {
                subjects.extend(effect.uses.iter().copied());
                subjects.extend(effect.definitions.iter().copied());
            }

            if let Some(node) = unit.tree().expression(expression) {
                pending.extend(node.child_expressions());
            }
        }

        subjects
    }

    fn effect(&self, node: AnyBoundNodeId) -> Option<&OperationEffect> {
        self.by_node.get(&node)
    }

    fn operation_requires_conservative_liveness(operation: &AnalysisOperation) -> bool {
        matches!(operation.kind(), AnalysisOperationKind::Recovery(_))
    }
}

fn type_is_borrow<C>(
    request: CheckerUnitView<'_, C>,
    ty: bray_symbols::TypeId,
) -> Result<bool, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    request
        .semantic_values()
        .type_data(ty)
        .map(|data| matches!(data.as_ref(), TypeData::Borrow { .. }))
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)
}

fn dependency_subjects(
    requirements: &[BoundDependencyRequirement],
    storage: &StoragePlan,
) -> BTreeSet<BoundDependencySubject> {
    let mut subjects = BTreeSet::new();

    collect_dependency_subjects(requirements, storage, &mut subjects);

    subjects
}

fn collect_dependency_subjects(
    requirements: &[BoundDependencyRequirement],
    storage: &StoragePlan,
    subjects: &mut BTreeSet<BoundDependencySubject>,
) {
    for requirement in requirements {
        match requirement {
            BoundDependencyRequirement::Direct { subject, .. } => {
                subjects.insert(*subject);

                if let BoundDependencySubject::StorageAccess(access) = subject
                    && let Some(access) = storage.access(*access)
                {
                    subjects.extend(access_root_subjects(storage, access.root()));
                }
            }
            BoundDependencyRequirement::Guarded(requirement) => {
                let guard = match requirement.guard() {
                    BoundDependencyGuard::NullablePresent(access)
                    | BoundDependencyGuard::ActiveUnionVariant { access, .. } => {
                        BoundDependencySubject::StorageAccess(access)
                    }
                    BoundDependencyGuard::BorrowCapabilityActive(capability) => {
                        BoundDependencySubject::BorrowCapability(capability)
                    }
                    BoundDependencyGuard::ScopedCapabilityLive(capability) => {
                        BoundDependencySubject::ScopedCapability(capability)
                    }
                };

                subjects.insert(guard);
                collect_dependency_subjects(requirement.requirements(), storage, subjects);
            }
        }
    }
}

fn access_root_subjects(
    storage: &StoragePlan,
    root: StorageAccessRoot,
) -> Vec<BoundDependencySubject> {
    match root {
        StorageAccessRoot::Storage(storage)
        | StorageAccessRoot::Recovery(storage)
        | StorageAccessRoot::OwnedIndirection { storage, .. } => {
            vec![BoundDependencySubject::Storage(storage)]
        }
        StorageAccessRoot::Borrow(capability) => {
            let mut subjects = Vec::new();
            let mut current = Some(capability);

            while let Some(capability) = current {
                subjects.push(BoundDependencySubject::BorrowCapability(capability));

                current = storage
                    .borrow_capability(capability)
                    .and_then(|planned| planned.parent());
            }

            subjects
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct BlockTransfer {
    generated: BTreeSet<BoundDependencySubject>,
    killed: BTreeSet<BoundDependencySubject>,
}

struct LivenessDomain<'analysis> {
    reachability: &'analysis ReachabilityResult,
    effects: &'analysis OperationEffects,
    transfers: Box<[BlockTransfer]>,
}

impl<'analysis> LivenessDomain<'analysis> {
    fn new(
        graph: &'analysis ControlFlowGraph,
        reachability: &'analysis ReachabilityResult,
        effects: &'analysis OperationEffects,
    ) -> Self {
        let transfers = graph
            .blocks()
            .iter()
            .map(|block| block_transfer(graph, block, effects))
            .collect();

        Self {
            reachability,
            effects,
            transfers,
        }
    }

    fn universe(&self) -> &BTreeSet<BoundDependencySubject> {
        &self.effects.universe
    }

    fn transfer(&self, block: &AnalysisBlock) -> Option<&BlockTransfer> {
        block
            .id()
            .to_index()
            .and_then(|index| self.transfers.get(index))
    }
}

impl FixedPointDomain for LivenessDomain<'_> {
    type State = BTreeSet<BoundDependencySubject>;

    fn direction(&self) -> FlowDirection {
        FlowDirection::Backward
    }

    fn bottom(&self) -> Self::State {
        BTreeSet::new()
    }

    fn should_seed(&self, block: &AnalysisBlock) -> bool {
        self.reachability.is_block_reachable(block.id())
            && self
                .transfer(block)
                .is_some_and(|transfer| !transfer.generated.is_empty())
    }

    fn boundary(&self) -> Self::State {
        BTreeSet::new()
    }

    fn merge_boundary(&self, _: &mut Self::State, _: &Self::State) -> bool {
        false
    }

    fn transfer(&self, block: &AnalysisBlock, source: &Self::State) -> Self::State {
        if !self.reachability.is_block_reachable(block.id()) {
            return BTreeSet::new();
        }

        let Some(transfer) = self.transfer(block) else {
            return BTreeSet::new();
        };

        let mut output = transfer.generated.clone();

        output.extend(
            source
                .iter()
                .filter(|subject| !transfer.killed.contains(subject))
                .copied(),
        );

        output
    }

    fn propagate(
        &self,
        source: &Self::State,
        edge: &AnalysisEdge,
        target: &mut Self::State,
    ) -> bool {
        if !self.reachability.is_edge_reachable(edge.id()) {
            return false;
        }

        if edge.kind() == AnalysisEdgeKind::Recovery {
            return merge_state(target, self.universe());
        }

        merge_state(target, source)
    }

    fn convergence_bound(&self, graph: &ControlFlowGraph) -> usize {
        graph
            .blocks()
            .len()
            .saturating_mul(self.universe().len().saturating_add(1))
    }
}

fn block_transfer(
    graph: &ControlFlowGraph,
    block: &AnalysisBlock,
    effects: &OperationEffects,
) -> BlockTransfer {
    let mut transfer = BlockTransfer::default();

    for operation in block.operations().iter().rev() {
        let Some(operation) = graph.operation(*operation) else {
            continue;
        };

        if OperationEffects::operation_requires_conservative_liveness(operation) {
            transfer.generated = effects.universe.clone();
            transfer.killed.clear();

            continue;
        }

        let Some(effect) = effects.effect(operation.kind().node()) else {
            continue;
        };

        transfer
            .generated
            .retain(|subject| !effect.definitions.contains(subject));

        transfer.generated.extend(effect.uses.iter().copied());
        transfer.killed.extend(effect.definitions.iter().copied());
    }

    transfer
}

fn transfer_operation(
    state: &mut BTreeSet<BoundDependencySubject>,
    operation: &AnalysisOperation,
    effects: &OperationEffects,
    universe: &BTreeSet<BoundDependencySubject>,
) {
    if OperationEffects::operation_requires_conservative_liveness(operation) {
        state.extend(universe.iter().copied());

        return;
    }

    let Some(effect) = effects.effect(operation.kind().node()) else {
        return;
    };

    state.retain(|subject| !effect.definitions.contains(subject));
    state.extend(effect.uses.iter().copied());
}

fn merge_state(
    target: &mut BTreeSet<BoundDependencySubject>,
    incoming: &BTreeSet<BoundDependencySubject>,
) -> bool {
    let previous_len = target.len();

    target.extend(incoming.iter().copied());

    target.len() != previous_len
}

fn collect_facts(
    graph: &ControlFlowGraph,
    reachability: &ReachabilityResult,
    effects: &OperationEffects,
    universe: &BTreeSet<BoundDependencySubject>,
    result: &super::fixed_point::FixedPointResult<BTreeSet<BoundDependencySubject>>,
    kind: bray_bound_tree::BoundUnitKind,
) -> Result<LivenessFacts, bray_bound_tree::LivenessFactsBuildError> {
    let mut last_uses = Vec::new();
    let mut live_across_scopes = Vec::new();
    let mut live_across_suspensions = Vec::new();
    let mut is_recovered = false;

    for block in graph.blocks() {
        if !reachability.is_block_reachable(block.id()) {
            continue;
        }

        let Some(live_out) = result.state(block.id()) else {
            continue;
        };

        let mut state = live_out.clone();

        for operation in block.operations().iter().rev() {
            let Some(operation) = graph.operation(*operation) else {
                continue;
            };

            if let AnalysisOperationKind::ScopeExit {
                block,
                phase: AnalysisScopeExitPhase::LifecycleResolution,
                ..
            } = operation.kind()
            {
                live_across_scopes.extend(
                    state
                        .iter()
                        .copied()
                        .map(|subject| LiveAcrossScope::new(block, subject)),
                );
            }

            if let AnalysisOperationKind::Suspension { expression, .. } = operation.kind() {
                live_across_suspensions.extend(
                    state
                        .iter()
                        .copied()
                        .chain(
                            effects
                                .effect(operation.kind().node())
                                .into_iter()
                                .flat_map(|effect| effect.uses.iter().copied()),
                        )
                        .map(|subject| LiveAcrossSuspension::new(expression, subject)),
                );
            }

            if OperationEffects::operation_requires_conservative_liveness(operation) {
                is_recovered = true;
            } else if let Some(effect) = effects.effect(operation.kind().node()) {
                is_recovered |= effects.recovered_nodes.contains(&operation.kind().node());

                last_uses.extend(
                    effect
                        .uses
                        .iter()
                        .filter(|subject| {
                            !effect.definitions.contains(subject) && !state.contains(subject)
                        })
                        .copied()
                        .map(|subject| LastUse::new(subject, operation.kind().node())),
                );

                last_uses.extend(
                    effect
                        .definitions
                        .iter()
                        .filter(|subject| {
                            matches!(subject, BoundDependencySubject::BorrowCapability(_))
                                && !effect.uses.contains(subject)
                                && !state.contains(subject)
                        })
                        .copied()
                        .map(|subject| LastUse::new(subject, operation.kind().node())),
                );
            }

            transfer_operation(&mut state, operation, effects, universe);
        }
    }

    if graph.edges().iter().any(|edge| {
        reachability.is_edge_reachable(edge.id()) && edge.kind() == AnalysisEdgeKind::Recovery
    }) {
        is_recovered = true;
    }

    LivenessFacts::try_new(
        graph.unit(),
        kind,
        last_uses,
        live_across_scopes,
        live_across_suspensions,
        is_recovered,
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use bray_bound_tree::{
        AnyBoundNodeId, BorrowCapabilityOrigin, BoundArgument, BoundCallExpression,
        BoundDependencySubject, BoundErrorExpression, BoundExpression, BoundExpressionId,
        BoundNodeOrigin, BoundUnit, BoundUnitId, CheckedMemoryOperations, PlannedBorrowCapability,
        StorageAccess, StorageAccessPurpose, StorageAccessRoot, StorageIdentity,
        StoragePlanBuilder,
    };
    use bray_symbols::BorrowKind;

    use super::{OperationEffects, transfer_operation};
    use crate::analysis::id::{AnalysisOperationId, ProgramPointId};
    use crate::analysis::model::{AnalysisOperation, AnalysisOperationKind};
    use crate::test_support::{callable_key, error_type, expression_unit, push_expression};

    #[test]
    fn storage_plans_produce_exact_uses_and_definitions() {
        let unit = BoundUnitId::new(7);

        let (bound_unit, expression, origin) = test_expression(unit);

        let mut builder =
            StoragePlanBuilder::new(unit, bray_bound_tree::BoundUnitKind::CallableBody);

        let Ok(storage) = builder.push_identity(StorageIdentity::Temporary(expression)) else {
            panic!("test storage identity must be valid");
        };

        let access = StorageAccess::new(
            StorageAccessRoot::Storage(storage),
            [],
            error_type(),
            origin.source_anchor(),
            false,
        );

        let Ok(access) = builder.push_access(access) else {
            panic!("test storage access must be valid");
        };

        if builder
            .plan_access(expression, StorageAccessPurpose::Read, access)
            .is_err()
        {
            panic!("test access plan must be valid");
        }

        let memory = empty_memory_operations(unit);
        let effects = OperationEffects::from_storage_plan(&bound_unit, &builder.finish(), &memory);
        let node = AnyBoundNodeId::Expression(expression);

        let Some(effect) = effects.effect(node) else {
            panic!("planned access must produce one operation effect");
        };

        assert_eq!(
            effect.uses,
            BTreeSet::from([
                BoundDependencySubject::Storage(storage),
                BoundDependencySubject::StorageAccess(access),
            ])
        );

        assert_eq!(
            effect.definitions,
            BTreeSet::from([
                BoundDependencySubject::Storage(storage),
                BoundDependencySubject::StorageAccess(access),
            ])
        );
    }

    #[test]
    fn recovered_storage_accesses_keep_every_known_subject_live() {
        let unit = BoundUnitId::new(8);

        let (bound_unit, expression, origin) = test_expression(unit);

        let mut builder =
            StoragePlanBuilder::new(unit, bray_bound_tree::BoundUnitKind::CallableBody);

        let Ok(storage) = builder.push_identity(StorageIdentity::Temporary(expression)) else {
            panic!("test storage identity must be valid");
        };

        let access = StorageAccess::new(
            StorageAccessRoot::Storage(storage),
            [],
            error_type(),
            origin.source_anchor(),
            true,
        );

        let Ok(access) = builder.push_access(access) else {
            panic!("test storage access must be valid");
        };

        if builder
            .plan_access(expression, StorageAccessPurpose::Read, access)
            .is_err()
        {
            panic!("test access plan must be valid");
        }

        let memory = empty_memory_operations(unit);
        let effects = OperationEffects::from_storage_plan(&bound_unit, &builder.finish(), &memory);

        let operation = AnalysisOperation::new(
            AnalysisOperationId::from_slot(unit, 0),
            AnalysisOperationKind::Bound(expression.into()),
            ProgramPointId::from_slot(unit, 0),
            ProgramPointId::from_slot(unit, 1),
        );

        let mut state = BTreeSet::new();

        transfer_operation(&mut state, &operation, &effects, &effects.universe);

        assert_eq!(
            state,
            BTreeSet::from([
                BoundDependencySubject::Storage(storage),
                BoundDependencySubject::StorageAccess(access),
            ])
        );
    }

    #[test]
    fn identity_definitions_follow_their_bound_provenance() {
        let key = callable_key();
        let unit = BoundUnitId::new(9);

        let (_, expression, _) = test_expression(unit);

        let origin = BoundNodeOrigin::source(key.source());
        let identity = StorageIdentity::CompilerCreated(origin);

        assert_eq!(identity.definition_node(), None);

        assert_eq!(
            StorageIdentity::Temporary(expression).definition_node(),
            Some(expression.into())
        );
    }

    #[test]
    fn calls_retain_borrow_capabilities_created_by_their_inputs() {
        let unit = BoundUnitId::new(10);

        let (bound_unit, expressions) = expression_unit(unit, |tree, origin| {
            let callee = push_expression(
                tree,
                BoundExpression::Error(BoundErrorExpression::new(origin, error_type())),
            );

            let argument = push_expression(
                tree,
                BoundExpression::Error(BoundErrorExpression::new(origin, error_type())),
            );

            let call = push_expression(
                tree,
                BoundExpression::Call(BoundCallExpression::pending(
                    origin,
                    callee,
                    [],
                    [BoundArgument::new(argument, None, false)],
                )),
            );

            vec![callee, argument, call]
        });

        let [_, argument, call] = expressions.as_slice() else {
            panic!("test expressions must retain their source order");
        };

        let source = bound_unit
            .tree()
            .expression(*argument)
            .map(BoundExpression::origin)
            .map(BoundNodeOrigin::source_anchor)
            .unwrap_or_else(|| panic!("test argument must retain its source anchor"));

        let mut builder =
            StoragePlanBuilder::new(unit, bray_bound_tree::BoundUnitKind::CallableBody);

        let identity = builder
            .push_identity(StorageIdentity::Temporary(*argument))
            .unwrap_or_else(|error| panic!("test storage identity must build: {error:?}"));

        let access = builder
            .push_access(StorageAccess::new(
                StorageAccessRoot::Storage(identity),
                [],
                error_type(),
                source,
                false,
            ))
            .unwrap_or_else(|error| panic!("test storage access must build: {error:?}"));

        let capability = builder
            .push_borrow_capability(PlannedBorrowCapability::new(
                BorrowCapabilityOrigin::Expression(*argument),
                BorrowKind::Shared,
                access,
                None,
                source,
                false,
            ))
            .unwrap_or_else(|error| panic!("test borrow capability must build: {error:?}"));

        let memory = empty_memory_operations(unit);
        let storage = builder.finish();
        let mut effects = OperationEffects::from_storage_plan(&bound_unit, &storage, &memory);

        effects.retain_call_input_dependencies(&bound_unit);

        let Some(effect) = effects.effect(AnyBoundNodeId::Expression(*call)) else {
            panic!("call input retention must produce one operation effect");
        };

        assert!(
            effect
                .uses
                .contains(&BoundDependencySubject::BorrowCapability(capability))
        );
    }

    fn test_expression(unit: BoundUnitId) -> (BoundUnit, BoundExpressionId, BoundNodeOrigin) {
        let (unit, expressions) = expression_unit(unit, |tree, origin| {
            vec![push_expression(
                tree,
                BoundExpression::Error(BoundErrorExpression::new(origin, error_type())),
            )]
        });

        (
            unit,
            expressions[0],
            BoundNodeOrigin::source(callable_key().source()),
        )
    }

    fn empty_memory_operations(unit: BoundUnitId) -> CheckedMemoryOperations {
        CheckedMemoryOperations::try_new(
            unit,
            bray_bound_tree::BoundUnitKind::CallableBody,
            [],
            false,
        )
        .unwrap_or_else(|error| panic!("empty memory operations must build: {error:?}"))
    }
}
