use crate::constant::operator::binary_operator;
use bray_compiler_known::IntegerRepresentation;
use bray_symbols::{
    AnyConstantDefinitionId, ConstantField, ConstantInstanceKey, ConstantProjection,
    ConstantProjectionKind, ConstantTermData, ConstantTermId, ConstantValueId, ConstantValueKind,
    GenericSubstitutionId, ImplementationInstanceId, TargetSizedIntegerType, TypeId,
};

use super::super::ConstantReferenceResolution;
use super::super::call::ConstantCallRequest;
use super::super::diagnostic::{ConstantDiagnostic, ConstantLimitKind, diagnostic_operation};
use super::super::integer::fits_integer_representation;
use super::super::operation::fold_binary;
use super::evaluator::TemplateEvaluator;
use super::support::{
    TemplateEvaluationFailure, check_definition_materialization, integer_index, operation_failure,
};
use crate::CheckerRequestContext;

impl<C> TemplateEvaluator<'_, C>
where
    C: CheckerRequestContext + ?Sized,
{
    fn evaluate_invocation(
        &mut self,
        target: crate::ConstantCallTarget,
        implementation: Option<ImplementationInstanceId>,
        arguments: &[ConstantTermId],
        recovery_type: TypeId,
    ) -> Result<ConstantValueId, TemplateEvaluationFailure> {
        let contract = self
            .resolver
            .contract(&target)
            .map_err(|error| self.record_query_failure(error))?;

        self.diagnostics
            .add_range(contract.diagnostics().iter().cloned());

        let Some(contract) = contract.value() else {
            return Ok(super::support::recovery_value(
                self.context.semantic_values(),
                recovery_type,
            ));
        };

        assert_eq!(
            arguments.len(),
            contract.inputs().len(),
            "checked constant term retains its selected argument mapping"
        );

        let arguments = arguments
            .iter()
            .zip(contract.inputs())
            .map(|(argument, ty)| self.evaluate_term(*argument, *ty))
            .collect::<Result<Vec<_>, _>>()?;

        let limits = self
            .budget
            .remaining_limits(self.limits)
            .nested_call()
            .ok_or_else(|| {
                TemplateEvaluationFailure::Diagnostic(ConstantDiagnostic::limit(
                    ConstantLimitKind::EvaluationSteps,
                    1,
                    0,
                ))
            })?;

        let request = match target {
            crate::ConstantCallTarget::Declaration(callable) => ConstantCallRequest::new(
                crate::ConstantCallTarget::Declaration(callable),
                implementation,
                arguments,
                contract.result(),
                limits,
            ),
            target => {
                assert!(
                    implementation.is_none(),
                    "predicate invocation has no selected callable implementation"
                );

                ConstantCallRequest::new(target, None, arguments, contract.result(), limits)
            }
        };

        self.resolve_call(&request, contract.result())
    }

    pub(super) fn evaluate_requirements(
        &mut self,
        target: &crate::ConstantCallTarget,
        requirements: &[bray_symbols::PredicateSemanticSummary],
    ) -> Result<bool, TemplateEvaluationFailure> {
        if requirements.is_empty() {
            return Ok(true);
        }

        let values = self.context.semantic_values();

        let boolean = crate::representation::representation_type_for_context(
            self.context,
            bray_compiler_known::RepresentationRole::ScalarBool,
        );

        let evidence = crate::ExecutionCallEvidence::from_constants(values, self.arguments);

        for (index, requirement) in requirements.iter().enumerate() {
            let established = if requirement.domain() == bray_symbols::PredicateDomain::Value {
                let template = self
                    .resolver
                    .requirement_template(target, index)
                    .map_err(|error| self.record_query_failure(error))?;

                self.diagnostics
                    .add_range(template.diagnostics().iter().cloned());

                if self.diagnostics.has_errors() {
                    return Ok(false);
                }

                let template = template
                    .value()
                    .as_ref()
                    .expect("admitted ordinary requirement retains its complete checked graph");

                let mut evaluator = TemplateEvaluator {
                    context: self.context,
                    template,
                    substitution: self.substitution,
                    selected_implementation: self.selected_implementation,
                    arguments: self.arguments,
                    resolver: self.resolver,
                    limits: self.limits,
                    budget: self.budget,
                    values: vec![None; template.nodes().len()],
                    checked_materialization: Default::default(),
                    diagnostics: Default::default(),
                    upstream_failure: None,
                    static_initializer: false,
                };

                let result = evaluator.evaluate_result(
                    bray_bound_tree::CheckedTemplateKind::CallableContract,
                    boolean,
                );

                self.diagnostics.add_range(evaluator.diagnostics);
                self.upstream_failure = evaluator.upstream_failure;

                let value = result?;

                if self.diagnostics.has_errors() {
                    return Ok(false);
                }

                match values.constant_value_data(value).kind() {
                    ConstantValueKind::Boolean(value) => *value,
                    value => {
                        panic!("checked constant requirement produces a Boolean, got {value:?}")
                    }
                }
            } else if let Some(term) = requirement.condition() {
                self.budget
                    .try_charge_step()
                    .map_err(TemplateEvaluationFailure::Diagnostic)?;

                let term = values
                    .substitute_constant_term(term, self.substitution)
                    .expect("checked constant requirement belongs to its selected generic context");

                let condition = crate::execution_condition_from_type_term(values, term, 0)
                    .expect("checked compiler-only requirement has portable predicate meaning");

                if requirement.is_trusted() {
                    evidence.proves_trusted(&[condition])
                } else {
                    evidence.proves(&[condition])
                }
            } else {
                false
            };

            if !established {
                let origin = self
                    .resolver
                    .requirement_origin(target, index)
                    .map_err(|error| self.record_query_failure(error))?;

                return Err(TemplateEvaluationFailure::Diagnostic(
                    ConstantDiagnostic::Precondition { origin },
                ));
            }
        }

        Ok(true)
    }

    pub(super) fn evaluate_term(
        &mut self,
        term: ConstantTermId,
        ty: TypeId,
    ) -> Result<ConstantValueId, TemplateEvaluationFailure> {
        let value = evaluate_term(self, term, ty)?;

        check_definition_materialization(self, value)?;

        Ok(value)
    }
}

