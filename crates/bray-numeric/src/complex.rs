use astro_float_num::{BigFloat, INF_POS, RoundingMode, Sign, WORD_BIT_SIZE};

use crate::BinaryFormat;

const INPUT_PRECISION: usize = 128;

/// Multiplies two complex values, rounding each exact finite component once to the selected format.
/// NaN results use the format's positive quiet NaN. Inputs contain the selected IEEE component bits.
pub fn multiply(format: BinaryFormat, left: [u128; 2], right: [u128; 2]) -> [u128; 2] {
    let infinite = left
        .into_iter()
        .chain(right)
        .any(|bits| format.is_infinite(bits));

    let left = decode(format, left, infinite);
    let right = decode(format, right, infinite);
    let real = difference(&product(&left[0], &right[0]), &product(&left[1], &right[1]));
    let imaginary = sum(&product(&left[0], &right[1]), &product(&left[1], &right[0]));

    encode(format, [real, imaginary], infinite.then_some(INF_POS))
}

/// Divides complex values without overflowing or underflowing intermediate products.
/// Each finite quotient component rounds once to the selected format. Zero and nonfinite
/// operands follow the complex infinity recovery rules documented for scalar arithmetic.
pub fn divide(format: BinaryFormat, left: [u128; 2], right: [u128; 2]) -> [u128; 2] {
    let left_infinite = left.into_iter().any(|bits| format.is_infinite(bits));
    let right_infinite = right.into_iter().any(|bits| format.is_infinite(bits));
    let right_zero = right.into_iter().all(|bits| format.is_zero(bits));

    if right_zero {
        let sign = right[0] & format.sign_mask();
        let infinity = format.decode(format.infinity() | sign, INPUT_PRECISION);
        let left = decode(format, left, false);

        return encode(format, left, Some(infinity));
    }

    if left_infinite && right_infinite {
        return [format.nan(); 2];
    }

    let numerator = decode(format, left, left_infinite);
    let denominator = decode(format, right, right_infinite);

    let real = sum(
        &product(&numerator[0], &denominator[0]),
        &product(&numerator[1], &denominator[1]),
    );

    let imaginary = difference(
        &product(&numerator[1], &denominator[0]),
        &product(&numerator[0], &denominator[1]),
    );

    if left_infinite || right_infinite {
        let scale = if left_infinite {
            INF_POS
        } else {
            BigFloat::from_u8(0, INPUT_PRECISION)
        };

        return encode(format, [real, imaginary], Some(scale));
    }

    let divisor = sum(
        &product(&denominator[0], &denominator[0]),
        &product(&denominator[1], &denominator[1]),
    );

    // Both exact operands contribute to midpoint separation. A tiny numerator
    // term can decide a subnormal tie even when the divisor is a power of two.
    let numerator_precision = real
        .mantissa_max_bit_len()
        .unwrap_or(INPUT_PRECISION)
        .max(imaginary.mantissa_max_bit_len().unwrap_or(INPUT_PRECISION));

    let precision = numerator_precision
        + divisor.mantissa_max_bit_len().unwrap_or(INPUT_PRECISION)
        + format.fraction_bits()
        + 1
        + WORD_BIT_SIZE;

    let real = real.div(&divisor, precision, RoundingMode::ToEven);
    let imaginary = imaginary.div(&divisor, precision, RoundingMode::ToEven);

    encode(format, [real, imaginary], None)
}

fn decode(format: BinaryFormat, bits: [u128; 2], recover_infinity: bool) -> [BigFloat; 2] {
    let has_infinity = bits.into_iter().any(|bits| format.is_infinite(bits));

    bits.map(|bits| {
        let sign = bits & format.sign_mask();

        let magnitude = if recover_infinity && has_infinity {
            if format.is_infinite(bits) {
                // The encoding of one is derived from its exact integer value.
                let one = BigFloat::from_u8(1, INPUT_PRECISION);

                format.encode(&one)
            } else {
                0
            }
        } else if recover_infinity && format.is_nan(bits) {
            0
        } else {
            bits & !format.sign_mask()
        };

        format.decode(sign | magnitude, INPUT_PRECISION)
    })
}

fn encode(format: BinaryFormat, values: [BigFloat; 2], scale: Option<BigFloat>) -> [u128; 2] {
    values.map(|value| {
        let result = if let Some(scale) = &scale {
            value.mul(scale, INPUT_PRECISION, RoundingMode::ToEven)
        } else {
            value
        };

        format.encode(&result)
    })
}

pub(crate) fn product(left: &BigFloat, right: &BigFloat) -> BigFloat {
    let precision = left.mantissa_max_bit_len().unwrap_or(INPUT_PRECISION)
        + right.mantissa_max_bit_len().unwrap_or(INPUT_PRECISION);

    let mut result = left.mul(right, precision, RoundingMode::ToEven);

    if result.is_zero() {
        result.set_sign(if left.sign() == right.sign() {
            Sign::Pos
        } else {
            Sign::Neg
        });
    }

    result
}

pub(crate) fn sum(left: &BigFloat, right: &BigFloat) -> BigFloat {
    rounded_sum(left, right, sum_precision(left, right))
}

pub(crate) fn difference(left: &BigFloat, right: &BigFloat) -> BigFloat {
    rounded_difference(left, right, sum_precision(left, right))
}

