use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{CheckedExpressionSemantics, CheckedMemoryOperations, StoragePlan};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::ExecutionProperty;

use super::super::model::AnalysisExitKind;
use crate::{CheckerOutcome, CheckerQueryError, CheckerRequestContext, CheckerUnitView};

/// Body-local unwind behavior with the selected dependencies that can still unwind.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct UnwindCandidate {
    may_unwind: bool,
    entry_conditions: BTreeSet<crate::ExecutionCondition>,
    dependencies: BTreeSet<crate::ExecutionDependencyTarget>,
    calls: BTreeMap<bray_bound_tree::BoundExpressionId, BTreeSet<crate::ExecutionDependencyTarget>>,
}

impl UnwindCandidate {
    /// Execution-context conditions established at entry and unchanged by synchronous calls.
    pub fn entry_conditions(&self) -> &BTreeSet<crate::ExecutionCondition> {
        &self.entry_conditions
    }

    /// Whether local operations can unwind independently of the retained dependencies.
    pub const fn may_unwind(&self) -> bool {
        self.may_unwind
    }

    /// Locally non-unwinding call sites and all of their invocation and conversion dependencies.
    pub fn calls(
        &self,
    ) -> &BTreeMap<bray_bound_tree::BoundExpressionId, BTreeSet<crate::ExecutionDependencyTarget>>
    {
        &self.calls
    }

    /// Call targets whose unwind behavior contributes to this body.
    pub fn dependencies(&self) -> &BTreeSet<crate::ExecutionDependencyTarget> {
        &self.dependencies
    }
}

/// Infers abnormal completion from checked operations before ownership-flow analysis.
/// Termination is deliberately independent: a loop may run forever without unwinding.
pub fn check_unwind_candidate<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    prepared: &crate::PreparedBodyAnalysis,
    expressions: &CheckedExpressionSemantics,
    storage: &StoragePlan,
    memory: &CheckedMemoryOperations,
    entry_conditions: BTreeSet<crate::ExecutionCondition>,
) -> CheckerOutcome<UnwindCandidate, C::UpstreamError> {
    let graph = &prepared.graph;

    let excluded = graph
        .edges()
        .iter()
        .filter(|edge| edge.is_dependency_failure())
        .map(|edge| edge.id())
        .collect();

    let Some(reachable) =
        super::super::reachability::analyze_reachability_excluding(graph, request, &excluded)
    else {
        return CheckerOutcome::Cancelled;
    };

    let mut may_unwind = graph
        .exits()
        .iter()
        .filter(|exit| reachable.is_block_reachable(exit.block()))
        .any(|exit| {
            matches!(
                exit.kind(),
                AnalysisExitKind::Panic
                    | AnalysisExitKind::Cancellation
                    | AnalysisExitKind::Recovery
            )
        });

    let mut dependencies = Vec::new();
    let mut diagnostics = DiagnosticBag::new();
    let mut calls = BTreeMap::new();

    for (expression, _) in request.unit().tree().expressions() {
        if request.is_cancelled() {
            return CheckerOutcome::Cancelled;
        }

        // Reuse the operation rules for normal completion without imposing total's termination rule.
        let first_dependency = dependencies.len();

        let valid = super::operation::check_expression(
            request,
            expression,
            super::operation::ExecutionGoal::NoUnwind,
            expressions.selections(),
            storage,
            memory,
            false,
            &mut dependencies,
        );

        may_unwind |= !valid;

        if valid
            && matches!(
                expressions.selections().expression(expression),
                Some(
                    bray_bound_tree::SemanticSelection::Call(_)
                        | bray_bound_tree::SemanticSelection::Operation(
                            bray_bound_tree::SelectedOperation::Operator { .. }
                                | bray_bound_tree::SelectedOperation::CompoundAssignment(_)
                                | bray_bound_tree::SelectedOperation::Index { .. }
                                | bray_bound_tree::SelectedOperation::Conversion(_)
                        )
                )
            )
        {
            calls.insert(
                expression,
                dependencies[first_dependency..]
                    .iter()
                    .map(|dependency| dependency.target)
                    .collect(),
            );
        }
    }

    for operation in graph.operations() {
        if let super::super::model::AnalysisOperationKind::Call { invocation, .. } =
            operation.kind()
            && matches!(
                invocation,
                bray_bound_tree::BoundExecutionSite::ScopedEnter(_)
                    | bray_bound_tree::BoundExecutionSite::ScopedExit(_)
            )
        {
            super::operation::collect_scoped_dependency(
                expressions.selections(),
                invocation,
                ExecutionProperty::Total,
                &mut dependencies,
            );
        }
    }

    // Before move analysis, every owned storage type may require disposal. This upper bound
    // includes implicit cleanup and replacement, and never assumes a value was transferred.
    let types = storage
        .identity_entries()
        .filter_map(|(identity, _)| storage.storage_type(identity))
        .collect::<BTreeSet<_>>();

    for ty in types {
        let cleanup = match crate::asynchronous::execution_cleanup_dependencies(
            request,
            ty,
            ExecutionProperty::Total,
            crate::asynchronous::ExecutionCleanupMode::Disposal,
            request.unit().root().into(),
        ) {
            Ok(cleanup) => cleanup,
            Err(CheckerQueryError::Cancelled) => return CheckerOutcome::Cancelled,
            Err(CheckerQueryError::Upstream(error)) => {
                return CheckerOutcome::UpstreamFailure(error);
            }
        };

        let (cleanup, owned) = cleanup.into_parts();

        diagnostics.add_range(owned);

        match cleanup {
            Some(cleanup) => dependencies.extend(cleanup),
            None => may_unwind = true,
        }
    }

    CheckerOutcome::complete(
        UnwindCandidate {
            may_unwind,
            entry_conditions,
            calls,
            dependencies: dependencies
                .into_iter()
                .map(|dependency| dependency.target)
                .collect(),
        },
        diagnostics,
    )
}
