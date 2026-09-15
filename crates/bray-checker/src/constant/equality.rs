use bray_bound_tree::BoundOperator;
use bray_symbols::{ConstantField, ConstantValueId, ConstantValueKind, SemanticValueStore};

use super::operation::fold_binary;
use crate::CheckerInfrastructureError;

pub(crate) fn constant_values_equal(
    values: &SemanticValueStore,
    left: ConstantValueId,
    right: ConstantValueId,
) -> Result<bool, CheckerInfrastructureError> {
    let left = values.constant_value_data(left);

    let right = values.constant_value_data(right);

    if left.ty() != right.ty() {
        return Ok(false);
    }

    constant_kinds_equal(values, left.kind(), right.kind())
}

fn constant_kinds_equal(
    values: &SemanticValueStore,
    left: &ConstantValueKind,
    right: &ConstantValueKind,
) -> Result<bool, CheckerInfrastructureError> {
    let equal = match (left, right) {
        (ConstantValueKind::Error, _) | (_, ConstantValueKind::Error) => false,
        (ConstantValueKind::NullablePresent(left), ConstantValueKind::NullablePresent(right)) => {
            constant_values_equal(values, *left, *right)?
        }
        (ConstantValueKind::Tuple(left), ConstantValueKind::Tuple(right))
        | (ConstantValueKind::Array(left), ConstantValueKind::Array(right)) => {
            constant_sequences_equal(values, left, right)?
        }
        (ConstantValueKind::Product(left), ConstantValueKind::Product(right)) => {
            constant_fields_equal(values, left, right)?
        }
        (
            ConstantValueKind::Union {
                variant: left_variant,
                fields: left_fields,
            },
            ConstantValueKind::Union {
                variant: right_variant,
                fields: right_fields,
            },
        ) => {
            left_variant == right_variant
                && constant_fields_equal(values, left_fields, right_fields)?
        }
        (
            ConstantValueKind::NullablePresent(_)
            | ConstantValueKind::Tuple(_)
            | ConstantValueKind::Array(_)
            | ConstantValueKind::Product(_)
            | ConstantValueKind::Union { .. },
            _,
        )
        | (
            _,
            ConstantValueKind::NullablePresent(_)
            | ConstantValueKind::Tuple(_)
            | ConstantValueKind::Array(_)
            | ConstantValueKind::Product(_)
            | ConstantValueKind::Union { .. },
        ) => false,
        _ => {
            let result = fold_binary(BoundOperator::Equal, left, right, u32::MAX)
                .map_err(|error| CheckerInfrastructureError::ConstantOperation(error.into()))?;

            let ConstantValueKind::Boolean(equal) = result else {
                return Err(CheckerInfrastructureError::InvalidConstantEvaluationInput);
            };

            equal
        }
    };

    Ok(equal)
}

fn constant_sequences_equal(
    values: &SemanticValueStore,
    left: &[ConstantValueId],
    right: &[ConstantValueId],
) -> Result<bool, CheckerInfrastructureError> {
    if left.len() != right.len() {
        return Ok(false);
    }

    for (left, right) in left.iter().zip(right) {
        if !constant_values_equal(values, *left, *right)? {
            return Ok(false);
        }
    }

    Ok(true)
}

fn constant_fields_equal<I: Eq>(
    values: &SemanticValueStore,
    left: &[ConstantField<I, ConstantValueId>],
    right: &[ConstantField<I, ConstantValueId>],
) -> Result<bool, CheckerInfrastructureError> {
    if left.len() != right.len() {
        return Ok(false);
    }

    for (left, right) in left.iter().zip(right) {
        if left.field() != right.field()
            || !constant_values_equal(values, *left.value(), *right.value())?
        {
            return Ok(false);
        }
    }

    Ok(true)
}

#[cfg(test)]
mod tests {
    use bray_symbols::{
        ConstantValueData, ConstantValueKind, RealConstantBits, SemanticValueStore, TypeData,
    };

    use super::constant_values_equal;

    #[test]
    fn floating_zero_values_use_language_equality_instead_of_bit_identity() {
        let values = SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("test value store must initialize: {error:?}"));

        let ty = values
            .intern_type(TypeData::Error)
            .unwrap_or_else(|error| panic!("test type must intern: {error:?}"));

        let positive = values
            .intern_constant_value(ConstantValueData::new(
                ty,
                ConstantValueKind::Real(RealConstantBits::Binary32(0.0_f32.to_bits())),
            ))
            .unwrap_or_else(|error| panic!("positive zero must intern: {error:?}"));

        let negative = values
            .intern_constant_value(ConstantValueData::new(
                ty,
                ConstantValueKind::Real(RealConstantBits::Binary32((-0.0_f32).to_bits())),
            ))
            .unwrap_or_else(|error| panic!("negative zero must intern: {error:?}"));

        assert_ne!(positive, negative);

        assert_eq!(constant_values_equal(&values, positive, negative), Ok(true));
    }
}