pub(super) fn evaluate_term<C>(
    evaluator: &mut TemplateEvaluator<'_, C>,
    term: ConstantTermId,
    ty: TypeId,
) -> Result<ConstantValueId, TemplateEvaluationFailure>
where
    C: CheckerRequestContext + ?Sized,
{
    evaluator.observe_cancellation()?;

    evaluator
        .budget
        .try_charge_step()
        .map_err(TemplateEvaluationFailure::Diagnostic)?;

    let data = evaluator.context.semantic_values().constant_term_data(term);

    match data.as_ref() {
        ConstantTermData::Typed { term, ty } => {
            let ty = evaluator
                .context
                .semantic_values()
                .substitute_type(*ty, evaluator.substitution)
                .unwrap_or_else(|error| panic!("semantic_value in evaluate_term: {error:?}"));

            evaluator.evaluate_term(*term, ty)
        }
        ConstantTermData::Value(value) => Ok(*value),
        ConstantTermData::Unary { operation, operand } => {
            let operand = evaluator.evaluate_term(*operand, ty)?;

            evaluator.evaluate_unary(*operation, operand, ty)
        }
        ConstantTermData::Binary {
            operation,
            left,
            right,
        } => {
            let left = evaluator.evaluate_term(*left, ty)?;
            let right = evaluator.evaluate_term(*right, ty)?;

            let left = evaluator
                .context
                .semantic_values()
                .constant_value_data(left);

            let right = evaluator
                .context
                .semantic_values()
                .constant_value_data(right);

            let kind = fold_binary(
                binary_operator(*operation),
                left.kind(),
                right.kind(),
                evaluator.limits.integer_bits(),
            )
            .map_err(|error| {
                operation_failure(diagnostic_operation(binary_operator(*operation)), error)
            })?;

            Ok(evaluator.intern_value(ty, kind))
        }
        ConstantTermData::Conversion { operand, target } => {
            let operand = evaluator.evaluate_term(*operand, ty)?;

            let target = evaluator
                .context
                .semantic_values()
                .substitute_type(*target, evaluator.substitution)
                .unwrap_or_else(|error| panic!("semantic_value in evaluate_term: {error:?}"));

            evaluator.evaluate_conversion(operand, target)
        }
        ConstantTermData::VariantTest { subject, variant } => {
            let subject = evaluator.evaluate_term(*subject, ty)?;

            let value = evaluator
                .context
                .semantic_values()
                .constant_value_data(subject);

            let ConstantValueKind::Union {
                variant: active, ..
            } = value.kind()
            else {
                panic!("checked variant test must observe a union value");
            };

            Ok(evaluator.intern_value(ty, ConstantValueKind::Boolean(active == variant)))
        }
        ConstantTermData::NullablePresent(value) => {
            let value = evaluator.evaluate_term(*value, ty)?;

            Ok(evaluator.intern_value(ty, ConstantValueKind::NullablePresent(value)))
        }
        ConstantTermData::Tuple(values) => {
            let values = values
                .iter()
                .map(|value| evaluator.evaluate_term(*value, ty))
                .collect::<Result<Vec<_>, _>>()?;

            evaluator
                .budget
                .try_charge_elements(values.len())
                .map_err(TemplateEvaluationFailure::Diagnostic)?;

            Ok(evaluator.intern_value(ty, ConstantValueKind::Tuple(values.into())))
        }
        ConstantTermData::Array(values) => {
            let values = values
                .iter()
                .map(|value| evaluator.evaluate_term(*value, ty))
                .collect::<Result<Vec<_>, _>>()?;

            evaluator
                .budget
                .try_charge_elements(values.len())
                .map_err(TemplateEvaluationFailure::Diagnostic)?;

            Ok(evaluator.intern_value(ty, ConstantValueKind::Array(values.into())))
        }
        ConstantTermData::Product(fields) => {
            let fields = fields
                .iter()
                .map(|field| {
                    evaluator
                        .evaluate_term(*field.value(), ty)
                        .map(|value| ConstantField::new(*field.field(), value))
                })
                .collect::<Result<Vec<_>, _>>()?;

            evaluator
                .budget
                .try_charge_elements(fields.len())
                .map_err(TemplateEvaluationFailure::Diagnostic)?;

            Ok(evaluator.intern_value(ty, ConstantValueKind::Product(fields.into())))
        }
        ConstantTermData::Union { variant, fields } => {
            let fields = fields
                .iter()
                .map(|field| {
                    evaluator
                        .evaluate_term(*field.value(), ty)
                        .map(|value| ConstantField::new(*field.field(), value))
                })
                .collect::<Result<Vec<_>, _>>()?;

            evaluator
                .budget
                .try_charge_elements(fields.len())
                .map_err(TemplateEvaluationFailure::Diagnostic)?;

            Ok(evaluator.intern_value(
                ty,
                ConstantValueKind::Union {
                    variant: *variant,
                    fields: fields.into(),
                },
            ))
        }
        ConstantTermData::DefinitionApplication {
            definition,
            substitution,
            selected_implementation,
        } => evaluate_definition_application(
            evaluator,
            *definition,
            *substitution,
            *selected_implementation,
            ty,
        ),
        ConstantTermData::Call {
            callable,
            selected_implementation,
            arguments,
        } => {
            let callable = *evaluator
                .context
                .semantic_values()
                .callable_instance_data(*callable);

            evaluator.evaluate_invocation(
                crate::ConstantCallTarget::Declaration(callable),
                *selected_implementation,
                arguments,
                ty,
            )
        }
        ConstantTermData::PredicateCall {
            predicate,
            arguments,
        } => evaluator.evaluate_invocation(
            crate::ConstantCallTarget::Predicate(*predicate),
            None,
            arguments,
            ty,
        ),
        ConstantTermData::Projection(projection) => evaluate_projection(evaluator, projection, ty),
        ConstantTermData::IntegerLiteral {
            ty: integer_type,
            value,
        } => {
            let representation = match integer_type {
                TargetSizedIntegerType::Isize => IntegerRepresentation::TargetSigned,
                TargetSizedIntegerType::Usize => IntegerRepresentation::TargetUnsigned,
            };

            if !fits_integer_representation(value, representation, || {
                evaluator
                    .context
                    .selected_target()
                    .machine()
                    .pointer_width_bits()
            }) {
                return Err(TemplateEvaluationFailure::Diagnostic(
                    ConstantDiagnostic::Literal(crate::ConstantLiteralError::NotRepresentable),
                ));
            }

            Ok(evaluator.intern_value(ty, ConstantValueKind::Integer(value.clone())))
        }
        ConstantTermData::CallableArgument(ordinal) => Ok(ordinal
            .to_index()
            .and_then(|index| evaluator.arguments.get(index))
            .copied()
            .unwrap_or_else(|| {
                panic!("constant callable argument {ordinal:?} must exist in the application")
            })),
        ConstantTermData::Parameter(_) | ConstantTermData::TargetProperty(_) => {
            panic!("constant term must be closed before evaluation, term: {term:?}, type: {ty:?}")
        }
    }
}

