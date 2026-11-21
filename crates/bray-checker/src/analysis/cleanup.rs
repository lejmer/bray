use bray_bound_tree::{AnyBoundNodeId, StorageExitPoint, StorageIdentityId, StorageScope};
use bray_symbols::DependencyFailureKind;

use crate::CheckerRequestContext;

use super::build::ControlFlowGraphBuilder;
use super::id::AnalysisBlockId;
use super::model::{AnalysisEdgeKind, AnalysisExitKind, AnalysisRefinement, ScopedResultDestination, ScopedResultTransfer};

type FailureTails = std::collections::BTreeMap<(Option<AnalysisBlockId>, DependencyFailureKind), Vec<AnalysisBlockId>>;

pub(crate) fn exclude_infallible_cleanup_edges(
    graph: &super::model::ControlFlowGraph,
    unit: bray_bound_tree::BoundUnitView<'_>,
    storage: &bray_bound_tree::StoragePlan,
    cleanup: &super::guarantee::CleanupPlan,
    dependencies: &bray_bound_tree::CheckedDependencyContracts,
    excluded: &mut std::collections::BTreeSet<super::id::AnalysisEdgeId>,
) {
    use bray_bound_tree::{BoundControlTransferKind, BoundExpression, StorageValueConsumptionPhase};

    let mut fallible_owners = std::collections::BTreeMap::new();

    excluded.extend(graph.edges().iter().filter_map(|edge| {
        let point = edge.cleanup_failure_point()?;

        let owners = fallible_owners.entry(point.scope()).or_insert_with(|| {
            cleanup.fallible_scope_owners(storage, dependencies, point.scope()).collect::<Vec<_>>()
        });

        if owners.is_empty() { return Some(edge.id()); }

        let AnyBoundNodeId::Expression(transfer) = point.exit() else { return None; };

        let Some(BoundExpression::ControlTransfer(transfer_kind)) = unit.expression(transfer) else { return None; };

        if !matches!(transfer_kind.kind(), BoundControlTransferKind::Return | BoundControlTransferKind::Yield | BoundControlTransferKind::Break) {
            return None;
        }

        // These lexical sources are retired by the transfer before cleanup.
        // A pending destination is a separate owner if cleanup later fails.
        owners.iter().all(|owner| {
            storage.value_retirements(transfer.into(), StorageValueConsumptionPhase::Completion)
                .any(|path| path.root == *owner && path.projections.is_empty())
        })
            .then_some(edge.id())
    }));
}

pub(crate) fn scope_cleanup_failure(
    graph: &super::model::ControlFlowGraph,
    edge: &super::model::AnalysisEdge,
) -> Option<(StorageExitPoint, DependencyFailureKind)> {
    use bray_bound_tree::BoundExecutionSite::ScopedExit;

    let kind = match edge.refinement()? {
        AnalysisRefinement::CleanupOutcome { scope, exit, failure: Some(kind) } => {
            return Some((StorageExitPoint::new(scope, exit), kind));
        }
        AnalysisRefinement::InvocationFailure(ScopedExit(_), kind) => kind,
        AnalysisRefinement::InvocationEntry { invocation: ScopedExit(_), admitted: false } => DependencyFailureKind::Panic,
        _ => return None,
    };

    let point = graph.block(edge.source())?.operations().iter().filter_map(|id| graph.operation(*id))
        .filter(|operation| operation.is_normal_cleanup())
        .find_map(|operation| operation.scoped_exit())?;

    Some((point, kind))
}

