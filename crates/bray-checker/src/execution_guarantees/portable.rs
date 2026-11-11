use bray_bound_tree::BoundReferenceTarget;
use bray_symbols::{
    ConstantProjection, ConstantProjectionKind, ConstantTermData, ConstantTermId,
    ConstantValueData, ConstantValueKind, SemanticValueStore, SemanticValueStoreError,
    SymbolOrdinal, TypeId,
};

use super::ExecutionCondition;
use crate::constant::operator::{
    binary_operator, binary_term_operation, unary_operator, unary_term_operation,
};

/// Encodes bounded execution conditions using the existing portable constant-term representation.
/// Inputs use receiver-first ordinals, with the result following the last input.
/// Unsupported or runtime-dependent meaning cannot become portable evidence.
pub fn execution_condition_term(
    values: &SemanticValueStore,
    condition: &ExecutionCondition,
    inputs: &[BoundReferenceTarget],
    boolean_type: TypeId,
) -> Result<Option<ConstantTermId>, SemanticValueStoreError> {
    encode(values, condition, inputs, boolean_type, &mut {
        ExecutionCondition::WORK_LIMIT
    })
}

fn encode(
    values: &SemanticValueStore,
    condition: &ExecutionCondition,
    inputs: &[BoundReferenceTarget],
    boolean_type: TypeId,
    budget: &mut usize,
) -> Result<Option<ConstantTermId>, SemanticValueStoreError> {
    let Some(remaining) = budget.checked_sub(1) else {
        return Ok(None);
    };

    *budget = remaining;

    let data = match condition {
        ExecutionCondition::Boolean(value) => {
            ConstantTermData::Value(values.intern_constant_value(ConstantValueData::new(
                boolean_type,
                ConstantValueKind::Boolean(*value),
            ))?)
        }
        ExecutionCondition::Literal(value) => {
            // The semantic store owns the immutable literal independently of this condition.
            ConstantTermData::Value(values.intern_constant_value(value.as_ref().clone())?)
        }
        ExecutionCondition::Input(place) => {
            let mut term = if let Some(term) = place.constant_term() {
                term
            } else if let Some(BoundReferenceTarget::Surface(
                bray_symbols::AnySymbolId::GenericConstParameter(parameter),
            )) = place.reference()
            {
                values.intern_constant_term(ConstantTermData::Parameter(parameter))?
            } else {
                let Some(ordinal) = inputs
                    .iter()
                    .position(|input| Some(*input) == place.reference())
                    .and_then(|index| u32::try_from(index).ok())
                else {
                    return Ok(None);
                };

                values.intern_constant_term(ConstantTermData::CallableArgument(
                    SymbolOrdinal::new(ordinal),
                ))?
            };

            for field in &*place.projections {
                let kind = match *field {
                    ConstantProjectionKind::ProductField(field) => {
                        ConstantProjectionKind::ProductField(field)
                    }
                    ConstantProjectionKind::UnionPayloadField(field) => {
                        ConstantProjectionKind::UnionPayloadField(field)
                    }
                    ConstantProjectionKind::NullableValue => ConstantProjectionKind::NullableValue,
                    ConstantProjectionKind::TupleElement(index) => {
                        ConstantProjectionKind::TupleElement(index)
                    }
                    ConstantProjectionKind::ArrayElementOrdinal(index) => {
                        ConstantProjectionKind::ArrayElementOrdinal(index)
                    }
                    _ => return Ok(None),
                };

                let projected = values.intern_constant_term(ConstantTermData::Projection(
                    ConstantProjection::new(term, kind),
                ))?;

                term = projected;
            }

            return Ok(Some(term));
        }
        ExecutionCondition::Result => {
            let Ok(ordinal) = u32::try_from(inputs.len()) else {
                return Ok(None);
            };

            ConstantTermData::CallableArgument(SymbolOrdinal::new(ordinal))
        }
        ExecutionCondition::VariantTest(variant, subject) => {
            let Some(subject) = encode(values, subject, inputs, boolean_type, budget)? else {
                return Ok(None);
            };

            ConstantTermData::VariantTest {
                subject,
                variant: *variant,
            }
        }
        ExecutionCondition::Present(value) => {
            let Some(value) = encode(values, value, inputs, boolean_type, budget)? else {
                return Ok(None);
            };

            ConstantTermData::NullablePresent(value)
        }
        ExecutionCondition::Borrowed(value) => {
            let Some(operand) = encode(values, value, inputs, boolean_type, budget)? else {
                return Ok(None);
            };

            ConstantTermData::Unary {
                operation: bray_symbols::ConstantUnaryOperation::BorrowObservation,
                operand,
            }
        }
        ExecutionCondition::Entry {
            condition,
            captured: false,
        } => {
            let Some(operand) = encode(values, condition, inputs, boolean_type, budget)? else {
                return Ok(None);
            };

            ConstantTermData::Unary {
                operation: bray_symbols::ConstantUnaryOperation::EntryCondition,
                operand,
            }
        }
        ExecutionCondition::Entry { captured: true, .. } => return Ok(None),
        ExecutionCondition::Trusted(value) => {
            let Some(operand) = encode(values, value, inputs, boolean_type, budget)? else {
                return Ok(None);
            };

            ConstantTermData::Unary {
                operation: bray_symbols::ConstantUnaryOperation::PredicateTrust,
                operand,
            }
        }
        ExecutionCondition::Operation(operator, operands) => {
            let terms = operands
                .iter()
                .map(|operand| encode(values, operand, inputs, boolean_type, budget))
                .collect::<Result<Option<Vec<_>>, _>>()?;

            let Some(terms) = terms else {
                return Ok(None);
            };

            match terms.as_slice() {
                [operand] => {
                    let Some(operation) = unary_term_operation(*operator) else {
                        return Ok(None);
                    };

                    ConstantTermData::Unary {
                        operation,
                        operand: *operand,
                    }
                }
                [left, right] => {
                    let Some(operation) = binary_term_operation(*operator) else {
                        return Ok(None);
                    };

                    ConstantTermData::Binary {
                        operation,
                        left: *left,
                        right: *right,
                    }
                }
                _ => return Ok(None),
            }
        }
        ExecutionCondition::Call(callable, implementation, witnesses, arguments, reference_type) => {
            let Some(arguments) = arguments
                .iter()
                .map(|argument| encode(values, argument, inputs, boolean_type, budget))
                .collect::<Result<Option<Vec<_>>, _>>()?
            else {
                return Ok(None);
            };

            let call = ConstantTermData::call(
                values.intern_callable_instance(*callable)?,
                implementation
                    .map(|witness| values.intern_implementation_instance(witness))
                    .transpose()?,
                witnesses
                    .iter()
                    .map(|witness| values.intern_implementation_instance(*witness))
                    .collect::<Result<Vec<_>, _>>()?,
                arguments,
            );

            if let Some(ty) = reference_type {
                let term = values.intern_constant_term(call)?;
                return values.intern_constant_term(ConstantTermData::Typed { term, ty: *ty }).map(Some);
            }

            call
        }
        ExecutionCondition::Predicate(predicate, substitution, arguments) => {
            let terms = arguments
                .iter()
                .map(|argument| encode(values, argument, inputs, boolean_type, budget))
                .collect::<Result<Option<Vec<_>>, _>>()?;

            let Some(arguments) = terms else {
                return Ok(None);
            };

            ConstantTermData::PredicateCall {
                predicate: bray_symbols::PredicateInstanceData::new(*predicate, *substitution),
                arguments: arguments.into(),
            }
        }
        ExecutionCondition::Projection(field, value) => {
            let Some(value) = encode(values, value, inputs, boolean_type, budget)? else {
                return Ok(None);
            };

            return values
                .intern_constant_term(ConstantTermData::Projection(ConstantProjection::new(
                    value, *field,
                )))
                .map(Some);
        }
        ExecutionCondition::Unknown
        | ExecutionCondition::ValueAlias(_, _)
        | ExecutionCondition::Tagged(_, _)
        | ExecutionCondition::Variant(_)
        | ExecutionCondition::Constructed(_, _)
        | ExecutionCondition::Observation { .. }
        | ExecutionCondition::Expression(_)
        | ExecutionCondition::CapturedInput { .. }
        | ExecutionCondition::ScopedCapability(_)
        | ExecutionCondition::PostState(_, _)
        | ExecutionCondition::InitializedSnapshot(_, _, _)
        | ExecutionCondition::Joined(_, _, _)
        | ExecutionCondition::Discriminant(_)
        | ExecutionCondition::ScalarValue(_) => return Ok(None),
    };

    values.intern_constant_term(data).map(Some)
}

