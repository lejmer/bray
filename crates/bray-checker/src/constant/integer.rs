use std::num::NonZeroU16;

use bray_compiler_known::IntegerRepresentation;
use bray_symbols::{IntegerConstant, IntegerSign};
use num_bigint::{BigInt, Sign};

pub(super) fn to_big_integer(value: &IntegerConstant) -> BigInt {
    let sign = match value.sign() {
        IntegerSign::NonNegative => Sign::Plus,
        IntegerSign::Negative => Sign::Minus,
    };

    BigInt::from_bytes_be(sign, value.magnitude())
}

pub(super) fn from_big_integer(value: BigInt) -> IntegerConstant {
    let (sign, magnitude) = value.to_bytes_be();

    let sign = match sign {
        Sign::Minus => IntegerSign::Negative,
        Sign::NoSign | Sign::Plus => IntegerSign::NonNegative,
    };

    IntegerConstant::new(sign, magnitude)
}

pub(crate) fn integer_to_usize(value: &IntegerConstant) -> Option<usize> {
    if value.sign() != IntegerSign::NonNegative {
        return None;
    }

    let mut result = 0_usize;

    for byte in value.magnitude() {
        result = result.checked_mul(256)?.checked_add(usize::from(*byte))?;
    }

    Some(result)
}

pub(crate) fn fits_integer_representation(
    value: &IntegerConstant,
    representation: IntegerRepresentation,
    target_width: impl FnOnce() -> NonZeroU16,
) -> bool {
    let width = match representation {
        IntegerRepresentation::Signed(width) | IntegerRepresentation::Unsigned(width) => width,
        IntegerRepresentation::TargetSigned | IntegerRepresentation::TargetUnsigned => {
            target_width().get()
        }
    };

    match representation {
        IntegerRepresentation::Signed(_) | IntegerRepresentation::TargetSigned => {
            fits_signed(value, width)
        }
        IntegerRepresentation::Unsigned(_) | IntegerRepresentation::TargetUnsigned => {
            value.sign() == IntegerSign::NonNegative
                && significant_bits(value.magnitude()) <= usize::from(width)
        }
    }
}

fn fits_signed(value: &IntegerConstant, width: u16) -> bool {
    let bits = significant_bits(value.magnitude());

    match value.sign() {
        IntegerSign::NonNegative => bits < usize::from(width),
        IntegerSign::Negative => {
            let maximum_bits = usize::from(width);

            bits < maximum_bits || bits == maximum_bits && is_power_of_two(value.magnitude())
        }
    }
}

pub(crate) fn significant_bits(magnitude: &[u8]) -> usize {
    magnitude.first().map_or(0, |first| {
        magnitude.len() * 8 - first.leading_zeros() as usize
    })
}

fn is_power_of_two(magnitude: &[u8]) -> bool {
    magnitude.iter().map(|byte| byte.count_ones()).sum::<u32>() == 1
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU16;

    use bray_compiler_known::IntegerRepresentation;
    use bray_symbols::{IntegerConstant, IntegerSign};

    use super::fits_integer_representation;

    #[test]
    fn signed_bounds_include_the_minimum_and_exclude_positive_overflow() {
        let width = NonZeroU16::new(8).unwrap_or(NonZeroU16::MIN);
        let minimum = IntegerConstant::new(IntegerSign::Negative, [0x80]);
        let below_minimum = IntegerConstant::new(IntegerSign::Negative, [0x81]);
        let maximum = IntegerConstant::new(IntegerSign::NonNegative, [0x7f]);
        let above_maximum = IntegerConstant::new(IntegerSign::NonNegative, [0x80]);

        assert!(fits_integer_representation(
            &minimum,
            IntegerRepresentation::Signed(8),
            || width
        ));

        assert!(!fits_integer_representation(
            &below_minimum,
            IntegerRepresentation::Signed(8),
            || width
        ));

        assert!(fits_integer_representation(
            &maximum,
            IntegerRepresentation::Signed(8),
            || width
        ));

        assert!(!fits_integer_representation(
            &above_maximum,
            IntegerRepresentation::Signed(8),
            || width
        ));
    }
}
