use bray_bound_tree::{
    BoundExpressionId, BoundStructuredExpression, SelectedPropagation, SemanticSelection,
};
use bray_compiler_known::RepresentationRole;

use crate::representation::type_representation;
use crate::{CheckerInfrastructureError, CheckerRequestContext};

use super::build::ControlFlowGraphBuilder;
use super::id::AnalysisBlockId;
use super::model::{AnalysisEdgeKind, AnalysisExitKind, AnalysisOperationKind, AnalysisRefinement};

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

        let representation = self
            .request()
            .available_compiler_known_symbols()
            .result_representation();

        self.push_edge(
            current,
            success,
            AnalysisEdgeKind::ResultSuccess,
            self.propagation_variant(
                expression,
                representation.map(|value| value.success_variant()),
            ),
        );

        self.push_edge(
            current,
            failure,
            AnalysisEdgeKind::ResultErrorPropagation,
            self.propagation_variant(
                expression,
                representation.map(|value| value.error_variant()),
            ),
        );

        self.storage.push_operation(
            failure,
            AnalysisOperationKind::PropagationFailure(expression),
        );

        self.push_exit(
            failure,
            AnalysisExitKind::ResultErrorPropagation,
            expression.into(),
        );

        self.push_bound(success, expression.into());

        Some(Some(success))
    }

    pub(super) fn build_nullable_propagation(
        &mut self,
        expression: BoundExpressionId,
        subject: BoundExpressionId,
        current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        let success = self.push_block();
        let failure = self.push_block();

        self.push_edge(
            current,
            success,
            AnalysisEdgeKind::NullablePresent,
            Some(AnalysisRefinement::NullablePresence {
                expression: subject,
                is_present: true,
            }),
        );

        self.push_edge(
            current,
            failure,
            AnalysisEdgeKind::NullableAbsent,
            Some(AnalysisRefinement::NullablePresence {
                expression: subject,
                is_present: false,
            }),
        );

        self.push_bound(success, expression.into());

        self.push_exit(
            failure,
            AnalysisExitKind::ResultErrorPropagation,
            expression.into(),
        );

        Some(Some(success))
    }

    pub(super) fn build_result_or_run_result_propagation(
        &mut self,
        id: BoundExpressionId,
        expression: &BoundStructuredExpression,
        current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        match self.propagation_role(id, expression) {
            Ok(Some(RepresentationRole::Result)) => self.build_result_propagation(id, current),
            Ok(Some(RepresentationRole::RunResult)) => {
                Some(Some(self.build_run_result_propagation(id, current)))
            }
            Ok(_) => {
                self.push_recovery(current, id.into());

                Some(Some(current))
            }
            Err(error) => {
                self.record_infrastructure_failure(error);

                None
            }
        }
    }

    fn propagation_role(
        &self,
        id: BoundExpressionId,
        expression: &BoundStructuredExpression,
    ) -> Result<Option<RepresentationRole>, CheckerInfrastructureError> {
        if let Some(SemanticSelection::Propagation(selection)) = self
            .selections()
            .and_then(|selections| selections.expression(id))
        {
            return Ok(match selection {
                SelectedPropagation::Nullable { .. } => None,
                SelectedPropagation::Result { .. } => Some(RepresentationRole::Result),
                SelectedPropagation::CurrentRun => Some(RepresentationRole::RunResult),
            });
        }

        let Some(operand) = expression.operands().first().copied() else {
            return Ok(None);
        };

        let Some(operand_type) = self
            .checked_storage()
            .and_then(|storage| {
                storage
                    .expression_plans(operand)
                    .find_map(|plan| storage.access(plan.access()))
                    .map(|access| access.reached_type())
            })
            .or_else(|| self.view().expression(operand)?.ty())
        else {
            return Ok(None);
        };

        type_representation(self.request(), operand_type)
    }

    pub(super) fn build_run_result_propagation(
        &mut self,
        expression: BoundExpressionId,
        current: AnalysisBlockId,
    ) -> AnalysisBlockId {
        let completed = self.push_block();
        let panicked = self.push_block();
        let cancelled = self.push_block();

        let representation = self
            .request()
            .available_compiler_known_symbols()
            .run_result_representation();

        self.push_edge(
            current,
            completed,
            AnalysisEdgeKind::RunResultCompleted,
            self.propagation_variant(
                expression,
                representation.map(|value| value.completed_variant()),
            ),
        );

        self.push_edge(
            current,
            panicked,
            AnalysisEdgeKind::RunResultPanicked,
            self.propagation_variant(
                expression,
                representation.map(|value| value.panicked_variant()),
            ),
        );

        self.push_edge(
            current,
            cancelled,
            AnalysisEdgeKind::RunResultCancelled,
            self.propagation_variant(
                expression,
                representation.map(|value| value.cancelled_variant()),
            ),
        );

        self.storage.push_operation(
            panicked,
            AnalysisOperationKind::PropagationFailure(expression),
        );

        self.push_exit(panicked, AnalysisExitKind::Panic, expression.into());

        self.push_exit(cancelled, AnalysisExitKind::Cancellation, expression.into());

        self.push_bound(completed, expression.into());

        completed
    }

    fn propagation_variant(
        &self,
        expression: BoundExpressionId,
        variant: Option<bray_symbols::UnionVariantSymbolId>,
    ) -> Option<AnalysisRefinement> {
        let bray_bound_tree::BoundExpression::Structured(expression) =
            self.view().expression(expression)?
        else {
            return None;
        };

        let [subject] = expression.operands() else {
            return None;
        };

        Some(AnalysisRefinement::UnionVariant {
            expression: *subject,
            variant: variant?,
        })
    }
}
