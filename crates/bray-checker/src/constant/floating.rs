use bray_bound_tree::BoundOperator;
use bray_compiler_known::RepresentationRole;
use bray_symbols::{ConstantValueKind, IntegerConstant, RealConstantBits};
use rustc_apfloat::ieee::{Double, Half, Quad, Single};
use rustc_apfloat::{Float, FloatConvert, Status};

use super::integer::to_big_integer;
use super::operation::ConstantOperationError;

pub(super) fn fold_real_binary(
    operator: BoundOperator,
    left: RealConstantBits,
    right: RealConstantBits,
) -> Result<ConstantValueKind, ConstantOperationError> {
    match (left, right) {
        (RealConstantBits::Binary16(left), RealConstantBits::Binary16(right)) => {
            fold_float::<Half>(operator, u128::from(left), u128::from(right), binary16)
        }
        (RealConstantBits::Binary32(left), RealConstantBits::Binary32(right)) => {
            fold_float::<Single>(operator, u128::from(left), u128::from(right), binary32)
        }
        (RealConstantBits::Binary64(left), RealConstantBits::Binary64(right)) => {
            fold_float::<Double>(operator, u128::from(left), u128::from(right), binary64)
        }
        (RealConstantBits::Binary128(left), RealConstantBits::Binary128(right)) => {
            fold_float::<Quad>(
                operator,
                u128::from_be_bytes(left),
                u128::from_be_bytes(right),
                binary128,
            )
        }
        _ => Err(ConstantOperationError::Invalid),
    }
}

pub(super) fn fold_complex_binary(
    operator: BoundOperator,
    left_real: RealConstantBits,
    left_imaginary: RealConstantBits,
    right_real: RealConstantBits,
    right_imaginary: RealConstantBits,
) -> Result<ConstantValueKind, ConstantOperationError> {
    match operator {
        BoundOperator::Add | BoundOperator::Subtract => {
            let real = arithmetic(operator, left_real, right_real)?;
            let imaginary = arithmetic(operator, left_imaginary, right_imaginary)?;

            Ok(ConstantValueKind::Complex { real, imaginary })
        }
        BoundOperator::Multiply => {
            let real = arithmetic(
                BoundOperator::Subtract,
                arithmetic(BoundOperator::Multiply, left_real, right_real)?,
                arithmetic(BoundOperator::Multiply, left_imaginary, right_imaginary)?,
            )?;

            let imaginary = arithmetic(
                BoundOperator::Add,
                arithmetic(BoundOperator::Multiply, left_real, right_imaginary)?,
                arithmetic(BoundOperator::Multiply, left_imaginary, right_real)?,
            )?;

            Ok(ConstantValueKind::Complex { real, imaginary })
        }
        BoundOperator::Divide => {
            divide_complex(left_real, left_imaginary, right_real, right_imaginary)
        }
        BoundOperator::Equal | BoundOperator::NotEqual => {
            let equal = compare(BoundOperator::Equal, left_real, right_real)?
                && compare(BoundOperator::Equal, left_imaginary, right_imaginary)?;

            Ok(ConstantValueKind::Boolean(
                if operator == BoundOperator::Equal {
                    equal
                } else {
                    !equal
                },
            ))
        }
        _ => Err(ConstantOperationError::Invalid),
    }
}

fn divide_complex(
    left_real: RealConstantBits,
    left_imaginary: RealConstantBits,
    right_real: RealConstantBits,
    right_imaginary: RealConstantBits,
) -> Result<ConstantValueKind, ConstantOperationError> {
    if is_zero(right_real) && is_zero(right_imaginary) {
        return Err(ConstantOperationError::DivisionByZero);
    }

    let denominator = arithmetic(
        BoundOperator::Add,
        arithmetic(BoundOperator::Multiply, right_real, right_real)?,
        arithmetic(BoundOperator::Multiply, right_imaginary, right_imaginary)?,
    )?;

    let real = arithmetic(
        BoundOperator::Divide,
        arithmetic(
            BoundOperator::Add,
            arithmetic(BoundOperator::Multiply, left_real, right_real)?,
            arithmetic(BoundOperator::Multiply, left_imaginary, right_imaginary)?,
        )?,
        denominator,
    )?;

    let imaginary = arithmetic(
        BoundOperator::Divide,
        arithmetic(
            BoundOperator::Subtract,
            arithmetic(BoundOperator::Multiply, left_imaginary, right_real)?,
            arithmetic(BoundOperator::Multiply, left_real, right_imaginary)?,
        )?,
        denominator,
    )?;

    Ok(ConstantValueKind::Complex { real, imaginary })
}

fn arithmetic(
    operator: BoundOperator,
    left: RealConstantBits,
    right: RealConstantBits,
) -> Result<RealConstantBits, ConstantOperationError> {
    let ConstantValueKind::Real(result) = fold_real_binary(operator, left, right)? else {
        return Err(ConstantOperationError::Invalid);
    };

    Ok(result)
}

