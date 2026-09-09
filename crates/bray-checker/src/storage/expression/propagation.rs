use bray_bound_tree::{
    BoundExpressionId, BoundOperationPoint, SelectedPropagation, SemanticSelection,
    StorageAccessId, StorageAccessPurpose, StorageProjection,
};
use bray_compiler_known::RepresentationRole;
use bray_symbols::{NamedTypeSymbolId, StructSymbolId};

use super::super::plan::{PlanError, Planner};
use crate::{CheckerInfrastructureError, CheckerRequestContext};

impl<C> Planner<'_, C>
where
    C: CheckerRequestContext + ?Sized,
{
    pub(super) fn plan_propagation(
        &mut self,
        expression: BoundExpressionId,
        operands: &[BoundExpressionId],
    ) -> Result<StorageAccessId, PlanError<C::UpstreamError>> {
        let [operand] = operands else {
            return self.recovery_access(expression);
        };

        let base = self.plan_expression(*operand, Some(StorageAccessPurpose::Read))?;
        let available = self.request.available_compiler_known_symbols();
        let values = self.request.semantic_values();

        let (success_variant, success_field, failure_variant, failure_field, failure_type) =
            match self.selections.expression(expression) {
                Some(SemanticSelection::Propagation(SelectedPropagation::Result { .. })) => {
                    let representation = available
                        .result_representation()
                        .ok_or(CheckerInfrastructureError::InvalidStoragePlan)?;

                    let [_, error] = available
                        .representation_type_arguments::<2>(
                            values,
                            RepresentationRole::Result,
                            self.expression_type(*operand)?.ty(),
                        )
                        .map_err(CheckerInfrastructureError::SemanticValueStore)?
                        .ok_or(CheckerInfrastructureError::InvalidStoragePlan)?;

                    (
                        representation.success_variant(),
                        representation.success_field(),
                        representation.error_variant(),
                        representation.error_field(),
                        error,
                    )
                }
                Some(SemanticSelection::Propagation(SelectedPropagation::CurrentRun)) => {
                    let representation = available
                        .run_result_representation()
                        .ok_or(CheckerInfrastructureError::InvalidStoragePlan)?;

                    let report = available
                        .representation_symbol::<StructSymbolId>(RepresentationRole::PanicReport)
                        .ok_or(CheckerInfrastructureError::InvalidStoragePlan)?;

                    let report = values
                        .intern_non_generic_named_type(NamedTypeSymbolId::Struct(report))
                        .map_err(CheckerInfrastructureError::SemanticValueStore)?;

                    (
                        representation.completed_variant(),
                        representation.completed_field(),
                        representation.panicked_variant(),
                        representation.panicked_field(),
                        report,
                    )
                }
                _ => return self.recovery_access(expression),
            };

        let failure = self.project_typed_access(
            expression,
            base,
            StorageProjection::ActiveUnionPayloadField {
                variant: failure_variant,
                field: failure_field,
            },
            failure_type,
        )?;

        self.builder_mut()?
            .plan_access_at(
                BoundOperationPoint::PropagationFailure(expression),
                expression,
                StorageAccessPurpose::ValueTransfer,
                failure,
            )
            .map_err(CheckerInfrastructureError::StoragePlan)?;

        let success = self.project_access(
            expression,
            base,
            Some(StorageProjection::ActiveUnionPayloadField {
                variant: success_variant,
                field: success_field,
            }),
        )?;

        self.record_purpose(expression, Some(StorageAccessPurpose::Projection), success)?;

        Ok(success)
    }
}
