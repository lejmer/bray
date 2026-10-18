use super::super::model::{
    AnalysisCallPhase, AnalysisOperation, AnalysisOperationKind, AnalysisScopeExitPhase,
    ControlFlowGraph,
};
use super::set::RefinementSet;
use super::universe::{MAX_REFINEMENT_CELLS, RefinementUniverse, RefinementUniverseError};
use crate::{CheckerOutcome, CheckerRequestContext, CheckerUnitView};
use bray_bound_tree::{
    AnyBoundNodeId, CheckedExpressionSemantics, CheckedPatterns, CheckedRefinements,
    RefinementOccurrence, StoragePlan,
};
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticKind, DiagnosticLabel,
    DiagnosticLabelKind, DiagnosticRefinementCapacity, DiagnosticRefinementCapacitySurface,
    SeverityKind,
};
use std::collections::BTreeMap;

pub(crate) fn prepare_refinements<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    patterns: &CheckedPatterns,
    expressions: &CheckedExpressionSemantics,
    storage: &StoragePlan,
    graph: &ControlFlowGraph,
    memory: &bray_bound_tree::CheckedMemoryOperations,
) -> CheckerOutcome<RefinementUniverse, C::UpstreamError> {
    match RefinementUniverse::new(
        graph,
        request,
        patterns,
        expressions,
        storage,
        memory,
        storage.copyable_types(),
    ) {
        Ok(universe) => CheckerOutcome::without_diagnostics(universe),
        Err(RefinementUniverseError::Cancelled) => CheckerOutcome::Cancelled,
        Err(RefinementUniverseError::CapacityExceeded(capacity)) => CheckerOutcome::complete(
            {
                let mut universe = RefinementUniverse::default();

                universe.is_recovered = true;

                universe
            },
            DiagnosticBag::single(capacity_diagnostic(
                request.source(request.unit().key().source()).span(),
                capacity,
            )),
        ),
    }
}

pub(crate) fn publish_refinements<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    graph: &ControlFlowGraph,
    universe: &RefinementUniverse,
    occurrences: BTreeMap<AnyBoundNodeId, RefinementSet>,
    invalidations: BTreeMap<
        bray_bound_tree::BoundExecutionSite,
        bray_bound_tree::StorageInvalidation,
    >,
) -> CheckerOutcome<CheckedRefinements, C::UpstreamError> {
    let count = occurrences.values().fold(0usize, |count, set| {
        count
            .checked_add(set.len())
            .expect("published refinement counts fit the host address space")
    });

    let mut diagnostics = DiagnosticBag::new();

    let mut recovered = universe.is_recovered
        || graph
            .operations()
            .iter()
            .any(|operation| matches!(operation.kind(), AnalysisOperationKind::Recovery(_)));

    let occurrences = if count > MAX_REFINEMENT_CELLS {
        let capacity = DiagnosticRefinementCapacity::try_new(
            DiagnosticRefinementCapacitySurface::PublishedRefinements,
            u64::try_from(count).expect("refinement count fits diagnostic representation"),
            u64::try_from(MAX_REFINEMENT_CELLS)
                .expect("configured refinement capacity fits diagnostic representation"),
        )
        .expect("exceeded capacity has a valid diagnostic");

        diagnostics.add(capacity_diagnostic(
            request.source(request.unit().key().source()).span(),
            capacity,
        ));

        recovered = true;

        Vec::new()
    } else {
        occurrences
            .into_iter()
            .filter(|(_, set)| !set.is_empty())
            .map(|(node, set)| {
                RefinementOccurrence::new(node, universe.active_refinements(&set).cloned())
            })
            .collect()
    };

    // Unvisited operations retain their conservative prepared effects. Reachable operations
    // replace them with the effects established on every converged incoming path.
    let mut effects = universe.invalidating_accesses.clone();

    effects.extend(invalidations);

    let refinements = CheckedRefinements::try_new(
        graph.unit(),
        request.view().kind(),
        occurrences,
        effects,
        recovered,
    )
    .expect("composite flow publishes valid refinement occurrences");

    CheckerOutcome::complete(refinements, diagnostics)
}

fn capacity_diagnostic(
    span: bray_source::SourceSpan,
    capacity: DiagnosticRefinementCapacity,
) -> Diagnostic {
    Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::CheckingRefinementCapacityExceeded,
        SeverityKind::Error,
    )
    .with_primary_span(span)
    .with_label(DiagnosticLabel::primary(
        DiagnosticLabelKind::RefinementCapacityExceeded,
        span,
    ))
    .with_arg(DiagnosticArg::refinement_capacity(capacity))
}