fn compare(
    operator: BoundOperator,
    left: RealConstantBits,
    right: RealConstantBits,
) -> Result<bool, ConstantOperationError> {
    let ConstantValueKind::Boolean(result) = fold_real_binary(operator, left, right)? else {
        return Err(ConstantOperationError::Invalid);
    };

    Ok(result)
}

fn is_zero(value: RealConstantBits) -> bool {
    match value {
        RealConstantBits::Binary16(bits) => Half::from_bits(u128::from(bits)).is_zero(),
        RealConstantBits::Binary32(bits) => Single::from_bits(u128::from(bits)).is_zero(),
        RealConstantBits::Binary64(bits) => Double::from_bits(u128::from(bits)).is_zero(),
        RealConstantBits::Binary128(bits) => Quad::from_bits(u128::from_be_bytes(bits)).is_zero(),
    }
}

fn fold_float<F>(
    operator: BoundOperator,
    left: u128,
    right: u128,
    encode: impl FnOnce(u128) -> Result<RealConstantBits, ConstantOperationError>,
) -> Result<ConstantValueKind, ConstantOperationError>
where
    F: Float,
{
    let left = F::from_bits(left);
    let right = F::from_bits(right);

    let value = match operator {
        BoundOperator::Add => (left + right).value,
        BoundOperator::Subtract => (left - right).value,
        BoundOperator::Multiply => (left * right).value,
        BoundOperator::Divide | BoundOperator::Remainder if right.is_zero() => {
            return Err(ConstantOperationError::DivisionByZero);
        }
        BoundOperator::Divide => (left / right).value,
        BoundOperator::Remainder => (left % right).value,
        BoundOperator::Equal => return Ok(ConstantValueKind::Boolean(left == right)),
        BoundOperator::NotEqual => return Ok(ConstantValueKind::Boolean(left != right)),
        BoundOperator::Less => return Ok(ConstantValueKind::Boolean(left < right)),
        BoundOperator::LessEqual => return Ok(ConstantValueKind::Boolean(left <= right)),
        BoundOperator::Greater => return Ok(ConstantValueKind::Boolean(left > right)),
        BoundOperator::GreaterEqual => return Ok(ConstantValueKind::Boolean(left >= right)),
        _ => return Err(ConstantOperationError::Invalid),
    };

    encode(value.to_bits()).map(ConstantValueKind::Real)
}

pub(super) fn convert_real(
    value: RealConstantBits,
    target: RepresentationRole,
) -> Result<RealConstantBits, ConstantOperationError> {
    match (value, target) {
        (RealConstantBits::Binary16(bits), RepresentationRole::ScalarR16) => {
            Ok(RealConstantBits::Binary16(bits))
        }
        (RealConstantBits::Binary16(bits), RepresentationRole::ScalarR32) => {
            convert_float::<Half, Single>(u128::from(bits), binary32)
        }
        (RealConstantBits::Binary16(bits), RepresentationRole::ScalarR64) => {
            convert_float::<Half, Double>(u128::from(bits), binary64)
        }
        (RealConstantBits::Binary16(bits), RepresentationRole::ScalarR128) => {
            convert_float::<Half, Quad>(u128::from(bits), binary128)
        }
        (RealConstantBits::Binary32(bits), RepresentationRole::ScalarR32) => {
            Ok(RealConstantBits::Binary32(bits))
        }
        (RealConstantBits::Binary32(bits), RepresentationRole::ScalarR64) => {
            convert_float::<Single, Double>(u128::from(bits), binary64)
        }
        (RealConstantBits::Binary32(bits), RepresentationRole::ScalarR128) => {
            convert_float::<Single, Quad>(u128::from(bits), binary128)
        }
        (RealConstantBits::Binary64(bits), RepresentationRole::ScalarR64) => {
            Ok(RealConstantBits::Binary64(bits))
        }
        (RealConstantBits::Binary64(bits), RepresentationRole::ScalarR128) => {
            convert_float::<Double, Quad>(u128::from(bits), binary128)
        }
        (RealConstantBits::Binary128(bits), RepresentationRole::ScalarR128) => {
            Ok(RealConstantBits::Binary128(bits))
        }
        _ => Err(ConstantOperationError::Invalid),
    }
}

pub(super) fn integer_to_real(
    value: &IntegerConstant,
    target: RepresentationRole,
) -> Result<RealConstantBits, ConstantOperationError> {
    match target {
        RepresentationRole::ScalarR16 => integer_to_float::<Half>(value, binary16),
        RepresentationRole::ScalarR32 => integer_to_float::<Single>(value, binary32),
        RepresentationRole::ScalarR64 => integer_to_float::<Double>(value, binary64),
        RepresentationRole::ScalarR128 => integer_to_float::<Quad>(value, binary128),
        _ => Err(ConstantOperationError::Invalid),
    }
}

