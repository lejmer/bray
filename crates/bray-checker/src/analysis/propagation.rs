use bray_bound_tree::{BoundExpressionId, BoundStructuredExpression};
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
    pub(super) fn build_propagation(
        &mut self,
        expression: BoundExpressionId,
        current: AnalysisBlockId,
        nullable: bool,
    ) -> Option<Option<AnalysisBlockId>> {
        let success = self.push_block();
        let failure = self.push_block();

        let (success_kind, failure_kind, success_refinement, failure_refinement) = if nullable {
            (
                AnalysisEdgeKind::NullablePresent,
                AnalysisEdgeKind::NullableAbsent,
                Some(AnalysisRefinement::NullablePresence {
                    expression,
                    is_present: true,
                }),
                Some(AnalysisRefinement::NullablePresence {
                    expression,
                    is_present: false,
                }),
            )
        } else {
            (
                AnalysisEdgeKind::ResultSuccess,
                AnalysisEdgeKind::ResultErrorPropagation,
                None,
                None,
            )
        };

        self.push_edge(current, success, success_kind, success_refinement);
        self.push_edge(current, failure, failure_kind, failure_refinement);
        self.push_exit(failure, AnalysisExitKind::ResultErrorPropagation);

        Some(Some(success))
    }

    pub(super) fn build_result_or_run_result_propagation(
        &mut self,
        id: BoundExpressionId,
        expression: &BoundStructuredExpression,
        current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        match self.propagation_operand_role(expression) {
            Some(RepresentationRole::Result) => self.build_propagation(id, current, false),
            Some(RepresentationRole::RunResult) => {
                Some(Some(self.build_run_result_propagation(current)))
            }
            _ => {
                self.push_recovery(current, id.into());

                Some(Some(current))
            }
        }
    }

    fn propagation_operand_role(
        &self,
        expression: &BoundStructuredExpression,
    ) -> Option<RepresentationRole> {
        let operand = expression.operands().first().copied()?;

        let operand_type = self
            .checked_storage()
            .and_then(|storage| {
                storage
                    .expression_plans(operand)
                    .find_map(|plan| storage.access(plan.access()))
                    .map(|access| access.reached_type())
            })
            .or_else(|| self.view().expression(operand)?.ty())?;

        type_representation(self.request(), operand_type).ok()?
    }

    fn build_run_result_propagation(&mut self, current: AnalysisBlockId) -> AnalysisBlockId {
        let completed = self.push_block();
        let panicked = self.push_block();
        let cancelled = self.push_block();

        self.push_edge(
            current,
            completed,
            AnalysisEdgeKind::RunResultCompleted,
            None,
        );

        self.push_edge(current, panicked, AnalysisEdgeKind::RunResultPanicked, None);

        self.push_edge(
            current,
            cancelled,
            AnalysisEdgeKind::RunResultCancelled,
            None,
        );

        self.push_exit(panicked, AnalysisExitKind::Panic);
        self.push_exit(cancelled, AnalysisExitKind::Cancellation);

        completed
    }
}
