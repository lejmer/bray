use bray_bound_tree::BoundReferenceTarget;
use bray_symbols::{
    AnySymbolId, ConstantProjection, ConstantProjectionKind, ConstantTermData, ConstantTermId,
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
            let Some(ordinal) = inputs
                .iter()
                .position(|input| *input == place.root)
                .and_then(|index| u32::try_from(index).ok())
            else {
                return Ok(None);
            };

            let mut term = values.intern_constant_term(ConstantTermData::CallableArgument(
                SymbolOrdinal::new(ordinal),
            ))?;

            for field in &*place.fields {
                let Some(projected) = field_term(values, term, *field)? else {
                    return Ok(None);
                };

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
        ExecutionCondition::Field(field, value) => {
            let Some(value) = encode(values, value, inputs, boolean_type, budget)? else {
                return Ok(None);
            };

            return field_term(values, value, *field);
        }
        ExecutionCondition::Unknown
        | ExecutionCondition::Expression(_)
        | ExecutionCondition::PostState(_, _) => return Ok(None),
    };

    values.intern_constant_term(data).map(Some)
}

fn field_term(
    values: &SemanticValueStore,
    subject: ConstantTermId,
    field: AnySymbolId,
) -> Result<Option<ConstantTermId>, SemanticValueStoreError> {
    let kind = match field {
        AnySymbolId::StructField(field) => ConstantProjectionKind::ProductField(field),
        AnySymbolId::UnionPayloadField(field) => ConstantProjectionKind::UnionPayloadField(field),
        _ => return Ok(None),
    };

    values
        .intern_constant_term(ConstantTermData::Projection(ConstantProjection::new(
            subject, kind,
        )))
        .map(Some)
}

/// Restores a portable condition using the supplied callable's semantic input identities.
/// Unknown forms and exhausted traversal retain no proof meaning.
pub fn execution_condition_from_term(
    values: &SemanticValueStore,
    term: ConstantTermId,
    inputs: &[BoundReferenceTarget],
) -> Result<ExecutionCondition, SemanticValueStoreError> {
    decode(values, term, inputs, &mut {
        ExecutionCondition::WORK_LIMIT
    })
}

fn decode(
    values: &SemanticValueStore,
    term: ConstantTermId,
    inputs: &[BoundReferenceTarget],
    budget: &mut usize,
) -> Result<ExecutionCondition, SemanticValueStoreError> {
    let Some(remaining) = budget.checked_sub(1) else {
        return Ok(ExecutionCondition::Unknown);
    };

    *budget = remaining;

    Ok(match values.constant_term_data(term)?.as_ref() {
        ConstantTermData::Typed { term, .. } => decode(values, *term, inputs, budget)?,
        ConstantTermData::Value(value) => {
            let value = values.constant_value_data(*value)?;

            match value.kind() {
                ConstantValueKind::Boolean(value) => ExecutionCondition::Boolean(*value),
                ConstantValueKind::Error => ExecutionCondition::Unknown,
                _ => ExecutionCondition::Literal(value),
            }
        }
        ConstantTermData::CallableArgument(ordinal) => {
            let index = usize::try_from(ordinal.raw()).ok();

            match index {
                Some(index) if index == inputs.len() => ExecutionCondition::Result,
                Some(index) => inputs
                    .get(index)
                    .map_or(ExecutionCondition::Unknown, |input| {
                        ExecutionCondition::Input((*input).into())
                    }),
                None => ExecutionCondition::Unknown,
            }
        }
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
            let field = match projection.kind() {
                ConstantProjectionKind::ProductField(field) => Some(field.into()),
                ConstantProjectionKind::UnionPayloadField(field) => Some(field.into()),
                _ => None,
            };

            match field {
                Some(field) => ExecutionCondition::field(
                    field,
                    decode(values, projection.subject(), inputs, budget)?,
                ),
                None => ExecutionCondition::Unknown,
            }
        }
        _ => ExecutionCondition::Unknown,
    })
}