/// Restores a portable condition using the supplied callable's semantic input identities.
/// Unknown forms and exhausted traversal retain no proof meaning.
pub fn execution_condition_from_term(
    values: &SemanticValueStore,
    term: ConstantTermId,
    inputs: &[BoundReferenceTarget],
) -> Result<ExecutionCondition, SemanticValueStoreError> {
    let inputs = inputs
        .iter()
        .copied()
        .map(|input| ExecutionCondition::Input(super::ExecutionPlace::from(input)))
        .collect::<Vec<_>>();

    decode(values, term, &inputs, &mut {
        ExecutionCondition::WORK_LIMIT
    })
}

/// Restores predicates carried by an opaque callable type using positional input identities.
pub fn execution_condition_from_type_term(
    values: &SemanticValueStore,
    term: ConstantTermId,
    parameter_count: usize,
) -> Result<ExecutionCondition, SemanticValueStoreError> {
    let inputs = (0..parameter_count)
        .map(|index| {
            ExecutionCondition::Input(super::ExecutionPlace::argument(SymbolOrdinal::new(
                u32::try_from(index).expect("checked callable parameter count must fit ordinals"),
            )))
        })
        .collect::<Vec<_>>();

    decode(values, term, &inputs, &mut {
        ExecutionCondition::WORK_LIMIT
    })
}

