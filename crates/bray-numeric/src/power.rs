use astro_float_num::{BigFloat, Consts, INF_POS, NAN, RoundingMode, Sign};

use crate::BinaryFormat;
use crate::complex::{product, rounded_difference, rounded_sum, sum};

const ROUND: RoundingMode = RoundingMode::ToEven;

/// Computes the principal complex power using software arithmetic in the selected format.
/// The logarithm follows the sign of the imaginary zero on the negative real axis.
/// Zero exponents and a base of one return one, including for nonfinite inputs.
pub fn power(format: BinaryFormat, base: [u128; 2], exponent: [u128; 2]) -> [u128; 2] {
    let one = format.encode(&BigFloat::from_u8(1, 128));

    if exponent.into_iter().all(|bits| format.is_zero(bits))
        || (base[0] == one && format.is_zero(base[1]))
    {
        return [one, 0];
    }

    if exponent[0] == one && format.is_zero(exponent[1]) {
        return base.map(|bits| {
            if format.is_nan(bits) {
                format.nan()
            } else {
                bits
            }
        });
    }

    if base
        .into_iter()
        .chain(exponent)
        .any(|bits| format.is_nan(bits))
    {
        return [format.nan(); 2];
    }

    // Large exponents amplify logarithm and phase errors. Preserve their integer
    // positions as well as component precision and two guard words.
    let probe = exponent.map(|bits| format.decode(bits, 128));

    let growth = probe
        .iter()
        .filter_map(BigFloat::exponent)
        .max()
        .unwrap_or(0)
        .max(0);

    let precision = working_precision(format, growth);

    let base = base.map(|bits| format.decode(bits, precision));
    let exponent = exponent.map(|bits| format.decode(bits, precision));
    let mut constants = Consts::new().expect("complex power constants allocate");
    let result = evaluate(&base, &exponent, precision, &mut constants);

    result.map(|value| format.encode(&value))
}

pub(crate) fn working_precision(format: BinaryFormat, exponent_growth: i32) -> usize {
    (format.fraction_bits()
        + 1
        + 128
        + usize::try_from(exponent_growth.max(0)).expect("positive IEEE exponent fits usize"))
    .max(256)
    .next_multiple_of(64)
}

pub(crate) fn magnitude_power(
    base: &BigFloat,
    exponent: &BigFloat,
    precision: usize,
    constants: &mut Consts,
) -> BigFloat {
    let zero = BigFloat::from_u8(0, precision);
    let one = BigFloat::from_u8(1, precision);

    if exponent.is_zero() || base.cmp(&one) == Some(0) {
        return one;
    }

    if exponent.is_inf() {
        let grows =
            (base.cmp(&one).is_some_and(|order| order > 0)) == (exponent.sign() == Some(Sign::Pos));

        return if grows { INF_POS } else { zero };
    }

    if base.is_zero() || base.is_inf() {
        let grows = (exponent.sign() == Some(Sign::Pos)) == base.is_inf();

        return if grows { INF_POS } else { zero };
    }

    if let Some((negative, count)) =
        integer_exponent(exponent, base.exponent().unwrap_or(0).unsigned_abs())
    {
        let count = usize::try_from(count).expect("bounded power exponent fits usize");
        let result = base.powi(count, precision, ROUND);

        return if negative {
            one.div(&result, precision, ROUND)
        } else {
            result
        };
    }

    // Generic adaptive pow can stall on exact dyadic results. Explicit guarded
    // composition matches the complex power policy without that convergence loop.
    base.ln(precision, ROUND, constants)
        .mul(exponent, precision, ROUND)
        .exp(precision, ROUND, constants)
}

