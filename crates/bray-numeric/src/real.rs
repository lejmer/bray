use astro_float_num::{BigFloat, Consts};

use crate::BinaryFormat;
use crate::power::{integer_modulo_four, magnitude_power, working_precision};

/// Computes a real power in the selected IEEE format without a host floating-point conversion.
/// Finite negative bases require an integral exponent. Indeterminate results use a positive quiet
/// NaN, while signed zeros and infinities follow the real-power rules documented for scalar types.
pub fn power(format: BinaryFormat, base: u128, exponent: u128) -> u128 {
    let one = format.encode(&BigFloat::from_u8(1, 128));

    if format.is_zero(exponent) || base == one {
        return one;
    }

    if format.is_nan(base) || format.is_nan(exponent) {
        return format.nan();
    }

    if exponent == one {
        return base;
    }

    let magnitude = base & !format.sign_mask();
    let negative_exponent = exponent & format.sign_mask() != 0;

    if format.is_infinite(exponent) {
        return match magnitude.cmp(&one) {
            std::cmp::Ordering::Equal => one,
            std::cmp::Ordering::Greater if !negative_exponent => format.infinity(),
            std::cmp::Ordering::Less if negative_exponent => format.infinity(),
            _ => 0,
        };
    }

    let exponent_value = format.decode(exponent, 128);
    let integer = integer_modulo_four(&exponent_value);

    let sign = if base & format.sign_mask() != 0 && integer.is_some_and(|value| value & 1 != 0) {
        format.sign_mask()
    } else {
        0
    };

    if format.is_zero(base) {
        return sign
            | if negative_exponent {
                format.infinity()
            } else {
                0
            };
    }

    if format.is_infinite(base) {
        return sign
            | if negative_exponent {
                0
            } else {
                format.infinity()
            };
    }

    if base & format.sign_mask() != 0 && integer.is_none() {
        return format.nan();
    }

    if magnitude == one {
        return one | sign;
    }

    let precision = working_precision(format, exponent_value.exponent().unwrap_or(0));
    let base = format.decode(magnitude, precision);
    let exponent = format.decode(exponent, precision);
    let mut constants = Consts::new().expect("real power constants allocate");
    let result = magnitude_power(&base, &exponent, precision, &mut constants);

    format.encode(&result) | sign
}

#[cfg(test)]
mod tests {
    use astro_float_num::BigFloat;

    use super::power;
    use crate::BinaryFormat;

    const FORMATS: [BinaryFormat; 4] = [
        BinaryFormat::Binary16,
        BinaryFormat::Binary32,
        BinaryFormat::Binary64,
        BinaryFormat::Binary128,
    ];

    fn integer(format: BinaryFormat, value: i128) -> u128 {
        let bits = format.encode(&BigFloat::from_u128(value.unsigned_abs(), 128));

        bits | if value < 0 { format.sign_mask() } else { 0 }
    }

    #[test]
    fn powers_follow_real_domain_zero_infinity_and_nan_rules() {
        for format in FORMATS {
            let one = integer(format, 1);
            let two = integer(format, 2);
            let three = integer(format, 3);
            let half = one - (1 << format.fraction_bits());
            let negative = format.sign_mask();
            let infinity = format.infinity();
            let nan = format.nan();

            for (base, exponent, expected) in [
                (nan, 0, one),
                (nan | negative | 3, negative, one),
                (one, nan, one),
                (one | negative, nan, nan),
                (nan, one, nan),
                (two, nan | negative | 3, nan),
                (0, three, 0),
                (negative, three, negative),
                (negative, two, 0),
                (negative, half, 0),
                (0, three | negative, infinity),
                (negative, three | negative, infinity | negative),
                (negative, two | negative, infinity),
                (negative, half | negative, infinity),
                (infinity, three, infinity),
                (infinity | negative, three, infinity | negative),
                (infinity | negative, two, infinity),
                (infinity | negative, half, infinity),
                (infinity, three | negative, 0),
                (infinity | negative, three | negative, negative),
                (infinity | negative, two | negative, 0),
                (infinity | negative, half | negative, 0),
                (two | negative, half, nan),
                (two | negative, one | 1, nan),
                (two | negative, three, integer(format, -8)),
                (two | negative, two, integer(format, 4)),
            ] {
                assert_eq!(
                    power(format, base, exponent),
                    expected,
                    "{format:?}: {base:x}, {exponent:x}"
                );
            }

            for exponent in [infinity, infinity | negative] {
                let grows = exponent == infinity;

                assert_eq!(power(format, one, exponent), one);
                assert_eq!(power(format, one | negative, exponent), one);

                for sign in [0, negative] {
                    assert_eq!(
                        power(format, half | sign, exponent),
                        if grows { 0 } else { infinity }
                    );

                    assert_eq!(
                        power(format, two | sign, exponent),
                        if grows { infinity } else { 0 }
                    );

                    assert_eq!(
                        power(format, sign, exponent),
                        if grows { 0 } else { infinity }
                    );

                    assert_eq!(
                        power(format, infinity | sign, exponent),
                        if grows { infinity } else { 0 }
                    );
                }
            }
        }
    }

