use bray_bound_tree::BoundOperator;
use num_bigint::BigInt;

pub(super) fn arithmetic_range(
    operator: BoundOperator,
    (left_min, left_max): (BigInt, BigInt),
    (right_min, right_max): (BigInt, BigInt),
) -> Option<(BigInt, BigInt)> {
    if !result_fits_analysis_budget(operator, &left_min, &left_max, &right_min, &right_max) {
        return None;
    }

    match operator {
        BoundOperator::Add => Some((left_min + right_min, left_max + right_max)),
        BoundOperator::Subtract => Some((left_min - right_max, left_max - right_min)),
        BoundOperator::Multiply => {
            let mut products = [
                &left_min * &right_min,
                &left_min * &right_max,
                &left_max * &right_min,
                &left_max * &right_max,
            ];

            products.sort();

            let [minimum, _, _, maximum] = products;

            Some((minimum, maximum))
        }
        BoundOperator::Divide if right_min > BigInt::from(0) || right_max < BigInt::from(0) => {
            let mut quotients = [
                &left_min / &right_min,
                &left_min / &right_max,
                &left_max / &right_min,
                &left_max / &right_max,
            ];

            quotients.sort();

            let [minimum, _, _, maximum] = quotients;

            Some((minimum, maximum))
        }
        BoundOperator::Divide if right_min == BigInt::from(0) && right_max == BigInt::from(0) => {
            None
        }
        BoundOperator::Divide => {
            let magnitude = std::cmp::max(-left_min, left_max);

            Some((-&magnitude, magnitude))
        }
        BoundOperator::Remainder
            if right_min == BigInt::from(0) && right_max == BigInt::from(0) =>
        {
            None
        }
        BoundOperator::Remainder => {
            let magnitude = std::cmp::max(-right_min, right_max) - 1;

            let minimum = if left_min < BigInt::from(0) {
                -&magnitude
            } else {
                BigInt::from(0)
            };

            let maximum = if left_max > BigInt::from(0) {
                magnitude
            } else {
                BigInt::from(0)
            };

            Some((minimum, maximum))
        }
        BoundOperator::ShiftLeft | BoundOperator::ShiftRight => {
            let minimum = usize::try_from(&right_min).ok()?;
            let maximum = usize::try_from(&right_max).ok()?;

            let mut shifted = if operator == BoundOperator::ShiftLeft {
                [
                    &left_min << minimum,
                    &left_min << maximum,
                    &left_max << minimum,
                    &left_max << maximum,
                ]
            } else {
                [
                    &left_min >> minimum,
                    &left_min >> maximum,
                    &left_max >> minimum,
                    &left_max >> maximum,
                ]
            };

            shifted.sort();

            let [minimum, _, _, maximum] = shifted;

            Some((minimum, maximum))
        }
        BoundOperator::Exponentiate => {
            let minimum = u32::try_from(&right_min).ok()?;
            let maximum = u32::try_from(&right_max).ok()?;

            let mut exponents = [
                minimum,
                maximum,
                minimum.saturating_add(1),
                maximum.saturating_sub(1),
            ];

            exponents.sort();

            let mut powers = Vec::with_capacity(9);

            for exponent in exponents
                .into_iter()
                .filter(|exponent| *exponent >= minimum && *exponent <= maximum)
            {
                powers.push(left_min.pow(exponent));
                powers.push(left_max.pow(exponent));
            }

            if left_min <= BigInt::from(0) && left_max >= BigInt::from(0) && maximum > 0 {
                powers.push(BigInt::from(0));
            }

            powers.sort();

            let maximum = powers.pop()?;
            let minimum = powers.into_iter().next()?;

            Some((minimum, maximum))
        }
        BoundOperator::BitwiseAnd
            if left_min >= BigInt::from(0) && right_min >= BigInt::from(0) =>
        {
            Some((BigInt::from(0), std::cmp::min(left_max, right_max)))
        }
        BoundOperator::BitwiseOr | BoundOperator::BitwiseXor
            if left_min >= BigInt::from(0) && right_min >= BigInt::from(0) =>
        {
            let width = usize::try_from(left_max.bits().max(right_max.bits())).ok()?;

            Some((BigInt::from(0), (BigInt::from(1) << width) - 1))
        }
        _ => None,
    }
}