fn evaluate(
    base: &[BigFloat; 2],
    exponent: &[BigFloat; 2],
    precision: usize,
    constants: &mut Consts,
) -> [BigFloat; 2] {
    let [a, b] = base;

    let [c, d] = exponent;

    let zero = BigFloat::from_u8(0, precision);
    let one = BigFloat::from_u8(1, precision);
    let half = one.div(&BigFloat::from_u8(2, precision), precision, ROUND);

    if d.is_zero() && c.cmp(&half) == Some(0) {
        return square_root(a, b, precision);
    }

    if b.is_zero() && a.cmp(&zero).is_some_and(|order| order > 0) && d.is_zero() {
        let real = magnitude_power(a, c, precision, constants);

        let mut imaginary = zero;
        let negative = (b.sign() == Some(Sign::Neg)) != (c.sign() == Some(Sign::Neg));

        imaginary.set_sign(if negative { Sign::Neg } else { Sign::Pos });

        return [real, imaginary];
    }

    if c.is_inf() || d.is_inf() {
        return [NAN, NAN];
    }

    if d.is_zero()
        && let Some(result) = axis_power(a, b, c, precision, constants)
    {
        return result;
    }

    let phase = argument(a, b, precision, constants);

    if (a.is_zero() && b.is_zero()) || a.is_inf() || b.is_inf() {
        if !d.is_zero() {
            return [NAN, NAN];
        }

        let grows = c.cmp(&zero).is_some_and(|order| order > 0);
        let infinite_base = a.is_inf() || b.is_inf();

        let scale = if grows == infinite_base {
            INF_POS
        } else {
            zero
        };

        let angle = phase.mul(c, precision, ROUND);

        return polar(&scale, &angle, precision, constants);
    }

    if d.is_zero() {
        if let Some((negative, mut count)) = integer_exponent(
            c,
            a.exponent()
                .unwrap_or(0)
                .unsigned_abs()
                .max(b.exponent().unwrap_or(0).unsigned_abs()),
        ) {
            let mut result = [one, zero];

            // Each clone retains a numerical accumulator that the loop mutates independently.
            let mut factor = base.clone();

            while count != 0 {
                if count & 1 != 0 {
                    result = multiply(&result, &factor, precision);
                }

                count >>= 1;

                if count != 0 {
                    factor = multiply(&factor, &factor, precision);
                }
            }

            return if negative {
                reciprocal(&result, precision)
            } else {
                result
            };
        }
    }

    let squared = sum(&product(a, a), &product(b, b));

    let logarithm = squared
        .ln(precision, ROUND, constants)
        .mul(&half, precision, ROUND);

    let magnitude = c
        .mul(&logarithm, precision, ROUND)
        .sub(&d.mul(&phase, precision, ROUND), precision, ROUND)
        .exp(precision, ROUND, constants);

    let angle =
        d.mul(&logarithm, precision, ROUND)
            .add(&c.mul(&phase, precision, ROUND), precision, ROUND);

    polar(&magnitude, &angle, precision, constants)
}