pub(crate) fn rounded_sum(left: &BigFloat, right: &BigFloat, precision: usize) -> BigFloat {
    let mut result = left.add(right, precision, RoundingMode::ToEven);

    if result.is_zero() && left.sign() != right.sign() {
        result.set_sign(Sign::Pos);
    }

    result
}

pub(crate) fn rounded_difference(left: &BigFloat, right: &BigFloat, precision: usize) -> BigFloat {
    let mut result = left.sub(right, precision, RoundingMode::ToEven);

    if result.is_zero() {
        let negative = left.is_zero()
            && right.is_zero()
            && left.sign() == Some(Sign::Neg)
            && right.sign() == Some(Sign::Pos);

        result.set_sign(if negative { Sign::Neg } else { Sign::Pos });
    }

    result
}

fn sum_precision(left: &BigFloat, right: &BigFloat) -> usize {
    if left.is_zero() {
        return right.mantissa_max_bit_len().unwrap_or(INPUT_PRECISION);
    }

    if right.is_zero() {
        return left.mantissa_max_bit_len().unwrap_or(INPUT_PRECISION);
    }

    let gap = left
        .exponent()
        .unwrap_or(0)
        .abs_diff(right.exponent().unwrap_or(0));

    left.mantissa_max_bit_len()
        .unwrap_or(INPUT_PRECISION)
        .max(right.mantissa_max_bit_len().unwrap_or(INPUT_PRECISION))
        + usize::try_from(gap).expect("IEEE exponent gap fits usize")
        + 1
}

#[cfg(test)]
mod tests {
    use super::{divide, multiply};
    use crate::BinaryFormat;

    #[test]
    fn finite_products_and_quotients_preserve_extreme_components() {
        let format = BinaryFormat::Binary64;
        let large = 0x7fef_ffff_ffff_ffff;
        let small = 1;
        let one = 0x3ff0_0000_0000_0000;
        let two = 0x4000_0000_0000_0000;

        assert_eq!(divide(format, [large, large], [large, large]), [one, 0]);
        assert_eq!(divide(format, [small, small], [small, small]), [one, 0]);

        assert_eq!(
            divide(format, [one, small], [small, 0]),
            [format.infinity(), one]
        );

        assert_eq!(
            multiply(format, [large, one], [one, small]),
            [large, one + 4]
        );

        assert_eq!(
            multiply(format, [large, large], [large, large]),
            [0, format.infinity()]
        );

        assert_eq!(multiply(format, [one, one], [one, one]), [0, two]);
    }

    #[test]
    fn complex_infinities_and_zero_divisors_have_explicit_results() {
        let format = BinaryFormat::Binary64;
        let one = 0x3ff0_0000_0000_0000;
        let infinity = format.infinity();
        let nan = format.nan();

        assert_eq!(divide(format, [one, one], [0, 0]), [infinity, infinity]);
        assert_eq!(divide(format, [0, 0], [0, 0]), [nan, nan]);
        assert_eq!(divide(format, [one, one], [infinity, infinity]), [0, 0]);

        assert_eq!(
            divide(format, [infinity, infinity], [one, 0]),
            [infinity, infinity]
        );

        assert_eq!(
            multiply(format, [infinity, nan], [one, one]),
            [infinity, infinity]
        );
    }

    #[test]
    fn division_retains_tiny_numerator_terms_that_decide_subnormal_ties() {
        let format = BinaryFormat::Binary64;
        let a = 0x3cc0_0000_0000_0000;
        let tiny = 1;
        let c = 0x7fe0_0000_0000_0000;
        let negative = 0x8000_0000_0000_0000;

        assert_eq!(divide(format, [a, tiny], [c, c]), [1, negative]);

        assert_eq!(
            divide(format, [a, negative | tiny], [c, c]),
            [0, negative | 1]
        );
    }

    #[test]
    fn extreme_division_and_multiplication_preserve_every_component_precision() {
        for (format, one) in [
            (BinaryFormat::Binary16, 0x3c00),
            (BinaryFormat::Binary32, 0x3f80_0000),
            (BinaryFormat::Binary64, 0x3ff0_0000_0000_0000),
            (
                BinaryFormat::Binary128,
                0x3fff_0000_0000_0000_0000_0000_0000_0000,
            ),
        ] {
            let largest = format.infinity() - 1;

            assert_eq!(
                divide(format, [largest, largest], [largest, largest]),
                [one, 0]
            );

            assert_eq!(divide(format, [1, 1], [1, 1]), [one, 0]);
            assert_eq!(divide(format, [one, 1], [1, 0]), [format.infinity(), one]);

            assert_eq!(
                multiply(format, [largest, largest], [largest, largest]),
                [0, format.infinity()]
            );

            assert_eq!(multiply(format, [one | 1, 0], [one, 0]), [one | 1, 0]);
        }
    }

    #[test]
    fn binary16_round_trips_every_encoding() {
        let format = BinaryFormat::Binary16;

        for bits in 0..=u16::MAX {
            let bits = u128::from(bits);

            let expected = if format.is_nan(bits) {
                format.nan()
            } else {
                bits
            };

            assert_eq!(
                format.encode(&format.decode(bits, 128)),
                expected,
                "{bits:x}"
            );
        }
    }
}