fn result_fits_analysis_budget(
    operator: BoundOperator,
    left_min: &BigInt,
    left_max: &BigInt,
    right_min: &BigInt,
    right_max: &BigInt,
) -> bool {
    let left_bits = left_min.bits().max(left_max.bits());
    let right_bits = right_min.bits().max(right_max.bits());

    let bits = match operator {
        BoundOperator::Add | BoundOperator::Subtract => left_bits.max(right_bits).checked_add(1),
        BoundOperator::Multiply => left_bits.checked_add(right_bits),
        BoundOperator::ShiftLeft => u64::try_from(right_max)
            .ok()
            .and_then(|shift| left_bits.checked_add(shift)),
        BoundOperator::Exponentiate => u64::try_from(right_max)
            .ok()
            .and_then(|exponent| left_bits.checked_mul(exponent)),
        _ => Some(left_bits.max(right_bits)),
    };

    bits.is_some_and(|bits| {
        bits <= u64::from(crate::ConstantEvaluationLimits::default().integer_bits())
    })
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::BoundOperator;
    use num_bigint::BigInt;

    use super::{arithmetic_range, result_fits_analysis_budget};

    #[test]
    fn arithmetic_bounds_include_every_value_from_small_operand_intervals() {
        for operator in [
            BoundOperator::Add,
            BoundOperator::Subtract,
            BoundOperator::Multiply,
            BoundOperator::Divide,
            BoundOperator::Remainder,
        ] {
            for left_min in -4..=4 {
                for left_max in left_min..=4 {
                    for right_min in -4..=4 {
                        for right_max in right_min..=4 {
                            if matches!(operator, BoundOperator::Divide | BoundOperator::Remainder)
                                && right_min == 0
                                && right_max == 0
                            {
                                continue;
                            }

                            check_interval(operator, left_min, left_max, right_min, right_max);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn powers_shifts_and_nonnegative_bitwise_operations_cover_their_intervals() {
        for operator in [
            BoundOperator::ShiftLeft,
            BoundOperator::ShiftRight,
            BoundOperator::Exponentiate,
            BoundOperator::BitwiseAnd,
            BoundOperator::BitwiseOr,
            BoundOperator::BitwiseXor,
        ] {
            let minimum = if matches!(
                operator,
                BoundOperator::BitwiseAnd | BoundOperator::BitwiseOr | BoundOperator::BitwiseXor
            ) {
                0
            } else {
                -4
            };

            for left_min in minimum..=4 {
                for left_max in left_min..=4 {
                    for right_min in 0..=4 {
                        for right_max in right_min..=4 {
                            check_interval(operator, left_min, left_max, right_min, right_max);
                        }
                    }
                }
            }
        }
    }

    fn check_interval(
        operator: BoundOperator,
        left_min: i32,
        left_max: i32,
        right_min: i32,
        right_max: i32,
    ) {
        let (minimum, maximum) = arithmetic_range(
            operator,
            (BigInt::from(left_min), BigInt::from(left_max)),
            (BigInt::from(right_min), BigInt::from(right_max)),
        )
        .expect("small valid intervals have finite bounds");

        for left in left_min..=left_max {
            for right in right_min..=right_max {
                let value = match operator {
                    BoundOperator::Add => left + right,
                    BoundOperator::Subtract => left - right,
                    BoundOperator::Multiply => left * right,
                    BoundOperator::Divide if right != 0 => left / right,
                    BoundOperator::Remainder if right != 0 => left % right,
                    BoundOperator::Divide | BoundOperator::Remainder => continue,
                    BoundOperator::ShiftLeft => left << right,
                    BoundOperator::ShiftRight => left >> right,
                    BoundOperator::Exponentiate => {
                        left.pow(u32::try_from(right).expect("nonnegative exponent"))
                    }
                    BoundOperator::BitwiseAnd => left & right,
                    BoundOperator::BitwiseOr => left | right,
                    BoundOperator::BitwiseXor => left ^ right,
                    _ => unreachable!("test enumerates supported arithmetic operations"),
                };

                let value = BigInt::from(value);

                assert!(
                    minimum <= value && value <= maximum,
                    "{operator:?} on {left_min}..={left_max} and {right_min}..={right_max} produced {value} outside {minimum}..={maximum}"
                );
            }
        }
    }

    #[test]
    fn zero_only_divisors_and_unbounded_allocations_have_no_arithmetic_bounds() {
        for operator in [BoundOperator::Divide, BoundOperator::Remainder] {
            assert_eq!(
                arithmetic_range(
                    operator,
                    (BigInt::from(-4), BigInt::from(4)),
                    (BigInt::from(0), BigInt::from(0))
                ),
                None
            );
        }

        let limit = BigInt::from(crate::ConstantEvaluationLimits::default().integer_bits());

        assert!(!result_fits_analysis_budget(
            BoundOperator::ShiftLeft,
            &BigInt::from(1),
            &BigInt::from(2),
            &BigInt::from(0),
            &limit
        ));

        assert!(!result_fits_analysis_budget(
            BoundOperator::Exponentiate,
            &BigInt::from(-2),
            &BigInt::from(2),
            &BigInt::from(0),
            &limit
        ));

        assert!(result_fits_analysis_budget(
            BoundOperator::ShiftRight,
            &BigInt::from(-2),
            &BigInt::from(2),
            &BigInt::from(0),
            &limit
        ));
    }
}
