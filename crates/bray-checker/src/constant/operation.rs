use std::num::NonZeroU16;

use bray_bound_tree::BoundOperator;
use bray_compiler_known::{IntegerRepresentation, RepresentationRole};
use bray_symbols::{ConstantValueKind, IntegerConstant, RealConstantBits};
use num_bigint::BigInt;

use super::integer::{from_big_integer, to_big_integer};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ConstantOperationError {
    Invalid,
    Unsupported,
    DivisionByZero,
    NotRepresentable,
    ResourceLimitExceeded,
}

pub(super) fn fold_unary(
    operator: BoundOperator,
    operand: &ConstantValueKind,
    representation: Option<RepresentationRole>,
    target_width: NonZeroU16,
) -> Result<ConstantValueKind, ConstantOperationError> {
    let result = match (operator, operand) {
        (BoundOperator::Add, ConstantValueKind::Integer(value)) => {
            // Integer constants are immutable, Arc-backed canonical payloads.
            ConstantValueKind::Integer(value.clone())
        }
        (BoundOperator::Subtract, ConstantValueKind::Integer(value)) => {
            integer(-to_big_integer(value))
        }
        (BoundOperator::LogicalNot, ConstantValueKind::Boolean(value)) => {
            ConstantValueKind::Boolean(!value)
        }
        (BoundOperator::BitwiseNot, ConstantValueKind::Integer(value)) => {
            let Some(integer_representation) =
                representation.and_then(RepresentationRole::integer_representation)
            else {
                return Err(ConstantOperationError::Invalid);
            };

            integer(bitwise_not(value, integer_representation, target_width))
        }
        (BoundOperator::Subtract, ConstantValueKind::Real(value)) => {
            ConstantValueKind::Real(negate_real(*value))
        }
        (BoundOperator::Subtract, ConstantValueKind::Complex { real, imaginary }) => {
            ConstantValueKind::Complex {
                real: negate_real(*real),
                imaginary: negate_real(*imaginary),
            }
        }
        _ => return Err(ConstantOperationError::Invalid),
    };

    Ok(result)
}

pub(super) fn fold_binary(
    operator: BoundOperator,
    left: &ConstantValueKind,
    right: &ConstantValueKind,
    maximum_integer_bits: u32,
) -> Result<ConstantValueKind, ConstantOperationError> {
    let result = match (left, right) {
        (ConstantValueKind::Integer(left), ConstantValueKind::Integer(right)) => {
            fold_integer_binary(operator, left, right, maximum_integer_bits)?
        }
        (ConstantValueKind::Boolean(left), ConstantValueKind::Boolean(right)) => {
            fold_boolean_binary(operator, *left, *right)?
        }
        (ConstantValueKind::Character(left), ConstantValueKind::Character(right)) => {
            fold_ordered(operator, left, right)?
        }
        (ConstantValueKind::String(left), ConstantValueKind::String(right)) => {
            fold_ordered(operator, left, right)?
        }
        (ConstantValueKind::Real(_), ConstantValueKind::Real(_))
        | (ConstantValueKind::Complex { .. }, ConstantValueKind::Complex { .. }) => {
            // TODO(BRA-122): Evaluate selected runtime-format real and complex operations.
            return Err(ConstantOperationError::Unsupported);
        }
        _ if matches!(operator, BoundOperator::Equal | BoundOperator::NotEqual) => {
            let equal = left == right;

            ConstantValueKind::Boolean(if operator == BoundOperator::Equal {
                equal
            } else {
                !equal
            })
        }
        _ => return Err(ConstantOperationError::Invalid),
    };

    Ok(result)
}

fn fold_integer_binary(
    operator: BoundOperator,
    left: &IntegerConstant,
    right: &IntegerConstant,
    maximum_integer_bits: u32,
) -> Result<ConstantValueKind, ConstantOperationError> {
    let left = to_big_integer(left);
    let right = to_big_integer(right);

    let result = match operator {
        BoundOperator::Add => integer(left + right),
        BoundOperator::Subtract => integer(left - right),
        BoundOperator::Multiply => integer(left * right),
        BoundOperator::Divide | BoundOperator::Remainder if right == BigInt::from(0_u8) => {
            return Err(ConstantOperationError::DivisionByZero);
        }
        BoundOperator::Divide => integer(left / right),
        BoundOperator::Remainder => integer(left % right),
        BoundOperator::BitwiseAnd => integer(left & right),
        BoundOperator::BitwiseOr => integer(left | right),
        BoundOperator::BitwiseXor => integer(left ^ right),
        BoundOperator::ShiftLeft => shift_left(left, &right, maximum_integer_bits)?,
        BoundOperator::ShiftRight => shift_right(left, &right)?,
        BoundOperator::Equal => ConstantValueKind::Boolean(left == right),
        BoundOperator::NotEqual => ConstantValueKind::Boolean(left != right),
        BoundOperator::Less => ConstantValueKind::Boolean(left < right),
        BoundOperator::LessEqual => ConstantValueKind::Boolean(left <= right),
        BoundOperator::Greater => ConstantValueKind::Boolean(left > right),
        BoundOperator::GreaterEqual => ConstantValueKind::Boolean(left >= right),
        BoundOperator::Exponentiate => exponentiate(left, &right, maximum_integer_bits)?,
        _ => return Err(ConstantOperationError::Invalid),
    };

    Ok(result)
}

