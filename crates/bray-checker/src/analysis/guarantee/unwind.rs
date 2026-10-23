use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    CheckedExpressionSemantics, CheckedMemoryOperations, StoragePlan, UnwindBehavior,
};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::ExecutionProperty;

use super::super::model::AnalysisExitKind;
use crate::{CheckerOutcome, CheckerQueryError, CheckerRequestContext, CheckerUnitView};

/// Body-local unwind behavior with the selected dependencies that can still unwind.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct UnwindCandidate {
    behavior: UnwindBehavior,
    entry_conditions: BTreeSet<crate::ExecutionCondition>,
    dependencies: BTreeSet<crate::ExecutionDependencyTarget>,
    calls: BTreeMap<bray_bound_tree::BoundExpressionId, UnwindCall>,
}

impl UnwindCandidate {
    /// Execution-context conditions established at entry and unchanged by synchronous calls.
    pub fn entry_conditions(&self) -> &BTreeSet<crate::ExecutionCondition> {
        &self.entry_conditions
    }

    /// Local abnormal outcomes, independent of retained dependencies.
    pub const fn behavior(&self) -> UnwindBehavior {
        self.behavior
    }

    /// Selected invocations and their local and dependency outcomes.
    pub fn calls(&self) -> &BTreeMap<bray_bound_tree::BoundExpressionId, UnwindCall> {
        &self.calls
    }

    /// Call targets whose unwind behavior contributes to this body.
    pub fn dependencies(&self) -> &BTreeSet<crate::ExecutionDependencyTarget> {
        &self.dependencies
    }
}

/// The local panic bound and selected dependencies of one invocation.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct UnwindCall {
    locally_non_unwinding: bool,
    dependencies: BTreeSet<crate::ExecutionDependencyTarget>,
}

impl UnwindCall {
    /// Whether local checks and argument conversions exclude abnormal completion.
    pub const fn locally_non_unwinding(&self) -> bool {
        self.locally_non_unwinding
    }

    /// The selected dependencies that may propagate an abnormal outcome.
    pub fn dependencies(&self) -> &BTreeSet<crate::ExecutionDependencyTarget> {
        &self.dependencies
    }
}

/// Infers abnormal completion from checked operations before ownership-flow analysis.
/// Termination is deliberately independent: a loop may run forever without unwinding.
// rust-style: allow(function-too-large, reason = "one reachable graph walk shares edge reachability while collecting direct failure, call, cleanup, and cancellation behavior")
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

    let mut may_unwind = false;
    let mut cancellation = bray_symbols::CurrentRunCancellation::NotEntered;

    for exit in graph
        .exits()
        .iter()
        .filter(|exit| reachable.is_block_reachable(exit.block()))
    {
        match exit.kind() {
            AnalysisExitKind::Cancellation | AnalysisExitKind::Recovery => {
                may_unwind = true;
                cancellation = bray_symbols::CurrentRunCancellation::MayEnter;
            }
            AnalysisExitKind::Panic => may_unwind = true,
            _ => {}
        }
    }

    // Suspension can cancel its current run even when no child body does.
    // Its failure edge is excluded above to separate local and callee unwind.
    if graph.edges().iter().any(|edge| {
        edge.kind() == super::super::model::AnalysisEdgeKind::RunCancellation
            && reachable.is_block_reachable(edge.source())
    }) {
        may_unwind = true;
        cancellation = bray_symbols::CurrentRunCancellation::MayEnter;
    }

    let mut dependencies = Vec::new();
    let mut diagnostics = DiagnosticBag::new();
    let mut calls = BTreeMap::new();

    // A source expression can have several input, invocation and completion operations.
    // Check each reachable expression once, using the prepared graph's established reachability.
    let reachable_expressions = graph
        .blocks()
        .iter()
        .filter(|block| reachable.is_block_reachable(block.id()))
        .flat_map(|block| block.operations())
        .filter_map(|operation| {
            match graph
                .operation(*operation)
                .expect("reachable block retains its operation")
                .kind()
                .node()
            {
                bray_bound_tree::AnyBoundNodeId::Expression(expression) => Some(expression),
                _ => None,
            }
        })
        .collect::<BTreeSet<_>>();

    for expression in reachable_expressions {
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
            None,
            memory,
            false,
            &mut dependencies,
        );

        may_unwind |= !valid;

        if matches!(
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
        ) {
            if !valid {
                match expressions.selections().expression(expression) {
                    Some(bray_bound_tree::SemanticSelection::Call(call))
                        if matches!(
                            call.resolution().result(),
                            bray_bound_tree::BoundCallResult::Immediate(_)
                        ) =>
                    {
                        // A local access check can panic before this invocation.
                        // Retain the invocation's independent cancellation dependency.
                        dependencies.push(crate::ExecutionDependency {
                            target: crate::ExecutionDependencyTarget::selected(call),
                            obligation: crate::ExecutionObligation::Property(
                                ExecutionProperty::Total,
                                None,
                            ),
                            occurrence: expression.into(),
                        });
                    }
                    Some(bray_bound_tree::SemanticSelection::Operation(_))
                        if expressions
                            .selections()
                            .invoked_callable(expression.into())
                            .is_some() =>
                    {
                        super::operation::collect_invocation_dependency(
                            expressions.selections(),
                            expression.into(),
                            ExecutionProperty::Total,
                            &mut dependencies,
                        );
                    }
                    _ => {}
                }
            }

            calls.insert(
                expression,
                UnwindCall {
                    locally_non_unwinding: valid,
                    dependencies: dependencies[first_dependency..]
                        .iter()
                        .map(|dependency| dependency.target)
                        .collect(),
                },
            );
        }
    }

    for operation in graph
        .blocks()
        .iter()
        .filter(|block| reachable.is_block_reachable(block.id()))
        .flat_map(|block| block.operations())
        .map(|operation| {
            graph
                .operation(*operation)
                .expect("reachable block retains its operation")
        })
    {
        if let super::super::model::AnalysisOperationKind::Call {
            invocation: invocation @ bray_bound_tree::BoundExecutionSite::CallInput { .. },
            phase: super::super::model::AnalysisCallPhase::Attempt,
        } = operation.kind()
        {
            may_unwind |= !super::operation::check_input(
                request,
                invocation,
                expressions.selections(),
                storage,
                ExecutionProperty::Total,
                &mut dependencies,
            );
        }

        if let super::super::model::AnalysisOperationKind::Call {
            invocation,
            phase: super::super::model::AnalysisCallPhase::Attempt,
        } = operation.kind()
            && matches!(
                invocation,
                bray_bound_tree::BoundExecutionSite::Iterate(_)
                    | bray_bound_tree::BoundExecutionSite::Next(_)
                    | bray_bound_tree::BoundExecutionSite::ScopedEnter(_)
                    | bray_bound_tree::BoundExecutionSite::ScopedExit(_)
            )
        {
            super::operation::collect_invocation_dependency(
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
            None => {
                may_unwind = true;
                cancellation = bray_symbols::CurrentRunCancellation::MayEnter;
            }
        }
    }

    CheckerOutcome::complete(
        UnwindCandidate {
            behavior: UnwindBehavior::new(may_unwind, cancellation),
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