pub(crate) fn decode(
    values: &SemanticValueStore,
    term: ConstantTermId,
    inputs: &[ExecutionCondition],
    budget: &mut usize,
) -> Result<ExecutionCondition, SemanticValueStoreError> {
    let Some(remaining) = budget.checked_sub(1) else {
        return Ok(ExecutionCondition::Unknown);
    };

    *budget = remaining;

    Ok(match values.constant_term_data(term).as_ref() {
        ConstantTermData::Typed { term, ty } => {
            let mut condition = decode(values, *term, inputs, budget)?;

            if matches!(values.type_data(*ty).as_ref(), bray_symbols::TypeData::Borrow { .. })
                && let ExecutionCondition::Call(_, _, _, _, reference_type) = &mut condition {
                *reference_type = Some(*ty);
            }

            condition
        }
        ConstantTermData::Parameter(parameter) => {
            ExecutionCondition::Input(BoundReferenceTarget::Surface((*parameter).into()).into())
        }
        ConstantTermData::DefinitionApplication { .. } | ConstantTermData::TraitConstant { .. } => {
            ExecutionCondition::Input(super::ExecutionPlace::constant_reference(term))
        }
        ConstantTermData::Value(value) => {
            let value = values.constant_value_data(*value);

            match value.kind() {
                ConstantValueKind::Boolean(value) => ExecutionCondition::Boolean(*value),
                ConstantValueKind::Error => ExecutionCondition::Unknown,
                _ => ExecutionCondition::Literal(value),
            }
        }
        ConstantTermData::VariantTest { subject, variant } => {
            decode(values, *subject, inputs, budget)?.matches_variant(*variant)
        }
        ConstantTermData::NullablePresent(value) => {
            ExecutionCondition::Present(std::sync::Arc::new(decode(values, *value, inputs, budget)?))
        }
        ConstantTermData::CallableArgument(ordinal) => {
            let index = usize::try_from(ordinal.raw()).ok();

            match index {
                Some(index) if index == inputs.len() => ExecutionCondition::Result,
                Some(index) => inputs
                    .get(index)
                    .cloned()
                    .unwrap_or(ExecutionCondition::Unknown),
                None => ExecutionCondition::Unknown,
            }
        }
        ConstantTermData::Unary {
            operation: bray_symbols::ConstantUnaryOperation::PredicateTrust,
            operand,
        } => ExecutionCondition::Trusted(std::sync::Arc::new(decode(
            values, *operand, inputs, budget,
        )?)),
        ConstantTermData::Unary {
            operation: bray_symbols::ConstantUnaryOperation::BorrowObservation,
            operand,
        } => decode(values, *operand, inputs, budget)?.borrowed(),
        ConstantTermData::Unary {
            operation: bray_symbols::ConstantUnaryOperation::EntryCondition,
            operand,
        } => ExecutionCondition::Entry {
            condition: std::sync::Arc::new(decode(values, *operand, inputs, budget)?),
            captured: false,
        },
        ConstantTermData::Unary { operation, operand } => ExecutionCondition::operation(
            unary_operator(*operation),
            vec![decode(values, *operand, inputs, budget)?],
        ),
        ConstantTermData::Binary {
            operation,
            left,
            right,
        } => ExecutionCondition::operation(
            binary_operator(*operation),
            vec![
                decode(values, *left, inputs, budget)?,
                decode(values, *right, inputs, budget)?,
            ],
        ),
        ConstantTermData::Call {
            callable,
            arguments,
            selected_implementation,
            implementation_witnesses,
        } => ExecutionCondition::call(
            *values.callable_instance_data(*callable),
            selected_implementation.map(|witness| *values.implementation_instance_data(witness)),
            implementation_witnesses
                .iter()
                .map(|witness| *values.implementation_instance_data(*witness)),
            arguments
                .iter()
                .map(|argument| decode(values, *argument, inputs, budget))
                .collect::<Result<_, _>>()?,
            None,
        ),
        ConstantTermData::PredicateCall {
            predicate,
            arguments,
        } => ExecutionCondition::predicate(
            predicate.definition(),
            predicate.substitution(),
            arguments
                .iter()
                .map(|argument| decode(values, *argument, inputs, budget))
                .collect::<Result<Vec<_>, _>>()?,
        ),
        ConstantTermData::Projection(projection) => {
            decode(values, projection.subject(), inputs, budget)?.component(projection.kind())
        }
        _ => ExecutionCondition::Unknown,
    })
}