fn axis_power(
    real: &BigFloat,
    imaginary: &BigFloat,
    exponent: &BigFloat,
    precision: usize,
    constants: &mut Consts,
) -> Option<[BigFloat; 2]> {
    let (magnitude, turns) = if imaginary.is_zero() {
        let mut magnitude = real.clone(); // The magnitude must not change the source sign.

        magnitude.set_sign(Sign::Pos);

        let turns = if real.sign() == Some(Sign::Neg) {
            exponent.mul(&BigFloat::from_u8(2, precision), precision, ROUND)
        } else {
            BigFloat::from_u8(0, precision)
        };

        (magnitude, turns)
    } else if real.is_zero() {
        let mut magnitude = imaginary.clone(); // The magnitude must not change the source sign.

        magnitude.set_sign(Sign::Pos);

        (magnitude, exponent.clone()) // Retain the exponent while selecting its quadrant.
    } else if real.abs_cmp(imaginary) == Some(0) {
        let magnitude =
            sum(&product(real, real), &product(imaginary, imaginary)).sqrt(precision, ROUND);

        let numerator = BigFloat::from_u8(
            if real.sign() == Some(Sign::Neg) { 3 } else { 1 },
            precision,
        );

        let turns = exponent.mul(&numerator, precision, ROUND).div(
            &BigFloat::from_u8(2, precision),
            precision,
            ROUND,
        );

        (magnitude, turns)
    } else {
        return None;
    };

    let mut quadrant = integer_modulo_four(&turns)?;

    if imaginary.sign() == Some(Sign::Neg) {
        quadrant = (4 - quadrant) % 4;
    }

    let scale = magnitude_power(&magnitude, exponent, precision, constants);
    let mut zero = BigFloat::from_u8(0, precision);

    let negative_angle =
        (imaginary.sign() == Some(Sign::Neg)) != (exponent.sign() == Some(Sign::Neg));

    zero.set_sign(if negative_angle { Sign::Neg } else { Sign::Pos });

    Some(match quadrant {
        0 => [scale, zero],
        1 => [BigFloat::from_u8(0, precision), scale],
        2 => [scale.neg(), zero],
        3 => [BigFloat::from_u8(0, precision), scale.neg()],
        _ => unreachable!("integer quadrant is reduced modulo four"),
    })
}

pub(crate) fn integer_modulo_four(value: &BigFloat) -> Option<u8> {
    let (words, _, sign, exponent, _) = value.as_raw_parts()?;

    let total = words.len() * astro_float_num::WORD_BIT_SIZE;
    let total = i64::try_from(total).expect("numeric precision fits i64");
    let shift = total - i64::from(exponent);

    let bit = |index: i64| {
        index >= 0
            && index < total
            && (words[usize::try_from(index).expect("mantissa ordinal is nonnegative")
                / astro_float_num::WORD_BIT_SIZE]
                >> (usize::try_from(index).expect("mantissa ordinal is nonnegative")
                    % astro_float_num::WORD_BIT_SIZE))
                & 1
                != 0
    };

    if (0..shift.max(0).min(total)).any(bit) {
        return None;
    }

    let quadrant = u8::from(bit(shift)) | (u8::from(bit(shift + 1)) << 1);

    Some(if sign == Sign::Neg {
        (4 - quadrant) % 4
    } else {
        quadrant
    })
}

fn argument(
    real: &BigFloat,
    imaginary: &BigFloat,
    precision: usize,
    constants: &mut Consts,
) -> BigFloat {
    let pi = constants.pi(precision, ROUND);
    let zero = BigFloat::from_u8(0, precision);

    let mut angle = if imaginary.is_zero() {
        if real.sign() == Some(Sign::Neg) {
            pi
        } else {
            zero
        }
    } else if real.is_inf() && imaginary.is_inf() {
        let quarter = pi.div(&BigFloat::from_u8(4, precision), precision, ROUND);

        if real.sign() == Some(Sign::Neg) {
            pi.sub(&quarter, precision, ROUND)
        } else {
            quarter
        }
    } else if real.is_zero() || imaginary.is_inf() {
        pi.div(&BigFloat::from_u8(2, precision), precision, ROUND)
    } else {
        let mut real_magnitude = real.clone(); // The absolute-value input has independent sign state.
        let mut imaginary_magnitude = imaginary.clone(); // The ratio must not change the source operand.

        real_magnitude.set_sign(Sign::Pos);
        imaginary_magnitude.set_sign(Sign::Pos);

        let acute = imaginary_magnitude
            .div(&real_magnitude, precision, ROUND)
            .atan(precision, ROUND, constants);

        if real.sign() == Some(Sign::Neg) {
            pi.sub(&acute, precision, ROUND)
        } else {
            acute
        }
    };

    angle.set_sign(
        imaginary
            .sign()
            .expect("non-NaN imaginary input has a sign"),
    );

    angle
}