    #[test]
    fn enormous_finite_exponents_preserve_positive_underflow_magnitudes() {
        for format in FORMATS {
            let largest = format.infinity() - 1;
            let one = integer(format, 1);

            assert_eq!(power(format, 1, largest), 0);
            assert_eq!(power(format, one | 1, largest | format.sign_mask()), 0);
            assert_eq!(crate::complex_power(format, [1, 0], [largest, 0]), [0, 0]);
        }
    }

    #[test]
    fn exponent_parity_preserves_every_selected_significand_bit() {
        for format in FORMATS {
            let one = integer(format, 1);
            let two = integer(format, 2);
            let negative = format.sign_mask();
            let precision = format.fraction_bits() + 1;
            let largest_odd = integer(format, (1_i128 << precision) - 1);
            let next_even = integer(format, 1_i128 << precision);

            assert_eq!(power(format, one | negative, largest_odd), one | negative);

            assert_eq!(
                power(format, one | negative, largest_odd | negative),
                one | negative
            );

            assert_eq!(power(format, one | negative, next_even), one);
            assert_eq!(power(format, one | negative, format.infinity() - 1), one);
            assert_eq!(power(format, one | 1, two), one | 2);

            assert_eq!(
                crate::complex_power(format, [one | 1, 0], [two, 0]),
                [one | 2, 0]
            );
        }

        let format = BinaryFormat::Binary128;
        let one = integer(format, 1);
        let beyond_u64 = integer(format, (1_i128 << 112) + 1);

        assert_eq!(
            power(format, one | format.sign_mask(), beyond_u64),
            one | format.sign_mask()
        );
    }

    #[test]
    fn transcendental_results_retain_selected_precision() {
        // Independent 160-decimal-digit oracle, rounded once to each IEEE format.
        for (format, square_root, fractional_power) in [
            (BinaryFormat::Binary16, 0x3da8, 0x408f),
            (BinaryFormat::Binary32, 0x3fb5_04f3, 0x4011_e372),
            (
                BinaryFormat::Binary64,
                0x3ff6_a09e_667f_3bcd,
                0x4002_3c6e_3224_f9d0,
            ),
            (
                BinaryFormat::Binary128,
                0x3fff_6a09_e667_f3bc_c908_b2fb_1366_ea95,
                0x4000_23c6_e322_4f9d_04ba_3630_2e14_e581,
            ),
        ] {
            let one = integer(format, 1);
            let half = one - (1 << format.fraction_bits());
            let three_quarters = half | (1 << (format.fraction_bits() - 1));

            assert_eq!(power(format, integer(format, 2), half), square_root);

            assert_eq!(
                power(format, integer(format, 3), three_quarters),
                fractional_power
            );

            assert_eq!(power(format, integer(format, 9), half), integer(format, 3));

            assert_eq!(
                power(
                    format,
                    integer(format, 16),
                    half - (1 << format.fraction_bits())
                ),
                integer(format, 2)
            );
        }
    }

    #[test]
    fn powers_round_at_overflow_and_subnormal_boundaries() {
        for (format, largest_power, smallest_power) in [
            (BinaryFormat::Binary16, 15, -24),
            (BinaryFormat::Binary32, 127, -149),
            (BinaryFormat::Binary64, 1023, -1074),
            (BinaryFormat::Binary128, 16383, -16494),
        ] {
            let two = integer(format, 2);
            let negative = format.sign_mask();
            let highest_power = format.infinity() - (1 << format.fraction_bits());

            assert_eq!(
                power(format, two, integer(format, largest_power)),
                highest_power
            );

            assert_eq!(
                power(format, two, integer(format, largest_power + 1)),
                format.infinity()
            );

            assert_eq!(power(format, two, integer(format, smallest_power)), 1);
            assert_eq!(power(format, two, integer(format, smallest_power - 1)), 0);

            assert_eq!(
                power(format, two | negative, integer(format, smallest_power - 1)),
                if (smallest_power - 1) & 1 != 0 {
                    negative
                } else {
                    0
                }
            );
        }
    }
}
