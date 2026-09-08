use bray_bound_tree::{
    BoundCallResult, BoundCallableTarget, BoundExpressionId, ConversionTarget, SelectedConversion,
    SelectedOperation, SemanticSelection,
};
use bray_symbols::{
    CallableInstanceData, ConstantTermData, ConstantTermId, ImplementationInstanceId, TypeId,
};

use crate::constant::diagnostic::{ConstantDiagnostic, ConstantLimitKind};

use crate::{
    CheckerInfrastructureError, CheckerRequestContext, ConstantCallRequest, ConstantCallResolution,
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
        let call = match self.input.semantic_selections().expression(expression) {
            Some(SemanticSelection::Operation(SelectedOperation::Construction(_))) => {
                return self.evaluate_construction(expression, ty);
            }
            Some(SemanticSelection::Call(call)) => call,
            _ => return Err(EvaluationFailure::invalid_expression(expression)),
        };

        if !matches!(call.resolution().result(), BoundCallResult::Immediate(_)) {
            return Err(EvaluationFailure::invalid_expression(expression));
        }

        let arguments = call
            .explicit_inputs()
            .ok_or_else(|| EvaluationFailure::invalid_expression(expression))?;

        let arguments = arguments
            .into_iter()
            .map(|(argument, conversion)| {
                let (term, ty) = match conversion {
                    Some(conversion) => (
                        self.evaluate_selected_conversion(argument, conversion)?,
                        conversion.target_type(),
                    ),
                    None => (self.evaluate(argument)?, self.expression_type(argument)?),
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
                ConversionTarget::Composite(_) | ConversionTarget::CallableContract => self
                    .intern_typed_term(
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
        let mut receiver_type = None;

        let arguments = arguments
            .into_iter()
            .enumerate()
            .map(|(index, (argument, ty))| {
                if index == 0 {
                    receiver_type = Some(ty);
                }

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

            if !resolver
                .is_constant_callable(callable)
                .map_err(|error| self.query_failure(error))?
            {
                return Err(EvaluationFailure::invalid_expression(expression));
            }

            if let Some(term) =
                self.symbolic_presence_call(callable, &arguments, receiver_type, result_type)?
            {
                return Ok(term);
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

                self.diagnostics = self.diagnostics.merged(result.diagnostics());

                Ok(value)
            }
            Ok(ConstantCallResolution::Cycle) => Err(EvaluationFailure::Source {
                expression,
                diagnostic: ConstantDiagnostic::Cycle { definition: None },
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
            Err(error) => Err(self.query_failure(error)),
        }
    }

    fn symbolic_presence_call(
        &mut self,
        callable: CallableInstanceData,
        arguments: &[ConstantTermId],
        receiver_type: Option<TypeId>,
        result_type: TypeId,
    ) -> Result<Option<ConstantTermId>, EvaluationFailure> {
        use bray_compiler_known::{ImplementationHook, RepresentationRole};

        let hook = self
            .request
            .implementation_hook(callable.definition().callable_symbol().into_any())
            .map_err(|error| self.query_failure(error))?;

        let Some(hook) = hook.filter(|hook| hook.is_available()) else {
            return Ok(None);
        };

        let present = match hook.hook() {
            ImplementationHook::NullableIsPresent => true,
            ImplementationHook::NullableIsAbsent => false,
            _ => return Ok(None),
        };

        let ([receiver], Some(receiver_type)) = (arguments, receiver_type) else {
            return Err(EvaluationFailure::invalid_input());
        };

        let values = self.request.semantic_values();

        let receiver_type = values.unborrowed_type(receiver_type).map_err(|error| {
            EvaluationFailure::Infrastructure(CheckerInfrastructureError::SemanticValueStore(error))
        })?;

        let receiver_type = values.type_data(receiver_type).map_err(|error| {
            EvaluationFailure::Infrastructure(CheckerInfrastructureError::SemanticValueStore(error))
        })?;

        if !matches!(receiver_type.as_ref(), bray_symbols::TypeData::Nullable(_))
            || crate::representation::type_representation(self.request, result_type)
                .map_err(EvaluationFailure::Infrastructure)?
                != Some(RepresentationRole::ScalarBool)
        {
            return Err(EvaluationFailure::invalid_input());
        }

        let test = self.intern_typed_term(
            result_type,
            ConstantTermData::Test {
                subject: *receiver,
                kind: bray_symbols::ConstantTest::NullablePresent,
            },
        )?;

        if present {
            Ok(Some(test))
        } else {
            self.intern_typed_term(
                result_type,
                ConstantTermData::Unary {
                    operation: bray_symbols::ConstantUnaryOperation::LogicalNot,
                    operand: test,
                },
            )
            .map(Some)
        }
    }
}