fn evaluate_projection<C>(
    evaluator: &mut TemplateEvaluator<'_, C>,
    projection: &ConstantProjection,
    ty: TypeId,
) -> Result<ConstantValueId, TemplateEvaluationFailure>
where
    C: CheckerRequestContext + ?Sized,
{
    let subject = evaluator.evaluate_term(projection.subject(), ty)?;

    let subject = evaluator
        .context
        .semantic_values()
        .constant_value_data(subject);

    let value = match (subject.kind(), projection.kind()) {
        (ConstantValueKind::Tuple(elements), ConstantProjectionKind::TupleElement(ordinal)) => {
            ordinal
                .to_index()
                .and_then(|index| elements.get(index))
                .copied()
        }
        (
            ConstantValueKind::Array(elements),
            ConstantProjectionKind::ArrayElementOrdinal(index),
        ) => index
            .to_index()
            .and_then(|index| elements.get(index))
            .copied(),
        (ConstantValueKind::Array(elements), ConstantProjectionKind::ArrayElement(index)) => {
            let index = evaluator.evaluate_term(index, ty)?;

            let index = evaluator
                .context
                .semantic_values()
                .constant_value_data(index);

            integer_index(index.kind())
                .and_then(|index| elements.get(index))
                .copied()
        }
        (
            ConstantValueKind::Array(elements),
            ConstantProjectionKind::ArraySlice { lower, upper },
        ) => {
            let mut bounds = [None, None];

            for (destination, bound) in bounds.iter_mut().zip([lower, upper]) {
                if let Some(bound) = bound {
                    let bound = evaluator.evaluate_term(bound, ty)?;

                    let bound = evaluator
                        .context
                        .semantic_values()
                        .constant_value_data(bound);

                    *destination = Some(integer_index(bound.kind()).ok_or_else(|| {
                        TemplateEvaluationFailure::invalid_expression(
                            bray_diagnostics::DiagnosticExpressionCategory::Indexing,
                        )
                    })?);
                }
            }

            let elements = crate::constant::array::slice_elements(elements, bounds[0], bounds[1])
                .ok_or_else(|| {
                TemplateEvaluationFailure::invalid_expression(
                    bray_diagnostics::DiagnosticExpressionCategory::Indexing,
                )
            })?;

            evaluator
                .budget
                .try_charge_elements(elements.len())
                .map_err(TemplateEvaluationFailure::Diagnostic)?;

            return Ok(
                evaluator.intern_value(ty, ConstantValueKind::array(elements.iter().copied()))
            );
        }
        (ConstantValueKind::Product(fields), ConstantProjectionKind::ProductField(field)) => fields
            .iter()
            .find(|entry| *entry.field() == field)
            .map(|entry| *entry.value()),
        (
            ConstantValueKind::Union { fields, .. },
            ConstantProjectionKind::UnionPayloadField(field),
        ) => fields
            .iter()
            .find(|entry| *entry.field() == field)
            .map(|entry| *entry.value()),
        (ConstantValueKind::NullablePresent(value), ConstantProjectionKind::NullableValue) => {
            Some(*value)
        }
        _ => None,
    };

    value.ok_or_else(|| {
        TemplateEvaluationFailure::invalid_expression(match projection.kind() {
            ConstantProjectionKind::ArrayElement(_) | ConstantProjectionKind::ArraySlice { .. } => {
                bray_diagnostics::DiagnosticExpressionCategory::Indexing
            }
            _ => bray_diagnostics::DiagnosticExpressionCategory::MemberAccess,
        })
    })
}