fn fold_boolean_binary(
    operator: BoundOperator,
    left: bool,
    right: bool,
) -> Result<ConstantValueKind, ConstantOperationError> {
    let value = match operator {
        BoundOperator::LogicalAnd => left && right,
        BoundOperator::LogicalOr => left || right,
        BoundOperator::Equal => left == right,
        BoundOperator::NotEqual => left != right,
        _ => return Err(ConstantOperationError::Invalid),
    };

    Ok(ConstantValueKind::Boolean(value))
}

fn fold_ordered<T>(
    operator: BoundOperator,
    left: &T,
    right: &T,
) -> Result<ConstantValueKind, ConstantOperationError>
where
    T: Ord,
{
    let value = match operator {
        BoundOperator::Equal => left == right,
        BoundOperator::NotEqual => left != right,
        BoundOperator::Less => left < right,
        BoundOperator::LessEqual => left <= right,
        BoundOperator::Greater => left > right,
        BoundOperator::GreaterEqual => left >= right,
        _ => return Err(ConstantOperationError::Invalid),
    };

    Ok(ConstantValueKind::Boolean(value))
}

fn shift_left(
    value: BigInt,
    count: &BigInt,
    maximum_integer_bits: u32,
) -> Result<ConstantValueKind, ConstantOperationError> {
    if count < &BigInt::from(0_u8) {
        return Err(ConstantOperationError::Invalid);
    }

    if value == BigInt::from(0_u8) {
        return Ok(integer(value));
    }

    let value_bits = value.bits();
    let available_bits = u64::from(maximum_integer_bits).saturating_sub(value_bits);

    if value_bits > u64::from(maximum_integer_bits) || count > &BigInt::from(available_bits) {
        return Err(ConstantOperationError::ResourceLimitExceeded);
    }

    let count = shift_count(count)?;

    Ok(integer(value << count))
}

fn shift_right(value: BigInt, count: &BigInt) -> Result<ConstantValueKind, ConstantOperationError> {
    if count < &BigInt::from(0_u8) {
        return Err(ConstantOperationError::Invalid);
    }

    if count >= &BigInt::from(value.bits()) {
        return Ok(integer(if value < BigInt::from(0_u8) {
            BigInt::from(-1_i8)
        } else {
            BigInt::from(0_u8)
        }));
    }

    let count = shift_count(count)?;

    Ok(integer(value >> count))
}

fn exponentiate(
    value: BigInt,
    exponent: &BigInt,
    maximum_integer_bits: u32,
) -> Result<ConstantValueKind, ConstantOperationError> {
    if exponent < &BigInt::from(0_u8) {
        return Err(ConstantOperationError::Invalid);
    }

    if value == BigInt::from(0_u8) {
        return Ok(integer(if exponent == &BigInt::from(0_u8) {
            BigInt::from(1_u8)
        } else {
            value
        }));
    }

    if value == BigInt::from(1_u8) {
        return Ok(integer(value));
    }

    if value == BigInt::from(-1_i8) {
        return Ok(integer(
            if exponent % BigInt::from(2_u8) == BigInt::from(0_u8) {
                BigInt::from(1_u8)
            } else {
                value
            },
        ));
    }

    let minimum_result_bits = BigInt::from(value.bits() - 1) * exponent + BigInt::from(1_u8);

    if minimum_result_bits > BigInt::from(maximum_integer_bits) {
        return Err(ConstantOperationError::ResourceLimitExceeded);
    }

    let exponent =
        u32::try_from(exponent).map_err(|_| ConstantOperationError::ResourceLimitExceeded)?;

    let result = value.pow(exponent);

    if result.bits() > u64::from(maximum_integer_bits) {
        return Err(ConstantOperationError::ResourceLimitExceeded);
    }

    Ok(integer(result))
}

