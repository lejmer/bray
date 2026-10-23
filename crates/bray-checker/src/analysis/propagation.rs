use bray_bound_tree::{
    BoundExpressionId, BoundStructuredExpression, SelectedPropagation, SemanticSelection,
};
use bray_compiler_known::RepresentationRole;

use crate::CheckerRequestContext;
use crate::representation::type_representation;

use super::build::ControlFlowGraphBuilder;
use super::id::AnalysisBlockId;
use super::model::{AnalysisEdgeKind, AnalysisExitKind, AnalysisRefinement};

impl<C> ControlFlowGraphBuilder<'_, C>
where
    C: CheckerRequestContext + ?Sized,
{
    pub(super) fn build_result_propagation(
        &mut self,
        expression: BoundExpressionId,
        current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        let success = self.push_block();
        let failure = self.push_block();

        self.push_edge(
            current,
            success,
            AnalysisEdgeKind::ResultSuccess,
            Some(AnalysisRefinement::PropagationOutcome {
                expression,
                succeeded: true,
            }),
        );

        self.push_edge(
            current,
            failure,
            AnalysisEdgeKind::ResultErrorPropagation,
            Some(AnalysisRefinement::PropagationOutcome {
                expression,
                succeeded: false,
            }),
        );

        self.finish_propagation(failure, expression);

        self.push_bound(success, expression.into());

        Some(Some(success))
    }

    pub(super) fn build_nullable_propagation(
        &mut self,
        expression: BoundExpressionId,
        current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        if self
            .selections()
            .is_some_and(|selections| selections.expression(expression).is_none())
        {
            self.push_recovery(current, expression.into());

            return Some(Some(current));
        }

        let success = self.push_block();
        let failure = self.push_block();

        self.push_edge(
            current,
            success,
            AnalysisEdgeKind::NullablePresent,
            Some(AnalysisRefinement::PropagationOutcome {
                expression,
                succeeded: true,
            }),
        );

        self.push_edge(
            current,
            failure,
            AnalysisEdgeKind::NullableAbsent,
            Some(AnalysisRefinement::PropagationOutcome {
                expression,
                succeeded: false,
            }),
        );

        self.push_bound(success, expression.into());

        self.finish_propagation(failure, expression);

        Some(Some(success))
    }

    fn finish_propagation(&mut self, failure: AnalysisBlockId, expression: BoundExpressionId) {
        if let Some(SemanticSelection::Propagation(selection)) = self
            .selections()
            .and_then(|selections| selections.expression(expression))
            && let Some(bray_bound_tree::SelectedPropagationBoundary::YieldRegion(target)) =
                selection.boundary()
        {
            let continuation = self.push_control_transfer(
                expression,
                failure,
                bray_bound_tree::BoundControlTransferKind::Yield,
                Some(target),
            );

            assert!(
                continuation.is_none(),
                "checked propagation targets a single-yield result region"
            );
        } else {
            self.push_exit(
                failure,
                AnalysisExitKind::ResultErrorPropagation,
                expression.into(),
            );
        }
    }

    pub(super) fn build_result_or_run_result_propagation(
        &mut self,
        id: BoundExpressionId,
        expression: &BoundStructuredExpression,
        current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        match self.propagation_role(id, expression) {
            Some(RepresentationRole::Result) => self.build_result_propagation(id, current),
            Some(RepresentationRole::RunResult) => {
                Some(Some(self.build_run_result_propagation(id, current)))
            }
            _ => {
                self.push_recovery(current, id.into());

                Some(Some(current))
            }
        }
    }

    fn propagation_role(
        &self,
        id: BoundExpressionId,
        expression: &BoundStructuredExpression,
    ) -> Option<RepresentationRole> {
        if let Some(selections) = self.selections() {
            let selection = match selections.expression(id) {
                Some(SemanticSelection::Propagation(selection)) => selection,
                None => return None,
                Some(unexpected) => panic!(
                    "propagation {id:?} retains a propagation selection, found {unexpected:?}"
                ),
            };

            return match selection {
                SelectedPropagation::Nullable { .. } => None,
                SelectedPropagation::Result { .. } => Some(RepresentationRole::Result),
                SelectedPropagation::CurrentRun { .. } => Some(RepresentationRole::RunResult),
            };
        }

        // Before semantic selection, provisional graphs can use the bound operand type.
        // Completed selection tables must never infer an outcome for a rejected operation.
        let Some(operand) = expression.operands().first().copied() else {
            return None;
        };

        let Some(operand_type) = self
            .checked_storage()
            .and_then(|storage| {
                storage
                    .node_plans((operand).into())
                    .find_map(|plan| storage.access(plan.access()))
                    .map(|access| access.reached_type())
            })
            .or_else(|| self.view().expression(operand)?.ty())
        else {
            return None;
        };

        type_representation(self.request(), operand_type)
    }

    fn build_run_result_propagation(
        &mut self,
        expression: BoundExpressionId,
        current: AnalysisBlockId,
    ) -> AnalysisBlockId {
        let completed = self.push_block();
        let panicked = self.push_block();
        let cancelled = self.push_block();

        self.push_edge(
            current,
            completed,
            AnalysisEdgeKind::RunResultCompleted,
            Some(AnalysisRefinement::PropagationOutcome {
                expression,
                succeeded: true,
            }),
        );

        self.push_edge(
            current,
            panicked,
            AnalysisEdgeKind::RunResultPanicked,
            Some(AnalysisRefinement::PropagationOutcome {
                expression,
                succeeded: false,
            }),
        );

        self.push_edge(
            current,
            cancelled,
            AnalysisEdgeKind::RunResultCancelled,
            Some(AnalysisRefinement::PropagationOutcome {
                expression,
                succeeded: false,
            }),
        );

        self.push_bound(completed, expression.into());

        self.push_exit(panicked, AnalysisExitKind::Panic, expression.into());

        self.push_exit(cancelled, AnalysisExitKind::Cancellation, expression.into());

        completed
    }
}
