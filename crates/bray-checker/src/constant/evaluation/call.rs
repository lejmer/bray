use bray_bound_tree::{
    BoundCallResult, BoundCallableTarget, BoundExpressionId, ConversionTarget, SelectedArgument,
    SelectedConversion, SemanticSelection,
};
use bray_symbols::{
    CallableInstanceData, ConstantTermData, ConstantTermId, ImplementationInstanceId, TypeId,
};

use crate::constant::diagnostic::{ConstantDiagnostic, ConstantLimitKind};

use crate::{
    CheckerInfrastructureError, CheckerQueryError, CheckerRequestContext, ConstantCallRequest,
    ConstantCallResolution,
};

use super::engine::Evaluator;
use super::support::EvaluationFailure;

impl<'view, 'input, 'types, C> Evaluator<'view, 'input, 'types, C>
where
    C: CheckerRequestContext + ?Sized,
{
    pub(super) fn evaluate_selected_call(
        &mut self,
        expression: BoundExpressionId,
        ty: TypeId,
    ) -> Result<ConstantTermId, EvaluationFailure> {
        let Some(SemanticSelection::Call(call)) =
            self.input.semantic_selections().expression(expression)
        else {
            return Err(EvaluationFailure::invalid_expression(expression));
        };

        if !matches!(call.resolution().result(), BoundCallResult::Immediate(_)) {
            return Err(EvaluationFailure::invalid_expression(expression));
        }

        let mut arguments =
            Vec::with_capacity(call.arguments().len() + usize::from(call.receiver().is_some()));

        if let Some(receiver) = call.receiver() {
            arguments.push((None, receiver.expression(), None, receiver.source_type()));
        }

        for argument in call.arguments() {
            let SelectedArgument::Explicit {
                expression,
                ordinal,
                conversion,
                ..
            } = argument
            else {
                return Err(EvaluationFailure::invalid_expression(expression));
            };

            arguments.push((
                Some(*ordinal),
                *expression,
                Some(conversion.clone()),
                conversion.target_type(),
            ));
        }

        arguments.sort_unstable_by_key(|(ordinal, ..)| *ordinal);

        let arguments = arguments
            .into_iter()
            .map(|(_, argument, conversion, ty)| {
                let term = match conversion {
                    Some(conversion) => self.evaluate_selected_conversion(argument, &conversion)?,
                    None => self.evaluate(argument)?,
                };

                Ok((term, ty))
            })
            .collect::<Result<Vec<_>, EvaluationFailure>>()?;

        if let BoundCallableTarget::Predicate(predicate) = call.target() {
            let arguments = arguments
                .into_iter()
                .map(|(argument, _)| argument)
                .collect::<Vec<_>>();

            return self
                .intern_typed_term(ty, ConstantTermData::predicate_call(predicate, arguments));
        }

        let BoundCallableTarget::Declaration(callable) = call.target() else {
            return Err(EvaluationFailure::invalid_expression(expression));
        };

        let witnesses = call.resolution().implementation_witnesses();

        let selected_implementation = match witnesses {
            [] => None,
            [witness] => Some(*witness),
            _ => return Err(EvaluationFailure::invalid_expression(expression)),
        };

        self.evaluate_call_terms(expression, callable, selected_implementation, arguments, ty)
    }

    fn evaluate_selected_conversion(
        &mut self,
        expression: BoundExpressionId,
        conversion: &SelectedConversion,
    ) -> Result<ConstantTermId, EvaluationFailure> {
        if let ConversionTarget::Trait {
            fulfillment,
            witness,
            ..
        } = conversion.target()
        {
            return self.evaluate_call(
                expression,
                *fulfillment,
                Some(*witness),
                std::slice::from_ref(&expression),
                conversion.target_type(),
            );
        }

        let operand = self.evaluate(expression)?;

        self.apply_selected_conversion(expression, conversion, operand)
    }

    pub(super) fn apply_selected_conversion(
        &mut self,
        expression: BoundExpressionId,
        conversion: &SelectedConversion,
        operand: ConstantTermId,
    ) -> Result<ConstantTermId, EvaluationFailure> {
        let ty = conversion.target_type();

        if matches!(conversion.target(), ConversionTarget::Identity) {
            return Ok(operand);
        }

        let Some(operand) = self.term_value(operand)? else {
            return match conversion.target() {
                ConversionTarget::Identity => Ok(operand),
                ConversionTarget::CallableContract => {
                    Err(EvaluationFailure::invalid_expression(expression))
                }
                ConversionTarget::NullablePresent => {
                    self.intern_typed_term(ty, ConstantTermData::NullablePresent(operand))
                }
                ConversionTarget::BuiltInScalar | ConversionTarget::CVariadicPromotion => self
                    .intern_typed_term(
                        ty,
                        ConstantTermData::Conversion {
                            operand,
                            target: conversion.target_type(),
                        },
                    ),
                ConversionTarget::Composite(_) => self.intern_typed_term(
                    ty,
                    ConstantTermData::Conversion {
                        operand,
                        target: conversion.target_type(),
                    },
                ),
                ConversionTarget::Trait { .. } | ConversionTarget::TraitConstraint { .. } => {
                    Err(EvaluationFailure::invalid_expression(expression))
                }
            };
        };

        let value = self.convert_value(expression, conversion, operand)?;
        let data = self.constant_value(value)?;

        if data.ty() != ty {
            return Err(EvaluationFailure::invalid_input());
        }

        self.intern_term(ConstantTermData::Value(value))
    }

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
            .map(|argument| {
                let value = self.evaluate(argument)?;
                let ty = self.expression_type(argument)?;

                Ok((value, ty))
            })
            .collect::<Result<Vec<_>, _>>()?;

        self.evaluate_call_terms(
            expression,
            callable,
            selected_implementation,
            arguments,
            result_type,
        )
    }

    pub(super) fn evaluate_call_terms(
        &mut self,
        expression: BoundExpressionId,
        callable: CallableInstanceData,
        selected_implementation: Option<ImplementationInstanceId>,
        arguments: impl IntoIterator<Item = (ConstantTermId, TypeId)>,
        result_type: TypeId,
    ) -> Result<ConstantTermId, EvaluationFailure> {
        let arguments = arguments
            .into_iter()
            .map(|(argument, ty)| {
                if self.retain_open_terms() {
                    self.type_term(argument, ty)
                } else {
                    Ok(argument)
                }
            })
            .collect::<Result<Vec<_>, _>>()?;

        if self.retain_open_terms() {
            let Some(resolver) = self.input.call_resolver() else {
                return Err(EvaluationFailure::invalid_expression(expression));
            };

            match resolver.is_constant_callable(callable) {
                Ok(true) => {}
                Ok(false) => return Err(EvaluationFailure::invalid_expression(expression)),
                Err(CheckerQueryError::Cancelled) => return Err(EvaluationFailure::Cancelled),
                Err(CheckerQueryError::Infrastructure(error)) => {
                    return Err(EvaluationFailure::Infrastructure(error));
                }
                Err(CheckerQueryError::Upstream(error)) => {
                    self.upstream_failure = Some(error);

                    return Err(EvaluationFailure::Upstream);
                }
            }

            let callable = self
                .request
                .semantic_values()
                .intern_callable_instance(callable)
                .map_err(|error| {
                    EvaluationFailure::Infrastructure(
                        CheckerInfrastructureError::SemanticValueStore(error),
                    )
                })?;

            return self.intern_typed_term(
                result_type,
                ConstantTermData::call(callable, selected_implementation, arguments),
            );
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

    fn merge_call_diagnostics(
        &mut self,
        expression: BoundExpressionId,
        diagnostics: &bray_diagnostics::DiagnosticBag,
    ) -> Result<(), EvaluationFailure> {
        if diagnostics
            .iter()
            .any(|diagnostic| diagnostic.primary_span().is_none())
        {
            let span = crate::diagnostic::expression_span(self.request, expression)
                .map_err(EvaluationFailure::Infrastructure)?;

            let anchored = diagnostics
                .iter()
                .map(|diagnostic| {
                    if diagnostic.primary_span().is_some() {
                        diagnostic.clone()
                    } else {
                        diagnostic.clone().with_primary_span(span).with_label(
                            bray_diagnostics::DiagnosticLabel::primary(
                                bray_diagnostics::DiagnosticLabelKind::InvalidConstantExpression,
                                span,
                            ),
                        )
                    }
                })
                .collect();

            self.diagnostics = self.diagnostics.merged(&anchored);
        } else {
            self.diagnostics = self.diagnostics.merged(diagnostics);
        }

        Ok(())
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
                diagnostic: ConstantDiagnostic::limit(ConstantLimitKind::EvaluationSteps, 1, 0),
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

                self.merge_call_diagnostics(expression, result.diagnostics())?;

                Ok(value)
            }
            Ok(ConstantCallResolution::Cycle) => Err(EvaluationFailure::Source {
                expression,
                diagnostic: ConstantDiagnostic::Cycle { definition: None },
            }),
            Ok(ConstantCallResolution::Ineligible(diagnostics)) => {
                if diagnostics.has_errors() {
                    self.merge_call_diagnostics(expression, &diagnostics)?;

                    return self
                        .recovery_value(result_type)
                        .map_err(EvaluationFailure::Infrastructure);
                }

                Err(EvaluationFailure::invalid_expression(expression))
            }
            Err(CheckerQueryError::Cancelled) => Err(EvaluationFailure::Cancelled),
            Err(CheckerQueryError::Infrastructure(error)) => {
                Err(EvaluationFailure::Infrastructure(error))
            }
            Err(CheckerQueryError::Upstream(error)) => {
                self.upstream_failure = Some(error);

                Err(EvaluationFailure::Upstream)
            }
        }
    }
}