impl<C> ControlFlowGraphBuilder<'_, C>
where
    C: CheckerRequestContext + ?Sized,
{
    fn scoped_exit(
        &self,
        scope: StorageScope,
        exit: AnyBoundNodeId,
        preserves_exit_value: bool,
    ) -> Option<(
        bray_bound_tree::BoundExpressionId,
        bray_bound_tree::BoundCallResult,
        Option<[ScopedResultTransfer; 2]>,
    )> {
        let expression = *self.scoped_uses.get(&scope)?;

        let Some(bray_bound_tree::SemanticSelection::ScopedUse(scoped)) = self
            .selections()
            .and_then(|selections| selections.expression(expression))
        else {
            panic!("scoped cleanup inventory retains its selected exit");
        };

        let result = scoped.exit_result();
        let invocation = bray_bound_tree::BoundExecutionSite::ScopedExit(expression);
        let storage = self.checked_storage().expect("scoped cleanup retains its storage plan");

        let transfers = storage.invocation_result(invocation).map(|access| {
            let source = storage.root_identity(access).expect("scoped exit owns its raw result slot");

            let (_, representation) = self.request().available_compiler_known_symbols()
                .result_payloads(self.request().semantic_values(), result.completion_type())
                .expect("scoped exit result has its checked representation");

            let destination = if preserves_exit_value && bray_bound_tree::scoped_exit_completes_result(self.request().view(), expression, Some(exit)) {
                let slot = storage.value_slot(expression).expect("with result replacement retains its private slot");

                let Some(bray_bound_tree::StorageBinding::Identity(displaced)) = storage.binding(bray_bound_tree::StorageBindingTarget::ScopedResultReplacement(expression)) else {
                    panic!("fallible scoped exit retains its displaced owner");
                };

                ScopedResultDestination::Region { slot, displaced: Some(displaced) }
            } else {
                ScopedResultDestination::Incident { pending: preserves_exit_value }
            };

            [
                ScopedResultTransfer {
                    invocation, source: Some(source), destination: ScopedResultDestination::Discard,
                    projection: bray_bound_tree::StorageProjection::ActiveUnionPayloadField {
                        variant: representation.success_variant(), field: representation.success_field(),
                    },
                },
                ScopedResultTransfer {
                    invocation, source: Some(source), destination,
                    projection: bray_bound_tree::StorageProjection::ActiveUnionPayloadField {
                        variant: representation.error_variant(), field: representation.error_field(),
                    },
                },
            ]
        });

        Some((expression, result, transfers))
    }

    pub(super) fn push_scope_exit(
        &mut self,
        current: AnalysisBlockId,
        scope: StorageScope,
        exit: AnyBoundNodeId,
        value: Option<StorageIdentityId>,
    ) -> AnalysisBlockId {
        let retained_depth = self.scopes.len();
        let inserted = self.scopes.last() != Some(&scope);

        if inserted {
            self.scopes.push(scope);
        }

        let scope_depth = self.scopes.len() - 1;

        let resolved = self.resolve_scopes(current, scope_depth, exit, true, value);

        self.scopes.truncate(retained_depth);

        resolved
    }

    pub(super) fn push_exit(
        &mut self,
        block: AnalysisBlockId,
        kind: AnalysisExitKind,
        exit: AnyBoundNodeId,
    ) {
        let block = if let Some(failure) = kind.failure_kind() {
            let cleanup = self.push_block();

            self.push_edge(block, cleanup, AnalysisEdgeKind::Sequential, Some(AnalysisRefinement::BeginCleanup(failure)));

            cleanup
        } else {
            block
        };

        let preserves_exit_value = matches!(
            kind,
            AnalysisExitKind::Return
                | AnalysisExitKind::ResultErrorPropagation
                | AnalysisExitKind::Yield
                | AnalysisExitKind::NormalFallthrough
        );

        if kind == AnalysisExitKind::Panic
            && let Some(catch) = self.catches.last().copied()
        {
            let block = self.resolve_scopes(block, catch.scope_depth, exit, false, None);

            self.push_edge(
                block,
                catch.target,
                AnalysisEdgeKind::Catch,
                Some(super::model::AnalysisRefinement::CaughtResult {
                    expression: catch.expression,
                    outcome: super::model::AnalysisCaughtResult::Panicked,
                }),
            );

            return;
        }

        let value = match kind {
            AnalysisExitKind::Return | AnalysisExitKind::ResultErrorPropagation | AnalysisExitKind::NormalFallthrough => {
                self.checked_storage().and_then(|storage| match storage.binding(bray_bound_tree::StorageBindingTarget::Result) {
                    Some(bray_bound_tree::StorageBinding::Identity(result)) => Some(result),
                    _ => None,
                })
            }
            _ => None,
        };

        let block = match kind {
            AnalysisExitKind::Divergence => block,
            _ => self.resolve_scopes(block, 0, exit, preserves_exit_value, value),
        };

        self.storage.push_exit(block, kind, exit);
    }

    pub(super) fn push_storage_admission(
        &mut self,
        block: AnalysisBlockId,
        exit: AnyBoundNodeId,
        failure: Option<AnalysisRefinement>,
    ) -> AnalysisBlockId {
        let rejected = self.push_block();
        let completed = self.push_block();

        // Rejection belongs to the caller before any later input transfer.
        self.push_edge(
            block,
            rejected,
            AnalysisEdgeKind::Panic,
            failure,
        );

        self.push_edge(block, completed, AnalysisEdgeKind::Sequential, None);
        self.push_exit(rejected, AnalysisExitKind::Panic, exit);

        completed
    }

    pub(super) fn push_dependency_failures(
        &mut self,
        block: AnalysisBlockId,
        invocation: bray_bound_tree::BoundExecutionSite,
    ) {
        let failure = self.push_block();

        self.push_edge(
            block,
            failure,
            AnalysisEdgeKind::Sequential,
            Some(super::model::AnalysisRefinement::InvocationFailure(
                invocation,
                bray_symbols::DependencyFailureKind::Panic,
            )),
        );

        self.push_exit(failure, AnalysisExitKind::Panic, invocation.node());

        if self.invocation_may_cancel(invocation) {
            let cancelled = self.push_block();

            self.push_edge(
                block,
                cancelled,
                AnalysisEdgeKind::Sequential,
                Some(super::model::AnalysisRefinement::InvocationFailure(
                    invocation,
                    bray_symbols::DependencyFailureKind::Cancellation,
                )),
            );

            self.push_exit(cancelled, AnalysisExitKind::Cancellation, invocation.node());
        }
    }

    fn invocation_may_cancel(&self, invocation: bray_bound_tree::BoundExecutionSite) -> bool {
        let bray_bound_tree::BoundExecutionSite::Node(AnyBoundNodeId::Expression(expression)) =
            invocation
        else {
            return true;
        };

        let Some(bray_bound_tree::SemanticSelection::Call(call)) = self
            .selections()
            .and_then(|selections| selections.expression(expression))
        else {
            return true;
        };

        call.may_enter_current_run_cancellation()
    }

    fn continue_scope_failure(
        &mut self,
        scope_index: usize,
        broadcast_depth: usize,
        exit: AnyBoundNodeId,
        abandoned: Option<StorageScope>,
        kind: DependencyFailureKind,
        tails: &mut FailureTails,
    ) -> AnalysisBlockId {
        let catch = (kind == DependencyFailureKind::Panic).then(|| {
            self
                .catches
                .iter()
                .rev()
                .find(|catch| catch.scope_depth <= scope_index)
                .copied()
        }).flatten();

        let depth = catch.map_or(0, |catch| catch.scope_depth);
        let entry = self.push_block();
        let active = self.push_block();

        self.push_edge(entry, active, AnalysisEdgeKind::Sequential, Some(AnalysisRefinement::BeginCleanup(kind)));

        let broadcast = abandoned.map_or(active, |scope| self.storage.push_scope_broadcast(active, scope, exit, false));
        let broadcast = self.resolve_scope_broadcasts(broadcast, depth..broadcast_depth, exit, false);
        let lifecycle = abandoned.map_or(broadcast, |scope| self.storage.push_scope_lifecycle(broadcast, scope, exit, false, None, None));
        let remaining = self.storage.push_scope_lifecycle(lifecycle, self.scopes[scope_index], exit, false, None, None);

        let tails = tails.entry((catch.map(|catch| catch.target), kind)).or_insert_with(|| {
            let start = self.push_block();

            if let Some(catch) = catch {
                self.push_edge(start, catch.target, AnalysisEdgeKind::Catch,
                    Some(AnalysisRefinement::CaughtResult { expression: catch.expression, outcome: super::model::AnalysisCaughtResult::Panicked }));
            } else {
                let kind = match kind {
                    DependencyFailureKind::Panic => AnalysisExitKind::Panic,
                    DependencyFailureKind::Cancellation => AnalysisExitKind::Cancellation,
                };

                self.storage.push_exit(start, kind, exit);
            }

            vec![start]
        });

        while tails.len() <= scope_index - depth {
            let scope = self.scopes[depth + tails.len() - 1];
            let start = self.push_block();
            let scoped = self.scoped_exit(scope, exit, false);
            let resolved = self.storage.push_scope_lifecycle(start, scope, exit, false, scoped, None);

            self.push_edge(resolved, tails[tails.len() - 1], AnalysisEdgeKind::Sequential, None);
            tails.push(start);
        }

        self.push_edge(remaining, tails[scope_index - depth], AnalysisEdgeKind::Sequential, None);

        entry
    }

    pub(super) fn resolve_scopes(
        &mut self,
        current: AnalysisBlockId,
        retained_depth: usize,
        exit: AnyBoundNodeId,
        preserves_exit_value: bool,
        value: Option<StorageIdentityId>,
    ) -> AnalysisBlockId {
        let current = self.resolve_scope_broadcasts(current, retained_depth..self.scopes.len(), exit, preserves_exit_value);

        self.resolve_scope_lifecycles(current, retained_depth..self.scopes.len(), exit, preserves_exit_value, value)
    }

    fn resolve_scope_broadcasts(
        &mut self,
        mut current: AnalysisBlockId,
        scopes: std::ops::Range<usize>,
        exit: AnyBoundNodeId,
        preserves_exit_value: bool,
    ) -> AnalysisBlockId {
        for index in scopes.rev() {
            current = self.storage.push_scope_broadcast(
                current,
                self.scopes[index],
                exit,
                preserves_exit_value,
            );
        }

        current
    }

    fn resolve_scope_lifecycles(
        &mut self,
        mut current: AnalysisBlockId,
        scopes: std::ops::Range<usize>,
        exit: AnyBoundNodeId,
        preserves_exit_value: bool,
        value: Option<StorageIdentityId>,
    ) -> AnalysisBlockId {
        let broadcast_depth = scopes.start;
        let mut tails = FailureTails::new();
        let abandoned = value.map(|value| StorageScope::AbandonedTransfer { transfer: exit, value });

        for index in scopes.rev() {
            let scope = self.scopes[index];
            let scoped = self.scoped_exit(scope, exit, preserves_exit_value);

            let local_cleanup = self.cleanup_requirements.as_ref()
                .is_some_and(|requirements| requirements.scope_candidates(scope).next().is_some());

            let failures = (preserves_exit_value && (scoped.is_some() || local_cleanup)).then(|| {
                super::assembly::ScopeFailureTargets {
                    panic: self.continue_scope_failure(index, broadcast_depth, exit, abandoned, DependencyFailureKind::Panic, &mut tails),
                    cancellation: self.continue_scope_failure(index, broadcast_depth, exit, abandoned, DependencyFailureKind::Cancellation, &mut tails),
                    local_cleanup,
                }
            });

            current = self.storage.push_scope_lifecycle(
                current,
                scope,
                exit,
                preserves_exit_value,
                scoped,
                failures,
            );
        }

        current
    }

}
