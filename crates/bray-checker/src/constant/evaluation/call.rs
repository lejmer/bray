use bray_bound_tree::BoundExpressionId;
use bray_diagnostics::DiagnosticKind;
use bray_symbols::{
    CallableInstanceData, ConstantTermData, ConstantTermId, ImplementationInstanceId, TypeId,
};

use crate::{
    CheckerFactError, CheckerInfrastructureError, CheckerRequestContext, ConstantCallRequest,
    ConstantCallResolution,
};

use super::engine::Evaluator;
use super::support::EvaluationFailure;

impl<'view, 'input, 'types, C> Evaluator<'view, 'input, 'types, C>
where
    C: CheckerRequestContext + ?Sized,
{
    pub(super) fn evaluate_call(
        &mut self,
        expression: BoundExpressionId,
        callable: CallableInstanceData,
        selected_implementation: Option<ImplementationInstanceId>,
        arguments: &[BoundExpressionId],
        result_type: TypeId,
    ) -> Result<ConstantTermId, EvaluationFailure> {
        let arguments = arguments
            .iter()
            .copied()
            .map(|argument| self.evaluate(argument))
            .collect::<Result<Vec<_>, _>>()?;

        if self.retain_open_terms() {
            let Some(resolver) = self.input.call_resolver() else {
                return Err(EvaluationFailure::invalid_expression(expression));
            };

            match resolver.is_constant_callable(callable) {
                Ok(true) => {}
                Ok(false) => return Err(EvaluationFailure::invalid_expression(expression)),
                Err(CheckerFactError::Cancelled) => return Err(EvaluationFailure::Cancelled),
                Err(CheckerFactError::Infrastructure(error)) => {
                    return Err(EvaluationFailure::Infrastructure(error));
                }
            }

            let callable = self
                .request
                .semantic_values()
                .intern_callable_instance(callable)
                .map_err(|_| {
                    EvaluationFailure::Infrastructure(
                        CheckerInfrastructureError::SemanticValueUnavailable,
                    )
                })?;

            return self.intern_term(ConstantTermData::call(
                callable,
                selected_implementation,
                arguments,
            ));
        }

        let values = arguments
            .iter()
            .copied()
            .map(|argument| self.closed_value(argument, expression))
            .collect::<Result<Vec<_>, _>>()?;

        let value = self.evaluate_call_values(
            expression,
            callable,
            selected_implementation,
            values,
            result_type,
        )?;

        self.intern_term(ConstantTermData::Value(value))
    }

    pub(super) fn evaluate_call_values(
        &mut self,
        expression: BoundExpressionId,
        callable: CallableInstanceData,
        selected_implementation: Option<ImplementationInstanceId>,
        arguments: impl IntoIterator<Item = bray_symbols::ConstantValueId>,
        result_type: TypeId,
    ) -> Result<bray_symbols::ConstantValueId, EvaluationFailure> {
        let limits = self.budget.remaining_limits(self.input.limits());

        let Some(limits) = limits.nested_call() else {
            return Err(EvaluationFailure::Source {
                expression,
                kind: DiagnosticKind::CheckingConstantEvaluationStepLimitExceeded,
            });
        };

        let request = ConstantCallRequest::new(
            callable,
            selected_implementation,
            arguments,
            result_type,
            limits,
        );

        let Some(resolver) = self.input.call_resolver() else {
            return Err(EvaluationFailure::invalid_expression(expression));
        };

        match resolver.resolve(&request) {
            Ok(ConstantCallResolution::Evaluated(result)) => {
                self.budget
                    .charge_usage(expression, result.value().usage())?;

                let value = result.value().value();
                let value_data = self.constant_value(value)?;

                if value_data.ty() != result_type {
                    return Err(EvaluationFailure::invalid_input());
                }

                self.diagnostics = self.diagnostics.merged(result.diagnostics());

                Ok(value)
            }
            Ok(ConstantCallResolution::Cycle) => Err(EvaluationFailure::Source {
                expression,
                kind: DiagnosticKind::CheckingCyclicConstantDefinition,
            }),
            Ok(ConstantCallResolution::Ineligible(diagnostics)) => {
                if diagnostics.has_errors() {
                    self.diagnostics = self.diagnostics.merged(&diagnostics);

                    return self
                        .recovery_value(result_type)
                        .map_err(EvaluationFailure::Infrastructure);
                }

                Err(EvaluationFailure::invalid_expression(expression))
            }
            Err(CheckerFactError::Cancelled) => Err(EvaluationFailure::Cancelled),
            Err(CheckerFactError::Infrastructure(error)) => {
                Err(EvaluationFailure::Infrastructure(error))
            }
        }
    }
}
