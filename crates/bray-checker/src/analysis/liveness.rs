use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    AnyBoundNodeId, BoundDependencySubject, LastUse, LiveAcrossScope, LiveAcrossSuspension,
    LivenessFacts, StorageAccessRoot, StorageIdentity, StoragePlan,
};

use crate::{CheckerInfrastructureError, CheckerOutcome, CheckerRequestContext, CheckerUnitView};

use super::build::{ControlFlowGraphBuildOutcome, build_storage_control_flow_graph};
use super::fixed_point::{FixedPointDomain, FixedPointOutcome, FlowDirection, solve_fixed_point};
use super::model::{
    AnalysisBlock, AnalysisEdge, AnalysisEdgeKind, AnalysisOperation, AnalysisOperationKind,
    AnalysisScopeExitPhase, ControlFlowGraph,
};
use super::reachability::{ReachabilityResult, analyze_reachability};

pub(crate) fn analyze_storage_liveness<C>(
    request: CheckerUnitView<'_, C>,
    storage: &StoragePlan,
) -> CheckerOutcome<LivenessFacts>
where
    C: CheckerRequestContext + ?Sized,
{
    if storage.unit() != request.unit().unit() || storage.kind() != request.unit().key().kind() {
        return CheckerOutcome::InfrastructureFailure(
            CheckerInfrastructureError::InvalidLivenessFacts,
        );
    }

    let graph = match build_storage_control_flow_graph(request, storage) {
        ControlFlowGraphBuildOutcome::Complete(graph) => graph,
        ControlFlowGraphBuildOutcome::Cancelled => {
            return CheckerOutcome::Cancelled;
        }
    };

    if !graph.is_well_formed() {
        panic!("checker control-flow graph violated its construction invariants");
    }

    let Some(reachability) = analyze_reachability(&graph, request) else {
        return CheckerOutcome::Cancelled;
    };

    let effects = OperationEffects::from_storage_plan(storage);
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
    fn from_storage_plan(storage: &StoragePlan) -> Self {
        let mut effects = Self::default();

        for (identity, provenance) in storage.identity_entries() {
            let subject = BoundDependencySubject::Storage(identity);

            effects.universe.insert(subject);

            let Some(node) = identity_definition(provenance) else {
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

            if let Some(parent) = planned.parent() {
                let parent = BoundDependencySubject::BorrowCapability(parent);

                effect.uses.insert(parent);
                effects.universe.insert(parent);
            }
        }

        effects
    }

    fn effect(&self, node: AnyBoundNodeId) -> Option<&OperationEffect> {
        self.by_node.get(&node)
    }

    fn operation_is_recovered(&self, operation: &AnalysisOperation) -> bool {
        matches!(operation.kind(), AnalysisOperationKind::Recovery(_))
            || self.recovered_nodes.contains(&operation.kind().node())
    }
}

const fn identity_definition(identity: StorageIdentity) -> Option<AnyBoundNodeId> {
    match identity {
        StorageIdentity::LocalOwned(node) | StorageIdentity::Result(node) => Some(node),
        StorageIdentity::Temporary(expression)
        | StorageIdentity::IterationCursor(expression)
        | StorageIdentity::IterationElement(expression)
        | StorageIdentity::Allocation(expression) => Some(AnyBoundNodeId::Expression(expression)),
        StorageIdentity::Alternative { pattern, .. } => Some(AnyBoundNodeId::Pattern(pattern)),
        StorageIdentity::Parameter(_)
        | StorageIdentity::Receiver(_)
        | StorageIdentity::AnonymousParameter(_)
        | StorageIdentity::PredicateParameter(_)
        | StorageIdentity::PostconditionResult(_)
        | StorageIdentity::CompilerCreated(_)
        | StorageIdentity::Error(_) => None,
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

        if effects.operation_is_recovered(operation) {
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
    if effects.operation_is_recovered(operation) {
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
            } = operation.kind()
            {
                live_across_scopes.extend(
                    state
                        .iter()
                        .copied()
                        .map(|subject| LiveAcrossScope::new(block, subject)),
                );
            }

            if let AnalysisOperationKind::DirectAwait(expression) = operation.kind() {
                live_across_suspensions.extend(
                    state
                        .iter()
                        .copied()
                        .map(|subject| LiveAcrossSuspension::new(expression, subject)),
                );
            }

            if effects.operation_is_recovered(operation) {
                is_recovered = true;
            } else if let Some(effect) = effects.effect(operation.kind().node()) {
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
        AnyBoundNodeId, BoundDependencySubject, BoundErrorExpression, BoundExpression,
        BoundExpressionId, BoundNodeOrigin, BoundTreeBuilder, BoundUnitId, StorageAccess,
        StorageAccessPurpose, StorageAccessRoot, StorageIdentity, StoragePlanBuilder,
    };

    use super::{OperationEffects, transfer_operation};
    use crate::analysis::id::{AnalysisOperationId, ProgramPointId};
    use crate::analysis::model::{AnalysisOperation, AnalysisOperationKind};
    use crate::test_support::{callable_key, error_type, push_expression};

    #[test]
    fn storage_plans_produce_exact_uses_and_definitions() {
        let unit = BoundUnitId::new(7);

        let (expression, origin) = test_expression(unit);

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

        let effects = OperationEffects::from_storage_plan(&builder.finish());
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

        let (expression, origin) = test_expression(unit);

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

        let effects = OperationEffects::from_storage_plan(&builder.finish());

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

        let (expression, _) = test_expression(unit);

        let origin = BoundNodeOrigin::source(key.source());
        let identity = StorageIdentity::CompilerCreated(origin);

        assert_eq!(super::identity_definition(identity), None);

        assert_eq!(
            super::identity_definition(StorageIdentity::Temporary(expression)),
            Some(expression.into())
        );
    }

    fn test_expression(unit: BoundUnitId) -> (BoundExpressionId, BoundNodeOrigin) {
        let origin = BoundNodeOrigin::source(callable_key().source());
        let mut tree = BoundTreeBuilder::new(unit);

        let expression = push_expression(
            &mut tree,
            BoundExpression::Error(BoundErrorExpression::new(origin, error_type())),
        );

        (expression, origin)
    }
}