fn evaluate_definition_application<C>(
    evaluator: &mut TemplateEvaluator<'_, C>,
    definition: AnyConstantDefinitionId,
    substitution: GenericSubstitutionId,
    selected_implementation: Option<ImplementationInstanceId>,
    ty: TypeId,
) -> Result<ConstantValueId, TemplateEvaluationFailure>
where
    C: CheckerRequestContext + ?Sized,
{
    let result = evaluator
        .resolver
        .resolve_constant(
            ConstantInstanceKey::new(definition, substitution, selected_implementation),
            evaluator.budget.remaining_limits(evaluator.limits),
        )
        .map_err(|error| evaluator.record_query_failure(error))?;

    evaluator.diagnostics = evaluator.diagnostics.merged(result.diagnostics());

    match result.value() {
        ConstantReferenceResolution::Value(value) => Ok(*value),
        ConstantReferenceResolution::Evaluated(result) => {
            evaluator
                .budget
                .try_charge_usage(result.usage())
                .map_err(TemplateEvaluationFailure::Diagnostic)?;

            Ok(result.value())
        }
        ConstantReferenceResolution::Term(term) => evaluator.evaluate_term(*term, ty),
        ConstantReferenceResolution::Cycle { definition } => Err(
            TemplateEvaluationFailure::Diagnostic(ConstantDiagnostic::Cycle {
                definition: *definition,
            }),
        ),
        ConstantReferenceResolution::Invalid => panic!(
            "constant definition application must have a valid reference resolution, definition: {definition:?}"
        ),
    }
}
