use bray_bound_tree::{
    BoundExpression, BoundExpressionId, ConversionTarget, SelectedPropagation,
    SelectedPropagationBoundary, SemanticSelection,
};
use bray_symbols::{
    ConstantField, ConstantProjection, ConstantProjectionKind, ConstantTermData, ConstantTermId,
    ConstantValueData, ConstantValueKind, SemanticValueStoreError,
};

use super::flow::{DomainState, GuaranteeDomain};

impl GuaranteeDomain<'_> {
    pub(super) fn propagation_subject(
        &self,
        state: &DomainState,
        expression: BoundExpressionId,
    ) -> Result<Option<ConstantTermId>, SemanticValueStoreError> {
        let Some(BoundExpression::Structured(structured)) = self.view.expression(expression) else {
            return Ok(None);
        };

        let [subject] = structured.operands() else {
            return Ok(None);
        };

        self.current_expression(state, *subject)
    }

    pub(super) fn propagated_return(
        &self,
        state: &DomainState,
        expression: BoundExpressionId,
    ) -> Result<Option<ConstantTermId>, SemanticValueStoreError> {
        match self.selections.expression(expression) {
            Some(SemanticSelection::Propagation(SelectedPropagation::Nullable {
                boundary: SelectedPropagationBoundary::Callable,
                result_type,
            })) => {
                let value = self.values.intern_constant_value(ConstantValueData::new(
                    *result_type,
                    ConstantValueKind::NullableAbsent,
                ))?;

                self.values
                    .intern_constant_term(ConstantTermData::Value(value))
                    .map(Some)
            }
            Some(SemanticSelection::Propagation(SelectedPropagation::Result {
                boundary: SelectedPropagationBoundary::Callable,
                error_conversion,
                ..
            })) if matches!(error_conversion.target(), ConversionTarget::Identity) => {
                let Some(representation) = self.result_representation else {
                    return Ok(None);
                };

                let payload = match self.propagation_subject(state, expression)? {
                    Some(subject) => {
                        self.values
                            .intern_constant_term(ConstantTermData::Projection(
                                ConstantProjection::new(
                                    subject,
                                    ConstantProjectionKind::UnionPayloadField(
                                        representation.error_field(),
                                    ),
                                ),
                            ))?
                    }
                    None => {
                        // Cleanup can invalidate payload observations while preserving the
                        // result case selected by this propagation edge.
                        let Some(payload) = self.input.propagation_payload(expression) else {
                            return Ok(None);
                        };

                        self.values
                            .intern_constant_term(ConstantTermData::CallableArgument(payload))?
                    }
                };

                self.values
                    .intern_constant_term(ConstantTermData::union(
                        representation.error_variant(),
                        [ConstantField::new(representation.error_field(), payload)],
                    ))
                    .map(Some)
            }
            _ => Ok(None),
        }
    }
}
