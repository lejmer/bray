use bray_bound_tree::{
    BoundCallResult, BoundCallableTarget, BoundExpressionId, ConversionTarget, SelectedArgument,
    SelectedConversion, SemanticSelection,
};
use bray_symbols::{
    CallableInstanceData, ConstantTermData, ConstantTermId, ImplementationInstanceId, TypeId,
};

use crate::constant::diagnostic::{ConstantDiagnostic, ConstantLimitKind};

use crate::{
    CheckerQueryError, CheckerRequestContext, ConstantCallRequest, ConstantCallResolution,
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

        let offset = usize::from(call.receiver().is_some());
        let mut supplied = vec![None; call.arguments().len() + offset];

        if let Some(receiver) = call.receiver() {
            let mut term = self.evaluate(receiver.expression())?;

            let ty = receiver
                .input_type(self.request.semantic_values())
                .expect("checked receiver input belongs to this store");

            if receiver.mode() == bray_symbols::ReceiverMode::Shared {
                let values = self.request.semantic_values();

                if let Some(mut value) = self.term_value(term) {
                    let bray_symbols::TypeData::Borrow { target, .. } = *values.type_data(ty)
                    else {
                        panic!("shared receiver input is a borrow");
                    };

                    loop {
                        let data = values.constant_value_data(value);

                        if data.ty() == ty {
                            break;
                        }

                        if data.ty() == target {
                            value = values
                                .intern_constant_value(bray_symbols::ConstantValueData::new(
                                    ty,
                                    bray_symbols::ConstantValueKind::Borrowed(value),
                                ))
                                .expect("checked shared receiver view retains its referent type");

                            break;
                        }

                        let bray_symbols::ConstantValueKind::Borrowed(referent) = data.kind()
                        else {
                            panic!("checked receiver adjustment retains its observed target");
                        };

                        value = *referent;
                    }

                    term = self.intern_term(ConstantTermData::Value(value));
                }
            }

            supplied[0] = Some((term, ty));
        }

        for argument in call.arguments() {
            if let SelectedArgument::Explicit {
                expression,
                ordinal,
                conversion,
                ..
            } = argument
            {
                let value = self.evaluate_selected_conversion(*expression, conversion)?;

                supplied[usize::try_from(*ordinal).expect("selected argument ordinal fits host")
                    + offset] = Some((value, conversion.target_type()));
            }
        }

        for argument in call.arguments() {
            let SelectedArgument::Default {
                provider,
                ordinal,
                ty,
                ..
            } = argument
            else {
                continue;
            };

            let Some(callable) = call.resolution().declaration_source() else {
                return Err(EvaluationFailure::invalid_expression(expression));
            };

            let earlier = supplied
                .iter()
                .take(usize::try_from(*ordinal).expect("default ordinal fits host") + offset)
                .map(|argument| {
                    let (term, _) = argument.expect("default observes supplied earlier arguments");

                    self.closed_value(term, expression)
                })
                .collect::<Result<Vec<_>, _>>()?;

            let value = self.evaluate_template_call_values(
                expression,
                crate::ConstantCallTarget::Default(
                    bray_symbols::DefaultValueProvider::CallableParameter(*provider),
                    callable.substitution(),
                ),
                earlier,
                *ty,
            )?;

            let term = self.intern_term(ConstantTermData::Value(value));

            supplied[usize::try_from(*ordinal).expect("default ordinal fits host") + offset] =
                Some((term, *ty));
        }

        let arguments = supplied
            .into_iter()
            .map(|argument| argument.expect("selected call supplies every parameter"))
            .collect::<Vec<_>>();

        if let BoundCallableTarget::Predicate(predicate) = call.target() {
            let arguments = arguments
                .into_iter()
                .map(|(argument, _)| argument)
                .collect::<Vec<_>>();

            return self
                .intern_typed_term(ty, ConstantTermData::predicate_call(predicate, arguments));
        }

        if let Some((body, _)) = call.resolution().anonymous_body() {
            let values = arguments
                .iter()
                .map(|(term, _)| self.closed_value(*term, expression))
                .collect::<Result<Vec<_>, _>>()?;

            let value = self.evaluate_template_call_values(
                expression,
                crate::ConstantCallTarget::Anonymous(body.clone(), None),
                values,
                ty,
            )?;

            return Ok(self.intern_term(ConstantTermData::Value(value)));
        }

        let Some(callable) = call.resolution().declaration_source() else {
            return Err(EvaluationFailure::invalid_expression(expression));
        };

        let witnesses = call.resolution().implementation_witnesses();

        let selected_implementation = match witnesses {
            [] => None,
            [witness] => Some(*witness),
            _ => {
                return Err(EvaluationFailure::invalid_expression(expression));
            }
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

        let Some(operand) = self.term_value(operand) else {
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
        let data = self.request.semantic_values().constant_value_data(value);

        if data.ty() != ty {
            panic!(
                "selected conversion at {expression:?} must produce type {ty:?}, actual: {data:?}"
            );
        }

        Ok(self.intern_term(ConstantTermData::Value(value)))
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
                Err(CheckerQueryError::Upstream(error)) => {
                    self.upstream_failure = Some(error);

                    return Err(EvaluationFailure::Upstream);
                }
            }

            let callable = self
                .request
                .semantic_values()
                .intern_callable_instance(callable).unwrap_or_else(|error| panic!("evaluate_call_terms must satisfy its checked construction contract: {error:?}"));

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

        Ok(self.intern_term(ConstantTermData::Value(value)))
    }

    fn merge_call_diagnostics(
        &mut self,
        expression: BoundExpressionId,
        diagnostics: &bray_diagnostics::DiagnosticBag,
    ) {
        // Preserve the published diagnostics while attaching this caller's source context.
        let mut anchored = diagnostics.clone();

        crate::diagnostic::anchor_diagnostics(
            &mut anchored,
            bray_diagnostics::DiagnosticLabelKind::InvalidConstantExpression,
            || Some(crate::diagnostic::expression_span(self.request, expression)),
        );

        self.diagnostics = self.diagnostics.merged(&anchored);
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
            crate::ConstantCallTarget::Declaration(callable),
            selected_implementation,
            arguments,
            result_type,
            limits,
        );

        self.resolve_constant_request(expression, &request)
    }

    fn evaluate_template_call_values(
        &mut self,
        expression: BoundExpressionId,
        target: crate::ConstantCallTarget,
        arguments: impl IntoIterator<Item = bray_symbols::ConstantValueId>,
        result_type: TypeId,
    ) -> Result<bray_symbols::ConstantValueId, EvaluationFailure> {
        let limits = self
            .budget
            .remaining_limits(self.input.limits())
            .nested_call()
            .ok_or_else(|| EvaluationFailure::Source {
                expression,
                diagnostic: ConstantDiagnostic::limit(ConstantLimitKind::EvaluationSteps, 1, 0),
            })?;

        let request = ConstantCallRequest::new(target, None, arguments, result_type, limits);

        self.resolve_constant_request(expression, &request)
    }

    fn resolve_constant_request(
        &mut self,
        expression: BoundExpressionId,
        request: &ConstantCallRequest,
    ) -> Result<bray_symbols::ConstantValueId, EvaluationFailure> {
        let result_type = request.result_type();

        let Some(resolver) = self.input.call_resolver() else {
            return Err(EvaluationFailure::invalid_expression(expression));
        };

        match resolver.resolve(request) {
            Ok(ConstantCallResolution::Evaluated(result)) => {
                self.budget
                    .charge_usage(expression, result.value().usage())?;

                let value = result.value().value();
                let value_data = self.request.semantic_values().constant_value_data(value);

                if value_data.ty() != result_type {
                    panic!(
                        "constant call at {expression:?} must produce type {result_type:?}, actual: {value_data:?}"
                    );
                }

                self.merge_call_diagnostics(expression, result.diagnostics());

                Ok(value)
            }
            Ok(ConstantCallResolution::Cycle) => Err(EvaluationFailure::Source {
                expression,
                diagnostic: ConstantDiagnostic::Cycle { definition: None },
            }),
            Ok(ConstantCallResolution::Ineligible(diagnostics)) => {
                if diagnostics.has_errors() {
                    self.merge_call_diagnostics(expression, &diagnostics);

                    return Ok(self.recovery_value(result_type));
                }

                Err(EvaluationFailure::invalid_expression(expression))
            }
            Err(CheckerQueryError::Cancelled) => Err(EvaluationFailure::Cancelled),
            Err(CheckerQueryError::Upstream(error)) => {
                self.upstream_failure = Some(error);

                Err(EvaluationFailure::Upstream)
            }
        }
    }
}