fn convert_float<F, T>(
    bits: u128,
    encode: impl FnOnce(u128) -> Result<RealConstantBits, ConstantOperationError>,
) -> Result<RealConstantBits, ConstantOperationError>
where
    F: Float + FloatConvert<T>,
    T: Float,
{
    let mut loses_information = false;
    let converted = F::from_bits(bits).convert(&mut loses_information);

    if loses_information {
        return Err(ConstantOperationError::NotRepresentable);
    }

    encode(converted.value.to_bits())
}

fn integer_to_float<F>(
    value: &IntegerConstant,
    encode: impl FnOnce(u128) -> Result<RealConstantBits, ConstantOperationError>,
) -> Result<RealConstantBits, ConstantOperationError>
where
    F: Float,
{
    let value = to_big_integer(value);

    let converted = if value.sign() == num_bigint::Sign::Minus {
        let value = i128::try_from(value).map_err(|_| ConstantOperationError::NotRepresentable)?;

        F::from_i128(value)
    } else {
        let value = u128::try_from(value).map_err(|_| ConstantOperationError::NotRepresentable)?;

        F::from_u128(value)
    };

    if converted.status != Status::OK {
        return Err(ConstantOperationError::NotRepresentable);
    }

    encode(converted.value.to_bits())
}

fn binary16(bits: u128) -> Result<RealConstantBits, ConstantOperationError> {
    u16::try_from(bits)
        .map(RealConstantBits::Binary16)
        .map_err(|_| ConstantOperationError::Invalid)
}

fn binary32(bits: u128) -> Result<RealConstantBits, ConstantOperationError> {
    u32::try_from(bits)
        .map(RealConstantBits::Binary32)
        .map_err(|_| ConstantOperationError::Invalid)
}

fn binary64(bits: u128) -> Result<RealConstantBits, ConstantOperationError> {
    u64::try_from(bits)
        .map(RealConstantBits::Binary64)
        .map_err(|_| ConstantOperationError::Invalid)
}

fn binary128(bits: u128) -> Result<RealConstantBits, ConstantOperationError> {
    Ok(RealConstantBits::Binary128(bits.to_be_bytes()))
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::BoundOperator;
    use bray_compiler_known::RepresentationRole;
    use bray_symbols::{ConstantValueKind, IntegerConstant, IntegerSign, RealConstantBits};

    use super::{convert_real, fold_complex_binary, fold_real_binary, integer_to_real};
    use crate::constant::operation::ConstantOperationError;

    #[test]
    fn real_arithmetic_uses_the_selected_ieee_format() {
        let left = RealConstantBits::Binary32(0x3fc0_0000);
        let right = RealConstantBits::Binary32(0x4010_0000);

        let result = fold_real_binary(BoundOperator::Add, left, right);

        assert_eq!(
            result,
            Ok(ConstantValueKind::Real(RealConstantBits::Binary32(
                0x4070_0000
            )))
        );
    }

    #[test]
    fn complex_multiplication_uses_selected_component_arithmetic() {
        let one = RealConstantBits::Binary32(0x3f80_0000);
        let two = RealConstantBits::Binary32(0x4000_0000);
        let three = RealConstantBits::Binary32(0x4040_0000);
        let four = RealConstantBits::Binary32(0x4080_0000);

        let result = fold_complex_binary(BoundOperator::Multiply, one, two, three, four);

        assert_eq!(
            result,
            Ok(ConstantValueKind::Complex {
                real: RealConstantBits::Binary32(0xc0a0_0000),
                imaginary: RealConstantBits::Binary32(0x4120_0000),
            })
        );
    }

    #[test]
    fn real_and_complex_division_reject_zero_divisors() {
        let zero = RealConstantBits::Binary32(0);
        let one = RealConstantBits::Binary32(0x3f80_0000);

        let real = fold_real_binary(BoundOperator::Divide, one, zero);
        let complex = fold_complex_binary(BoundOperator::Divide, one, one, zero, zero);

        assert_eq!(real, Err(ConstantOperationError::DivisionByZero));
        assert_eq!(complex, Err(ConstantOperationError::DivisionByZero));
    }

    #[test]
    fn real_and_integer_widening_do_not_use_host_floats() {
        let real = convert_real(
            RealConstantBits::Binary32(0x3fc0_0000),
            RepresentationRole::ScalarR64,
        );

        let integer = integer_to_real(
            &IntegerConstant::new(IntegerSign::NonNegative, [42]),
            RepresentationRole::ScalarR32,
        );

        assert_eq!(real, Ok(RealConstantBits::Binary64(0x3ff8_0000_0000_0000)));
        assert_eq!(integer, Ok(RealConstantBits::Binary32(0x4228_0000)));
    }
}