fn polar(
    scale: &BigFloat,
    angle: &BigFloat,
    precision: usize,
    constants: &mut Consts,
) -> [BigFloat; 2] {
    let cosine = angle.cos(precision, ROUND, constants);
    let sine = angle.sin(precision, ROUND, constants);

    [cosine, sine].map(|component| {
        if scale.is_inf() && component.is_zero() {
            component
        } else {
            scale.mul(&component, precision, ROUND)
        }
    })
}

fn multiply(left: &[BigFloat; 2], right: &[BigFloat; 2], precision: usize) -> [BigFloat; 2] {
    [
        rounded_difference(
            &left[0].mul(&right[0], precision, ROUND),
            &left[1].mul(&right[1], precision, ROUND),
            precision,
        ),
        rounded_sum(
            &left[0].mul(&right[1], precision, ROUND),
            &left[1].mul(&right[0], precision, ROUND),
            precision,
        ),
    ]
}

fn reciprocal(value: &[BigFloat; 2], precision: usize) -> [BigFloat; 2] {
    let squared = sum(
        &product(&value[0], &value[0]),
        &product(&value[1], &value[1]),
    );

    [
        value[0].div(&squared, precision, ROUND),
        value[1].neg().div(&squared, precision, ROUND),
    ]
}

fn square_root(real: &BigFloat, imaginary: &BigFloat, precision: usize) -> [BigFloat; 2] {
    let zero = BigFloat::from_u8(0, precision);
    let two = BigFloat::from_u8(2, precision);

    if imaginary.is_inf() {
        let mut signed_infinity = INF_POS;

        signed_infinity.set_sign(imaginary.sign().expect("infinity has a sign"));

        return [INF_POS, signed_infinity];
    }

    if real.is_zero() && imaginary.is_zero() {
        return [zero, imaginary.clone()]; // Preserve the input imaginary zero's sign.
    }

    let radius = sum(&product(real, real), &product(imaginary, imaginary)).sqrt(precision, ROUND);
    let negative_real = real.sign() == Some(Sign::Neg);

    let large = if negative_real {
        radius.sub(real, precision, ROUND)
    } else {
        radius.add(real, precision, ROUND)
    }
    .div(&two, precision, ROUND)
    .sqrt(precision, ROUND);

    let mut small = if large.is_inf() {
        zero
    } else {
        imaginary.div(&two.mul(&large, precision, ROUND), precision, ROUND)
    };

    small.set_sign(imaginary.sign().expect("non-NaN input has a sign"));

    if negative_real {
        let mut real = small;
        let mut imaginary_part = large;

        real.set_sign(Sign::Pos);
        imaginary_part.set_sign(imaginary.sign().expect("non-NaN input has a sign"));

        [real, imaginary_part]
    } else {
        [large, small]
    }
}

fn integer_exponent(value: &BigFloat, maximum_factor_exponent: u32) -> Option<(bool, u64)> {
    let exponent = value.exponent()?;

    if !(1..=64).contains(&exponent) {
        return None;
    }

    let (words, _, sign, _, _) = value.as_raw_parts()?;

    let bit_count = words.len() * astro_float_num::WORD_BIT_SIZE;
    let shift = bit_count - usize::try_from(exponent).ok()?;

    let bit = |index: usize| {
        (words[index / astro_float_num::WORD_BIT_SIZE] >> (index % astro_float_num::WORD_BIT_SIZE))
            & 1
            != 0
    };

    if (0..shift).any(bit) {
        return None;
    }

    let mut integer = 0_u64;

    for index in (shift..bit_count).rev() {
        integer = (integer << 1) | u64::from(bit(index));
    }

    let limit = u64::try_from(i32::MAX / 4).expect("positive power limit fits u64")
        / u64::from(maximum_factor_exponent.max(1));

    (integer <= limit).then_some((sign == Sign::Neg, integer))
}

#[cfg(test)]
mod tests {
    use super::power;
    use crate::BinaryFormat;
    use astro_float_num::BigFloat;