#[cfg(test)]
mod tests {
    use super::{execution_condition_from_term, execution_condition_term};
    use crate::execution_guarantees::ExecutionCondition;
    use crate::test_support::{
        TestCheckerContext, callable_instance, empty_substitution, semantic_values,
    };
    use bray_bound_tree::BoundReferenceTarget;
    use bray_symbols::{
        CallableParameterSymbolId, FunctionSymbolId, ImplementationInstanceData,
        ImplementationSymbolId, SymbolId, UnnamedTraitImplementationSymbolId,
    };

    #[test]
    fn portable_calls_preserve_witness_choices_and_argument_substitution() {
        let values = semantic_values();
        let context = TestCheckerContext::new(false);

        let boolean_type = crate::representation::representation_type_for_context(
            &context,
            bray_compiler_known::RepresentationRole::ScalarBool,
        );

        let callable =
            callable_instance(FunctionSymbolId::from_symbol_id(SymbolId::new(900)).into());

        let witnesses = [901, 902].map(|ordinal| {
            let definition = ImplementationSymbolId::UnnamedTrait(
                UnnamedTraitImplementationSymbolId::from_symbol_id(SymbolId::new(ordinal)),
            );

            ImplementationInstanceData::new(definition, empty_substitution(definition.into_any()))
        });

        let parameter = BoundReferenceTarget::Surface(
            CallableParameterSymbolId::from_symbol_id(SymbolId::new(903)).into(),
        );

        let argument = ExecutionCondition::Input(parameter.into());

        let condition = ExecutionCondition::call(
            callable,
            Some(witnesses[0]),
            [witnesses[1]],
            vec![argument.clone()],
            None,
        );

        let other =
            ExecutionCondition::call(callable, Some(witnesses[0]), [witnesses[0]], vec![argument], None);

        assert_ne!(condition, other);

        let term = execution_condition_term(values, &condition, &[parameter], boolean_type)
            .expect("portable call encodes")
            .expect("checked call is portable");

        let decoded = execution_condition_from_term(values, term, &[parameter])
            .expect("portable call decodes");

        assert_eq!(decoded, condition);

        let mut budget = ExecutionCondition::WORK_LIMIT;

        let substituted = decoded.substitute(
            &|_| ExecutionCondition::Boolean(true),
            &ExecutionCondition::Unknown,
            &mut budget,
        );

        let expected = ExecutionCondition::call(
            callable,
            Some(witnesses[0]),
            [witnesses[1]],
            vec![ExecutionCondition::Boolean(true)],
            None,
        );

        assert_eq!(substituted, expected);

        let reference_type = values.intern_type(bray_symbols::TypeData::Borrow {
            kind: bray_symbols::BorrowKind::Shared, target: boolean_type,
        }).expect("checked borrowed result type");
        let borrowed = ExecutionCondition::call(callable, Some(witnesses[0]), [witnesses[1]],
            vec![ExecutionCondition::Input(parameter.into())], Some(reference_type)).borrowed();
        let term = execution_condition_term(values, &borrowed, &[parameter], boolean_type)
            .expect("typed reference computation encodes").expect("typed reference computation is portable");

        assert_eq!(execution_condition_from_term(values, term, &[parameter])
            .expect("typed reference computation decodes"), borrowed);
        assert_eq!(borrowed.ownership_identity(), None);
    }
}