fn bitwise_not(
    value: &IntegerConstant,
    representation: IntegerRepresentation,
    target_width: NonZeroU16,
) -> BigInt {
    let value = to_big_integer(value);

    match representation {
        IntegerRepresentation::Unsigned(width) => unsigned_not(value, width),
        IntegerRepresentation::TargetUnsigned => unsigned_not(value, target_width.get()),
        IntegerRepresentation::Signed(_) | IntegerRepresentation::TargetSigned => !value,
    }
}

fn unsigned_not(value: BigInt, width: u16) -> BigInt {
    let mask = (BigInt::from(1_u8) << usize::from(width)) - BigInt::from(1_u8);

    !value & mask
}

fn integer(value: BigInt) -> ConstantValueKind {
    ConstantValueKind::Integer(from_big_integer(value))
}

fn shift_count(value: &BigInt) -> Result<usize, ConstantOperationError> {
    usize::try_from(value).map_err(|_| ConstantOperationError::ResourceLimitExceeded)
}

pub(super) fn negate_real(value: RealConstantBits) -> RealConstantBits {
    match value {
        RealConstantBits::Binary16(bits) => RealConstantBits::Binary16(bits ^ (1 << 15)),
        RealConstantBits::Binary32(bits) => RealConstantBits::Binary32(bits ^ (1 << 31)),
        RealConstantBits::Binary64(bits) => RealConstantBits::Binary64(bits ^ (1 << 63)),
        RealConstantBits::Binary128(mut bytes) => {
            bytes[0] ^= 1 << 7;

            RealConstantBits::Binary128(bytes)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU16;

    use bray_bound_tree::BoundOperator;
    use bray_compiler_known::RepresentationRole;
    use bray_symbols::{ConstantValueKind, IntegerConstant, IntegerSign};

    use super::{ConstantOperationError, fold_binary, fold_unary};

    #[test]
    fn integer_arithmetic_keeps_exact_intermediates_beyond_the_selected_width() {
        let maximum = integer([0xff]);
        let one = integer([1]);

        let intermediate = fold_binary(BoundOperator::Add, &maximum, &one, 64);

        let Ok(intermediate) = intermediate else {
            panic!("exact intermediate addition must succeed");
        };

        let result = fold_binary(BoundOperator::Subtract, &intermediate, &one, 64);

        assert_eq!(result, Ok(maximum));
    }

    #[test]
    fn unsigned_bitwise_complement_uses_the_selected_width() {
        let zero = integer([]);

        let result = fold_unary(
            BoundOperator::BitwiseNot,
            &zero,
            Some(RepresentationRole::ScalarU8),
            NonZeroU16::MIN,
        );

        assert_eq!(result, Ok(integer([0xff])));
    }

    #[test]
    fn shifts_keep_exact_intermediates_beyond_the_selected_width() {
        let one = integer([1]);
        let eight = integer([8]);

        let intermediate = fold_binary(BoundOperator::ShiftLeft, &one, &eight, 64);

        let Ok(intermediate) = intermediate else {
            panic!("exact intermediate shift must succeed");
        };

        let result = fold_binary(BoundOperator::ShiftRight, &intermediate, &eight, 64);

        assert_eq!(result, Ok(one));
    }

    #[test]
    fn exponentiation_keeps_exact_intermediates_beyond_the_selected_width() {
        let two = integer([2]);
        let eight = integer([8]);
        let one = integer([1]);

        let intermediate = fold_binary(BoundOperator::Exponentiate, &two, &eight, 64);

        let Ok(intermediate) = intermediate else {
            panic!("exact intermediate exponentiation must succeed");
        };

        let result = fold_binary(BoundOperator::Subtract, &intermediate, &one, 64);

        assert_eq!(result, Ok(integer([0xff])));
    }

    #[test]
    fn growing_integer_operations_observe_the_exact_integer_resource_limit() {
        let one = integer([1]);
        let two = integer([2]);
        let eight = integer([8]);

        let shift = fold_binary(BoundOperator::ShiftLeft, &one, &eight, 8);
        let exponentiation = fold_binary(BoundOperator::Exponentiate, &two, &eight, 8);

        assert_eq!(shift, Err(ConstantOperationError::ResourceLimitExceeded));

        assert_eq!(
            exponentiation,
            Err(ConstantOperationError::ResourceLimitExceeded)
        );
    }

    #[test]
    fn integer_division_by_zero_is_reported_explicitly() {
        let one = integer([1]);
        let zero = integer([]);

        let result = fold_binary(BoundOperator::Divide, &one, &zero, 64);

        assert_eq!(result, Err(ConstantOperationError::DivisionByZero));
    }

    fn integer(magnitude: impl IntoIterator<Item = u8>) -> ConstantValueKind {
        ConstantValueKind::Integer(IntegerConstant::new(IntegerSign::NonNegative, magnitude))
    }
}