    #[test]
    fn powers_preserve_exact_integer_and_square_root_results_in_every_format() {
        for format in [
            BinaryFormat::Binary16,
            BinaryFormat::Binary32,
            BinaryFormat::Binary64,
            BinaryFormat::Binary128,
        ] {
            let bits = |integer| format.encode(&BigFloat::from_u8(integer, 128));
            let zero = bits(0);
            let one = bits(1);
            let two = bits(2);
            let three = bits(3);
            let four = bits(4);

            let half = format.encode(&BigFloat::from_u8(1, 128).div(
                &BigFloat::from_u8(2, 128),
                128,
                super::ROUND,
            ));

            let negative = format.sign_mask();

            assert_eq!(power(format, [one, one], [two, zero]), [zero, two]);
            assert_eq!(power(format, [three, four], [half, zero]), [two, one]);

            assert_eq!(
                power(format, [one | negative, zero], [half, zero]),
                [zero, one]
            );

            assert_eq!(
                power(format, [one | negative, negative], [half, zero]),
                [zero, one | negative]
            );

            assert_eq!(
                power(format, [format.nan(), format.infinity()], [zero, negative]),
                [one, zero]
            );

            assert_eq!(
                power(format, [one, zero], [format.nan(), format.infinity()]),
                [one, zero]
            );
        }
    }

    #[test]
    fn principal_power_uses_both_complex_exponent_components() {
        // Independent 100-decimal-digit oracle, rounded to each IEEE component format.
        for (format, one, expected) in [
            (BinaryFormat::Binary16, 0x3c00, [0x3462, 0x38ab]),
            (
                BinaryFormat::Binary32,
                0x3f80_0000,
                [0x3e8c_4420, 0x3f15_6d6a],
            ),
            (
                BinaryFormat::Binary64,
                0x3ff0_0000_0000_0000,
                [0x3fd1_8884_016c_f327, 0x3fe2_adad_36b0_98a9],
            ),
            (
                BinaryFormat::Binary128,
                0x3fff_0000_0000_0000_0000_0000_0000_0000,
                [
                    0x3ffd_1888_4016_cf32_6937_aa1f_26e9_2354,
                    0x3ffe_2ada_d36b_098a_93b4_e172_92cd_c335,
                ],
            ),
        ] {
            assert_eq!(power(format, [one; 2], [one; 2]), expected);
        }
    }
}

#[cfg(test)]
mod boundary_tests {
    use super::power;
    use crate::BinaryFormat;

    #[test]
    fn square_roots_preserve_zero_signs_and_infinite_limits_in_every_format() {
        use astro_float_num::BigFloat;

        for format in [
            BinaryFormat::Binary16,
            BinaryFormat::Binary32,
            BinaryFormat::Binary64,
            BinaryFormat::Binary128,
        ] {
            let bits = |value| format.encode(&BigFloat::from_u8(value, 128));
            let one = bits(1);
            let two = bits(2);
            let three = bits(3);
            let four = bits(4);

            let half = format.encode(&BigFloat::from_u8(1, 128).div(
                &BigFloat::from_u8(2, 128),
                128,
                super::ROUND,
            ));

            let negative = format.sign_mask();
            let infinity = format.infinity();

            for sign in [0, negative] {
                for (base, expected) in [
                    ([infinity, sign], [infinity, sign]),
                    ([infinity, one | sign], [infinity, sign]),
                    ([infinity | negative, sign], [0, infinity | sign]),
                    ([infinity | negative, one | sign], [0, infinity | sign]),
                    ([one, infinity | sign], [infinity, infinity | sign]),
                    (
                        [one | negative, infinity | sign],
                        [infinity, infinity | sign],
                    ),
                    ([infinity, infinity | sign], [infinity, infinity | sign]),
                    (
                        [infinity | negative, infinity | sign],
                        [infinity, infinity | sign],
                    ),
                    ([four, sign], [two, sign]),
                    ([four | negative, sign], [0, two | sign]),
                    ([0, sign], [0, sign]),
                    ([negative, sign], [0, sign]),
                    ([three, four | sign], [two, one | sign]),
                ] {
                    assert_eq!(
                        power(format, base, [half, 0]),
                        expected,
                        "{format:?}: {base:x?}"
                    );
                }
            }
        }
    }

