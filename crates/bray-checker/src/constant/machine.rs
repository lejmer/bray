use std::num::NonZeroU16;

use bray_bound_tree::BoundOperator;
use bray_compiler_known::{IntegerRepresentation, RepresentationRole};
use bray_symbols::{ConstantValueKind, IntegerConstant};
use num_bigint::BigInt;

use super::integer::{fits_integer_representation, from_big_integer, to_big_integer};
use super::operation::{ConstantOperationError, fold_binary, fold_unary};

/// Evaluates a typed machine integer unary operation. Invalid typed inputs are compiler bugs.
pub fn fold_machine_integer_unary(
    operator: BoundOperator,
    operand: &IntegerConstant,
    role: RepresentationRole,
    target_width: NonZeroU16,
) -> ConstantValueKind {
    let representation = role
        .integer_representation()
        .expect("machine integer fold requires an integer role");

    assert!(fits_integer_representation(operand, representation, || {
        target_width
    }));

    assert!(matches!(
        operator,
        BoundOperator::Subtract | BoundOperator::BitwiseNot
    ));

    let value = fold_unary(
        operator,
        &ConstantValueKind::Integer(operand.clone()),
        Some(role),
        target_width,
    )
    .expect("validated machine integer unary operation must evaluate");

    normalize_integer(value, representation, target_width)
}

/// Evaluates a typed machine integer binary operation using the checker's exact arithmetic.
/// `None` preserves undefined or trapping runtime operations such as division by zero.
pub fn fold_machine_integer_binary(
    operator: BoundOperator,
    left: &IntegerConstant,
    right: &IntegerConstant,
    representation: IntegerRepresentation,
    target_width: NonZeroU16,
) -> Option<ConstantValueKind> {
    assert!(fits_integer_representation(left, representation, || {
        target_width
    }));

    assert!(fits_integer_representation(right, representation, || {
        target_width
    }));

    let width = width(representation, target_width);

    if matches!(operator, BoundOperator::Divide | BoundOperator::Remainder)
        && matches!(
            representation,
            IntegerRepresentation::Signed(_) | IntegerRepresentation::TargetSigned
        )
        && to_big_integer(left) == -(BigInt::from(1_u8) << usize::from(width - 1))
        && to_big_integer(right) == BigInt::from(-1)
    {
        return None;
    }

    if matches!(
        operator,
        BoundOperator::ShiftLeft | BoundOperator::ShiftRight
    ) {
        let count = to_big_integer(right);

        if count < BigInt::from(0_u8) || count >= BigInt::from(width) {
            return None;
        }
    }

    let maximum_bits = u32::from(width) * 2 + 1;

    let result = match fold_binary(
        operator,
        &ConstantValueKind::Integer(left.clone()),
        &ConstantValueKind::Integer(right.clone()),
        maximum_bits,
    ) {
        Ok(result) => result,
        Err(ConstantOperationError::DivisionByZero) => return None,
        Err(ConstantOperationError::ResourceLimitExceeded { .. }) => return None,
        Err(error) => panic!("validated machine integer binary operation failed: {error:?}"),
    };

    Some(normalize_integer(result, representation, target_width))
}

/// Applies the backend's integer cast to an already typed integer constant.
pub fn fold_machine_integer_truncate(
    operand: &IntegerConstant,
    source: IntegerRepresentation,
    target: IntegerRepresentation,
    target_width: NonZeroU16,
) -> ConstantValueKind {
    assert!(fits_integer_representation(operand, source, || {
        target_width
    }));

    normalize_integer(
        ConstantValueKind::Integer(operand.clone()),
        target,
        target_width,
    )
}

fn normalize_integer(
    value: ConstantValueKind,
    representation: IntegerRepresentation,
    target_width: NonZeroU16,
) -> ConstantValueKind {
    let ConstantValueKind::Integer(value) = value else {
        return value;
    };

    let width = width(representation, target_width);
    let modulus = BigInt::from(1_u8) << usize::from(width);
    let mut reduced = to_big_integer(&value) % &modulus;

    if reduced < BigInt::from(0_u8) {
        reduced += &modulus;
    }

    if matches!(
        representation,
        IntegerRepresentation::Signed(_) | IntegerRepresentation::TargetSigned
    ) && reduced >= (&modulus >> 1)
    {
        reduced -= modulus;
    }

    ConstantValueKind::Integer(from_big_integer(reduced))
}

