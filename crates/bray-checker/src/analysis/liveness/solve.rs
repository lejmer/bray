use std::collections::BTreeSet;

use bray_bound_tree::{
    BoundDependencySubject, CheckedMemoryOperations, CheckedSemanticSelections, LastUse,
    LiveAcrossScope, LiveAcrossSuspension, Liveness, StoragePlan,
};
use bray_symbols::CallableSignatureQuery;

use crate::analysis::build::{ControlFlowGraphBuildOutcome, build_storage_control_flow_graph};
use crate::analysis::fixed_point::{
    FixedPointDomain, FixedPointOutcome, FixedPointResult, FlowDirection, solve_fixed_point,
};
use crate::analysis::model::{
    AnalysisBlock, AnalysisEdge, AnalysisEdgeKind, AnalysisOperation, AnalysisOperationKind,
    AnalysisScopeExitPhase, ControlFlowGraph,
};
use crate::analysis::reachability::{ReachabilityResult, analyze_reachability};
use crate::unit::semantic_input_failure;
use crate::{
    CheckerInfrastructureError, CheckerInputKind, CheckerOutcome, CheckerQueryError,
    CheckerRequestContext, CheckerSemanticQueryProvider, CheckerUnitView,
};

use super::effects::OperationEffects;

pub(crate) fn analyze_storage_liveness<C>(
    request: CheckerUnitView<'_, C>,
    selections: &CheckedSemanticSelections,
    storage: &StoragePlan,
    memory: &CheckedMemoryOperations,
) -> CheckerOutcome<Liveness, C::UpstreamError>
where
    C: CheckerRequestContext + CheckerSemanticQueryProvider<CallableSignatureQuery> + ?Sized,
{
    if let Some(error) = semantic_input_failure(
        request,
        [
            (
                CheckerInputKind::SemanticSelections,
                (selections.unit(), selections.kind()),
            ),
            (
                CheckerInputKind::StoragePlan,
                (storage.unit(), storage.kind()),
            ),
            (
                CheckerInputKind::MemoryOperations,
                (memory.unit(), memory.kind()),
            ),
        ],
    ) {
        return CheckerOutcome::InfrastructureFailure(error);
    }

    let graph = match build_storage_control_flow_graph(request, storage, selections) {
        ControlFlowGraphBuildOutcome::Complete(graph) => graph,
        ControlFlowGraphBuildOutcome::Cancelled => {
            return CheckerOutcome::Cancelled;
        }
        ControlFlowGraphBuildOutcome::InfrastructureFailure(error) => {
            return CheckerOutcome::InfrastructureFailure(error);
        }
        ControlFlowGraphBuildOutcome::UpstreamFailure(error) => {
            return CheckerOutcome::UpstreamFailure(error);
        }
    };

    analyze_storage_liveness_with_graph(request, selections, storage, memory, &graph)
}

pub(crate) fn analyze_storage_liveness_with_graph<C>(
    request: CheckerUnitView<'_, C>,
    selections: &CheckedSemanticSelections,
    storage: &StoragePlan,
    memory: &CheckedMemoryOperations,
    graph: &ControlFlowGraph,
) -> CheckerOutcome<Liveness, C::UpstreamError>
where
    C: CheckerRequestContext + CheckerSemanticQueryProvider<CallableSignatureQuery> + ?Sized,
{
    if !graph.is_well_formed() {
        panic!("checker control-flow graph violated its construction invariants");
    }

    let effects = match OperationEffects::from_checked_inputs(request, selections, storage, memory)
    {
        Ok(effects) => effects,
        Err(CheckerQueryError::Cancelled) => return CheckerOutcome::Cancelled,
        Err(CheckerQueryError::Infrastructure(error)) => {
            return CheckerOutcome::InfrastructureFailure(error);
        }
        Err(CheckerQueryError::Upstream(error)) => {
            return CheckerOutcome::UpstreamFailure(error);
        }
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

    let liveness = collect_liveness(
        &graph,
        &reachability,
        &effects,
        domain.universe(),
        &result,
        storage.kind(),
    );

    match liveness {
        Ok(liveness) => CheckerOutcome::without_diagnostics(liveness),
        Err(error) => {
            CheckerOutcome::InfrastructureFailure(CheckerInfrastructureError::Liveness(error))
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

        let Some(effect) = effects.operation_effect(operation) else {
            continue;
        };

        transfer
            .generated
            .retain(|subject| !effect.defines(subject));

        transfer.generated.extend(effect.uses().copied());
        transfer.killed.extend(effect.definitions().copied());
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

    if let AnalysisOperationKind::ScopeExit {
        exit,
        phase: AnalysisScopeExitPhase::LifecycleResolution,
        ..
    } = operation.kind()
    {
        if let Some(effect) = effects.effect(exit) {
            state.extend(
                effect
                    .uses
                    .iter()
                    .copied()
                    .filter(|subject| effects.is_owner_dependency(*subject)),
            );
        }

        return;
    }

    let Some(effect) = effects.operation_effect(operation) else {
        return;
    };

    state.retain(|subject| !effect.defines(subject));
    state.extend(effect.uses().copied());
}

fn merge_state(
    target: &mut BTreeSet<BoundDependencySubject>,
    incoming: &BTreeSet<BoundDependencySubject>,
) -> bool {
    let previous_len = target.len();

    target.extend(incoming.iter().copied());

    target.len() != previous_len
}

fn collect_liveness(
    graph: &ControlFlowGraph,
    reachability: &ReachabilityResult,
    effects: &OperationEffects,
    universe: &BTreeSet<BoundDependencySubject>,
    result: &FixedPointResult<BTreeSet<BoundDependencySubject>>,
    kind: bray_bound_tree::BoundUnitKind,
) -> Result<Liveness, bray_bound_tree::LivenessBuildError> {
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
                exit,
                phase: AnalysisScopeExitPhase::LifecycleResolution,
                ..
            } = operation.kind()
            {
                live_across_scopes.extend(
                    state
                        .iter()
                        .copied()
                        .chain(
                            effects
                                .effect(exit)
                                .into_iter()
                                .flat_map(|effect| effect.uses.iter().copied()),
                        )
                        .filter(|subject| {
                            state.contains(subject) || effects.is_owner_dependency(*subject)
                        })
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
            } else if let Some(effect) = effects.operation_effect(operation) {
                is_recovered |= effects.recovered_nodes.contains(&operation.kind().node());

                last_uses.extend(
                    effect
                        .uses()
                        .filter(|subject| !effect.defines(subject) && !state.contains(subject))
                        .copied()
                        .map(|subject| LastUse::new(subject, operation.kind().node())),
                );

                last_uses.extend(
                    effect
                        .definitions()
                        .filter(|subject| {
                            matches!(subject, BoundDependencySubject::BorrowCapability(_))
                                && !effect.uses().any(|used| used == *subject)
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

    Liveness::try_new(
        graph.unit(),
        kind,
        last_uses,
        live_across_scopes,
        live_across_suspensions,
        effects
            .owner_dependencies
            .iter()
            .flat_map(|(expression, subjects)| {
                subjects
                    .iter()
                    .copied()
                    .map(|subject| bray_bound_tree::OwnerRetention::new(*expression, subject))
            }),
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
            .plan_access(
                expression.into(),
                expression,
                StorageAccessPurpose::Read,
                access,
            )
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
            .plan_access(
                expression.into(),
                expression,
                StorageAccessPurpose::Read,
                access,
            )
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