    #[test]
    fn zero_magnitudes_preserve_principal_power_directions() {
        let format = BinaryFormat::Binary64;
        let negative = format.sign_mask();
        let infinity = format.infinity();
        let three_quarters = 0x3fe8_0000_0000_0000;

        for (base, exponent, expected) in [
            ([negative, 0], three_quarters, [negative, 0]),
            ([negative, negative], three_quarters, [negative, negative]),
            (
                [negative, 0],
                three_quarters | negative,
                [infinity | negative; 2],
            ),
            (
                [negative, negative],
                three_quarters | negative,
                [infinity | negative, infinity],
            ),
        ] {
            assert_eq!(power(format, base, [exponent, 0]), expected);
        }
    }

    #[test]
    fn positive_real_powers_follow_infinite_exponent_limits_in_every_format() {
        use astro_float_num::BigFloat;

        for format in [
            BinaryFormat::Binary16,
            BinaryFormat::Binary32,
            BinaryFormat::Binary64,
            BinaryFormat::Binary128,
        ] {
            let one = format.encode(&BigFloat::from_u8(1, 128));
            let two = format.encode(&BigFloat::from_u8(2, 128));

            let half = format.encode(&BigFloat::from_u8(1, 128).div(
                &BigFloat::from_u8(2, 128),
                128,
                astro_float_num::RoundingMode::ToEven,
            ));

            let negative = format.sign_mask();
            let infinity = format.infinity();

            for (base, exponent, expected_real) in [
                (two, infinity, infinity),
                (two, infinity | negative, 0),
                (half, infinity, 0),
                (half, infinity | negative, infinity),
                (infinity, infinity, infinity),
                (infinity, infinity | negative, 0),
            ] {
                for imaginary in [0, negative] {
                    let expected_imaginary = imaginary ^ (exponent & negative);

                    assert_eq!(
                        power(format, [base, imaginary], [exponent, 0]),
                        [expected_real, expected_imaginary],
                        "{format:?}: {base:x}, {imaginary:x}, {exponent:x}"
                    );
                }
            }

            for exponent in [infinity, infinity | negative] {
                assert_eq!(power(format, [one, negative], [exponent, 0]), [one, 0]);

                for base in [[two | negative, 0], [0, one], [two, one]] {
                    assert_eq!(power(format, base, [exponent, 0]), [format.nan(); 2]);
                }
            }
        }
    }

    #[test]
    fn powers_define_zero_infinity_and_nan_boundaries() {
        let format = BinaryFormat::Binary64;
        let one = 0x3ff0_0000_0000_0000;
        let two = 0x4000_0000_0000_0000;
        let half = 0x3fe0_0000_0000_0000;
        let negative = format.sign_mask();
        let infinity = format.infinity();
        let nan = format.nan();

        assert_eq!(power(format, [0, negative], [two, 0]), [0, negative]);

        assert_eq!(
            power(format, [0, 0], [one | negative, 0]),
            [infinity, negative]
        );

        assert_eq!(power(format, [0, 0], [one, one]), [nan, nan]);
        assert_eq!(power(format, [infinity, 0], [two, 0]), [infinity, 0]);

        assert_eq!(
            power(format, [infinity | negative, 0], [half, 0]),
            [0, infinity]
        );

        assert_eq!(
            power(format, [infinity, 0], [one | negative, 0]),
            [0, negative]
        );

        assert_eq!(power(format, [nan, 0], [two, 0]), [nan, nan]);
    }
}