fn width(representation: IntegerRepresentation, target_width: NonZeroU16) -> u16 {
    match representation {
        IntegerRepresentation::Signed(width) | IntegerRepresentation::Unsigned(width) => width,
        IntegerRepresentation::TargetSigned | IntegerRepresentation::TargetUnsigned => {
            target_width.get()
        }
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU16;

    use bray_bound_tree::BoundOperator;
    use bray_compiler_known::{IntegerRepresentation, RepresentationRole};
    use bray_symbols::{ConstantValueKind, IntegerConstant, IntegerSign};

    use super::{
        fold_machine_integer_binary, fold_machine_integer_truncate, fold_machine_integer_unary,
    };

    fn integer(sign: IntegerSign, bytes: impl IntoIterator<Item = u8>) -> IntegerConstant {
        IntegerConstant::new(sign, bytes)
    }

    #[test]
    fn integer_boundaries_follow_selected_width_and_signedness() {
        let width = NonZeroU16::new(64).expect("target width is nonzero");
        let positive = IntegerSign::NonNegative;
        let negative = IntegerSign::Negative;
        let zero = integer(positive, []);
        let one = integer(positive, [1]);
        let seven = integer(positive, [7]);
        let eight = integer(positive, [8]);
        let maximum_signed = integer(positive, [0x7f]);
        let minimum_signed = integer(negative, [0x80]);
        let maximum_unsigned = integer(positive, [0xff]);
        let negative_one = integer(negative, [1]);
        let signed = IntegerRepresentation::Signed(8);
        let unsigned = IntegerRepresentation::Unsigned(8);

        let cases = [
            (
                BoundOperator::Add,
                &maximum_signed,
                &one,
                signed,
                Some(ConstantValueKind::Integer(minimum_signed.clone())),
            ),
            (
                BoundOperator::Add,
                &maximum_unsigned,
                &one,
                unsigned,
                Some(ConstantValueKind::Integer(zero.clone())),
            ),
            (
                BoundOperator::Multiply,
                &maximum_unsigned,
                &maximum_unsigned,
                unsigned,
                Some(ConstantValueKind::Integer(one.clone())),
            ),
            (BoundOperator::Divide, &one, &zero, signed, None),
            (
                BoundOperator::Divide,
                &minimum_signed,
                &negative_one,
                signed,
                None,
            ),
            (
                BoundOperator::Remainder,
                &minimum_signed,
                &negative_one,
                signed,
                None,
            ),
            (BoundOperator::Remainder, &one, &zero, unsigned, None),
            (BoundOperator::ShiftLeft, &one, &eight, unsigned, None),
            (
                BoundOperator::ShiftRight,
                &minimum_signed,
                &seven,
                signed,
                Some(ConstantValueKind::Integer(negative_one.clone())),
            ),
            (
                BoundOperator::Less,
                &minimum_signed,
                &one,
                signed,
                Some(ConstantValueKind::Boolean(true)),
            ),
            (
                BoundOperator::Greater,
                &maximum_unsigned,
                &one,
                unsigned,
                Some(ConstantValueKind::Boolean(true)),
            ),
        ];

        for (operator, left, right, representation, expected) in cases {
            assert_eq!(
                fold_machine_integer_binary(operator, left, right, representation, width),
                expected
            );
        }
    }

    #[test]
    fn unary_and_truncation_preserve_target_bits() {
        let width = NonZeroU16::new(64).expect("target width is nonzero");
        let positive = IntegerSign::NonNegative;
        let negative = IntegerSign::Negative;
        let zero = integer(positive, []);
        let signed_minimum = integer(negative, [0x80]);
        let unsigned_maximum = integer(positive, [0xff]);
        let wider = integer(positive, [0x12, 0x34]);

        assert_eq!(
            fold_machine_integer_unary(
                BoundOperator::Subtract,
                &signed_minimum,
                RepresentationRole::ScalarI8,
                width
            ),
            ConstantValueKind::Integer(signed_minimum),
        );

        assert_eq!(
            fold_machine_integer_unary(
                BoundOperator::BitwiseNot,
                &zero,
                RepresentationRole::ScalarU8,
                width
            ),
            ConstantValueKind::Integer(unsigned_maximum.clone()),
        );

        assert_eq!(
            fold_machine_integer_truncate(
                &wider,
                IntegerRepresentation::Unsigned(16),
                IntegerRepresentation::Unsigned(8),
                width
            ),
            ConstantValueKind::Integer(integer(positive, [0x34])),
        );

        assert_eq!(
            fold_machine_integer_truncate(
                &unsigned_maximum,
                IntegerRepresentation::Unsigned(8),
                IntegerRepresentation::Signed(8),
                width
            ),
            ConstantValueKind::Integer(integer(negative, [1])),
        );
    }
}