pub(crate) fn transfer_operation(
    state: &mut RefinementSet,
    operation: &AnalysisOperation,
    universe: &RefinementUniverse,
    storage: &StoragePlan,
    invalidation: Option<&bray_bound_tree::StorageInvalidation>,
    discriminants: &BTreeMap<bray_bound_tree::StorageIdentityId, Vec<bray_bound_tree::StoragePath>>,
) {
    match operation.kind() {
        AnalysisOperationKind::Result { expression, phase } => {
            if phase == crate::analysis::model::AnalysisResultPhase::Completion {
                universe.finish_operation(state, expression.into());
            }
        }
        AnalysisOperationKind::Recovery(_) => state.clear(),
        AnalysisOperationKind::PatternObservation(_) => {}
        AnalysisOperationKind::ScopeExit {
            phase: AnalysisScopeExitPhase::LifecycleResolution,
            ..
        } => {}
        AnalysisOperationKind::Bound(node) => {
            universe.invalidate_for_operation(state, node, storage, invalidation, discriminants);
            universe.finish_operation(state, node);
        }
        AnalysisOperationKind::Call {
            invocation,
            phase: AnalysisCallPhase::Attempt,
        } => {
            universe.invalidate_for_operation(
                state,
                invocation,
                storage,
                invalidation,
                discriminants,
            );
        }
        AnalysisOperationKind::Call {
            invocation,
            phase: AnalysisCallPhase::Completion,
        } => {
            if let bray_bound_tree::BoundExecutionSite::Node(node) = invocation {
                universe.finish_operation(state, node);
            }
        }
        AnalysisOperationKind::Suspension { occurrence, .. } => {
            universe.invalidate_for_operation(
                state,
                occurrence,
                storage,
                invalidation,
                discriminants,
            );

            if let bray_bound_tree::BoundExecutionSite::Node(node) = occurrence {
                universe.finish_operation(state, node);
            }
        }
        AnalysisOperationKind::TaskOperation { expression, .. } => {
            let node = AnyBoundNodeId::Expression(expression);

            universe.invalidate_for_operation(state, node, storage, invalidation, discriminants);
            universe.finish_operation(state, node);
        }
        AnalysisOperationKind::ScopeExit {
            phase: AnalysisScopeExitPhase::TaskCancellationBroadcast,
            ..
        } => {}
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{
        DiagnosticArg, DiagnosticBag, DiagnosticKind, DiagnosticRefinementCapacity,
        DiagnosticRefinementCapacitySurface, SeverityKind,
    };
    use bray_source::{SourceId, SourceSpan, TextRange, TextSize};
    use bray_testing::assert_goal_state_diagnostic_kind;

    use super::super::universe::{MAX_REFINEMENT_CELLS, MAX_REFINEMENT_REFINEMENTS};
    use super::capacity_diagnostic;

    #[test]
    fn refinement_capacity_recovery_preserves_each_exact_configured_surface() {
        let span = SourceSpan::new(
            SourceId::new(1),
            TextRange::new(TextSize::new(2), TextSize::new(3)),
        );

        let capacities = [
            capacity(
                DiagnosticRefinementCapacitySurface::RefinementEntries,
                MAX_REFINEMENT_REFINEMENTS,
            ),
            capacity(
                DiagnosticRefinementCapacitySurface::RetainedStateCells,
                MAX_REFINEMENT_CELLS,
            ),
            capacity(
                DiagnosticRefinementCapacitySurface::PublishedRefinements,
                MAX_REFINEMENT_CELLS,
            ),
        ];

        for capacity in capacities {
            let diagnostic = capacity_diagnostic(span, capacity);

            assert_eq!(
                diagnostic.kind(),
                DiagnosticKind::CheckingRefinementCapacityExceeded
            );

            assert_eq!(diagnostic.severity(), SeverityKind::Error);
            assert_eq!(diagnostic.primary_span(), Some(span));

            assert_eq!(
                diagnostic.args(),
                [DiagnosticArg::refinement_capacity(capacity)]
            );

            assert_goal_state_diagnostic_kind(
                &DiagnosticBag::single(diagnostic),
                DiagnosticKind::CheckingRefinementCapacityExceeded,
            );
        }
    }

    fn capacity(
        surface: DiagnosticRefinementCapacitySurface,
        maximum: usize,
    ) -> DiagnosticRefinementCapacity {
        let maximum = u64::try_from(maximum)
            .unwrap_or_else(|_| panic!("refinement capacity maximum must fit the protocol"));

        DiagnosticRefinementCapacity::try_new(surface, maximum + 1, maximum)
            .unwrap_or_else(|| panic!("limit plus one must be a capacity violation"))
    }
}
