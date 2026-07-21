use std::num::NonZeroU16;

use rustc_apfloat::Float;
use rustc_apfloat::ieee::{Double, Half, Quad, Single};

use bray_bound_tree::BoundLiteralKind;
use bray_compiler_known::{IntegerRepresentation, RepresentationRole};
use bray_symbols::{ConstantValueKind, IntegerConstant, IntegerSign, RealConstantBits};

/// Why a source literal cannot become a constant value of its selected representation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConstantLiteralError {
    /// The literal category or spelling does not match the selected representation.
    Invalid,
    /// The normalized value is outside the selected representation's range.
    NotRepresentable,
    /// The source literal exceeds the deterministic literal-size ceiling.
    SizeLimitExceeded,
}

const MAX_INTEGER_LITERAL_BYTES: usize = 4 * 1024;

/// Checks and normalizes one source literal for an exact selected representation.
pub fn check_constant_literal(
    kind: BoundLiteralKind,
    text: &str,
    representation: RepresentationRole,
    target_integer_width_bits: impl FnOnce() -> NonZeroU16,
) -> Result<ConstantValueKind, ConstantLiteralError> {
    match kind {
        BoundLiteralKind::Integer => parse_integer(text, representation, target_integer_width_bits),
        BoundLiteralKind::Real => parse_real(text, representation).map(ConstantValueKind::Real),
        BoundLiteralKind::Imaginary => parse_imaginary(text, representation),
        BoundLiteralKind::Boolean => parse_boolean(text),
        BoundLiteralKind::Character => parse_character(text),
        BoundLiteralKind::String => parse_string(text),
    }
}

pub(super) fn parse_literal(
    kind: BoundLiteralKind,
    text: &str,
    representation: RepresentationRole,
    target_integer_width_bits: impl FnOnce() -> NonZeroU16,
) -> Result<ConstantValueKind, ConstantLiteralError> {
    check_constant_literal(kind, text, representation, target_integer_width_bits)
}

/// Normalizes an integer literal without assuming a target-selected integer width.
pub fn normalize_integer_literal(text: &str) -> Result<IntegerConstant, ConstantLiteralError> {
    if text.len() > MAX_INTEGER_LITERAL_BYTES {
        return Err(ConstantLiteralError::SizeLimitExceeded);
    }

    let (radix, digits) = integer_digits(text)?;
    let magnitude = parse_unsigned_magnitude(digits, radix)?;

    Ok(IntegerConstant::new(IntegerSign::NonNegative, magnitude))
}

fn parse_integer(
    text: &str,
    representation: RepresentationRole,
    target_integer_width_bits: impl FnOnce() -> NonZeroU16,
) -> Result<ConstantValueKind, ConstantLiteralError> {
    if text.len() > MAX_INTEGER_LITERAL_BYTES {
        return Err(ConstantLiteralError::SizeLimitExceeded);
    }

    let Some(integer_representation) = representation.integer_representation() else {
        return Err(ConstantLiteralError::Invalid);
    };

    let (radix, digits) = integer_digits(text)?;
    let magnitude = parse_unsigned_magnitude(digits, radix)?;

    if !integer_literal_fits(
        &magnitude,
        integer_representation,
        target_integer_width_bits,
    ) {
        return Err(ConstantLiteralError::NotRepresentable);
    }

    Ok(ConstantValueKind::Integer(IntegerConstant::new(
        IntegerSign::NonNegative,
        magnitude,
    )))
}

fn integer_digits(text: &str) -> Result<(u8, &str), ConstantLiteralError> {
    if let Some(digits) = text.strip_prefix("0b").or_else(|| text.strip_prefix("0B")) {
        Ok((2, digits))
    } else if let Some(digits) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        Ok((16, digits))
    } else {
        Ok((10, text))
    }
}

fn parse_unsigned_magnitude(text: &str, radix: u8) -> Result<Vec<u8>, ConstantLiteralError> {
    let mut magnitude = Vec::new();
    let mut saw_digit = false;

    for character in text.chars() {
        if character == '_' {
            continue;
        }

        let Some(digit) = character.to_digit(u32::from(radix)) else {
            return Err(ConstantLiteralError::Invalid);
        };

        let digit = u8::try_from(digit).map_err(|_| ConstantLiteralError::Invalid)?;

        saw_digit = true;

        multiply_add_magnitude(&mut magnitude, radix, digit);
    }

    if !saw_digit {
        return Err(ConstantLiteralError::Invalid);
    }

    Ok(magnitude)
}

fn multiply_add_magnitude(magnitude: &mut Vec<u8>, multiplier: u8, addend: u8) {
    let mut carry = u16::from(addend);

    for byte in magnitude.iter_mut().rev() {
        let value = u16::from(*byte) * u16::from(multiplier) + carry;

        let [high, low] = value.to_be_bytes();

        *byte = low;
        carry = u16::from(high);
    }

    if carry != 0 {
        magnitude.insert(0, carry.to_be_bytes()[1]);
    }
}

fn integer_literal_fits(
    magnitude: &[u8],
    representation: IntegerRepresentation,
    target_integer_width_bits: impl FnOnce() -> NonZeroU16,
) -> bool {
    let significant_bits = magnitude.first().map_or(0, |first| {
        magnitude.len() * 8 - first.leading_zeros() as usize
    });

    match representation {
        IntegerRepresentation::Signed(width) => significant_bits < usize::from(width),
        IntegerRepresentation::Unsigned(width) => significant_bits <= usize::from(width),
        IntegerRepresentation::TargetSigned => {
            significant_bits < usize::from(target_integer_width_bits().get())
        }
        IntegerRepresentation::TargetUnsigned => {
            significant_bits <= usize::from(target_integer_width_bits().get())
        }
    }
}

fn parse_real(
    text: &str,
    representation: RepresentationRole,
) -> Result<RealConstantBits, ConstantLiteralError> {
    let text = text.replace('_', "");

    match representation {
        RepresentationRole::ScalarR16 => {
            parse_float_bits::<Half, u16>(&text).map(RealConstantBits::Binary16)
        }
        RepresentationRole::ScalarR32 => {
            parse_float_bits::<Single, u32>(&text).map(RealConstantBits::Binary32)
        }
        RepresentationRole::ScalarR64 => {
            parse_float_bits::<Double, u64>(&text).map(RealConstantBits::Binary64)
        }
        RepresentationRole::ScalarR128 => parse_float::<Quad>(&text)
            .map(|value| RealConstantBits::Binary128(value.to_bits().to_be_bytes())),
        _ => Err(ConstantLiteralError::Invalid),
    }
}

fn parse_float_bits<F, B>(text: &str) -> Result<B, ConstantLiteralError>
where
    F: Float,
    B: TryFrom<u128>,
{
    let value = parse_float::<F>(text)?;

    B::try_from(value.to_bits()).map_err(|_| ConstantLiteralError::Invalid)
}

fn parse_float<F>(text: &str) -> Result<F, ConstantLiteralError>
where
    F: Float,
{
    let value = text
        .parse::<F>()
        .map_err(|_| ConstantLiteralError::Invalid)?;

    if !value.is_finite() {
        return Err(ConstantLiteralError::NotRepresentable);
    }

    Ok(value)
}

fn parse_imaginary(
    text: &str,
    representation: RepresentationRole,
) -> Result<ConstantValueKind, ConstantLiteralError> {
    let Some(text) = text.strip_suffix('i') else {
        return Err(ConstantLiteralError::Invalid);
    };

    if representation.numeric_kind() == Some(bray_compiler_known::NumericRepresentationKind::Real) {
        return parse_real(text, representation).map(ConstantValueKind::Real);
    }

    let Some(component) = representation.complex_component() else {
        return Err(ConstantLiteralError::Invalid);
    };

    let imaginary = parse_real(text, component)?;
    let real = zero_like(imaginary);

    Ok(ConstantValueKind::Complex { real, imaginary })
}

fn zero_like(value: RealConstantBits) -> RealConstantBits {
    match value {
        RealConstantBits::Binary16(_) => RealConstantBits::Binary16(0),
        RealConstantBits::Binary32(_) => RealConstantBits::Binary32(0),
        RealConstantBits::Binary64(_) => RealConstantBits::Binary64(0),
        RealConstantBits::Binary128(_) => RealConstantBits::Binary128([0; 16]),
    }
}

fn parse_boolean(text: &str) -> Result<ConstantValueKind, ConstantLiteralError> {
    match text {
        "true" => Ok(ConstantValueKind::Boolean(true)),
        "false" => Ok(ConstantValueKind::Boolean(false)),
        _ => Err(ConstantLiteralError::Invalid),
    }
}

fn parse_character(text: &str) -> Result<ConstantValueKind, ConstantLiteralError> {
    let Some(content) = text
        .strip_prefix('\'')
        .and_then(|text| text.strip_suffix('\''))
    else {
        return Err(ConstantLiteralError::Invalid);
    };

    let decoded = decode_quoted_content(content, BoundLiteralKind::Character)?;

    let mut characters = decoded.chars();

    let Some(character) = characters.next() else {
        return Err(ConstantLiteralError::Invalid);
    };

    if characters.next().is_some() {
        return Err(ConstantLiteralError::Invalid);
    }

    Ok(ConstantValueKind::Character(character))
}

fn parse_string(text: &str) -> Result<ConstantValueKind, ConstantLiteralError> {
    let Some(content) = text
        .strip_prefix('"')
        .and_then(|text| text.strip_suffix('"'))
    else {
        return Err(ConstantLiteralError::Invalid);
    };

    decode_quoted_content(content, BoundLiteralKind::String).map(ConstantValueKind::string)
}

fn decode_quoted_content(
    content: &str,
    kind: BoundLiteralKind,
) -> Result<String, ConstantLiteralError> {
    let mut decoded = String::new();
    let mut characters = content.chars();

    while let Some(character) = characters.next() {
        if character != '\\' {
            decoded.push(character);
            continue;
        }

        let Some(escaped) = characters.next() else {
            return Err(ConstantLiteralError::Invalid);
        };

        match escaped {
            '"' => decoded.push('"'),
            '\'' if kind == BoundLiteralKind::Character => decoded.push('\''),
            '\\' => decoded.push('\\'),
            'n' => decoded.push('\n'),
            'r' => decoded.push('\r'),
            't' => decoded.push('\t'),
            '0' => decoded.push('\0'),
            'u' => decoded.push(decode_unicode_escape(&mut characters)?),
            _ => return Err(ConstantLiteralError::Invalid),
        }
    }

    Ok(decoded)
}

fn decode_unicode_escape(
    characters: &mut impl Iterator<Item = char>,
) -> Result<char, ConstantLiteralError> {
    if characters.next() != Some('{') {
        return Err(ConstantLiteralError::Invalid);
    }

    let mut value = 0_u32;
    let mut digits = 0_u8;

    loop {
        let Some(character) = characters.next() else {
            return Err(ConstantLiteralError::Invalid);
        };

        if character == '}' {
            break;
        }

        if digits == 6 {
            return Err(ConstantLiteralError::Invalid);
        }

        let Some(digit) = character.to_digit(16) else {
            return Err(ConstantLiteralError::Invalid);
        };

        value = value * 16 + digit;

        digits += 1;
    }

    if digits == 0 {
        return Err(ConstantLiteralError::Invalid);
    }

    char::from_u32(value).ok_or(ConstantLiteralError::Invalid)
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::num::NonZeroU16;

    use bray_bound_tree::BoundLiteralKind;
    use bray_compiler_known::RepresentationRole;
    use bray_symbols::{ConstantValueKind, IntegerSign, RealConstantBits};

    use super::{ConstantLiteralError, MAX_INTEGER_LITERAL_BYTES, parse_literal};

    #[test]
    fn source_literals_are_canonicalized_for_their_selected_types() {
        let target_width = target_width();

        let integer = parse_literal(
            BoundLiteralKind::Integer,
            "0X00_ff",
            RepresentationRole::ScalarU16,
            || target_width,
        );

        let string = parse_literal(
            BoundLiteralKind::String,
            r#""a\n\u{62}""#,
            RepresentationRole::String,
            || target_width,
        );

        let real = parse_literal(
            BoundLiteralKind::Real,
            "1.5",
            RepresentationRole::ScalarR32,
            || target_width,
        );

        let Ok(ConstantValueKind::Integer(integer)) = integer else {
            panic!("integer literal must parse");
        };

        assert_eq!(integer.sign(), IntegerSign::NonNegative);
        assert_eq!(integer.magnitude(), &[0xff]);
        assert_eq!(string, Ok(ConstantValueKind::string("a\nb")));

        assert_eq!(
            parse_literal(
                BoundLiteralKind::String,
                r#""\'""#,
                RepresentationRole::String,
                || target_width,
            ),
            Err(ConstantLiteralError::Invalid)
        );

        assert_eq!(
            real,
            Ok(ConstantValueKind::Real(RealConstantBits::Binary32(
                0x3fc0_0000
            )))
        );
    }

    #[test]
    fn fixed_width_integer_literals_reject_unrepresentable_values() {
        let target_width = target_width();

        assert_eq!(
            parse_literal(
                BoundLiteralKind::Integer,
                "128",
                RepresentationRole::ScalarI8,
                || target_width,
            ),
            Err(ConstantLiteralError::NotRepresentable)
        );

        assert_eq!(
            parse_literal(
                BoundLiteralKind::Integer,
                "256",
                RepresentationRole::ScalarU8,
                || target_width,
            ),
            Err(ConstantLiteralError::NotRepresentable)
        );
    }

    #[test]
    fn target_sized_and_oversized_integer_literals_are_bounded() {
        let width32 = NonZeroU16::new(32).unwrap_or(NonZeroU16::MIN);
        let width64 = NonZeroU16::new(64).unwrap_or(NonZeroU16::MIN);

        assert_eq!(
            parse_literal(
                BoundLiteralKind::Integer,
                "4294967296",
                RepresentationRole::ScalarUsize,
                || width32,
            ),
            Err(ConstantLiteralError::NotRepresentable)
        );

        assert!(
            parse_literal(
                BoundLiteralKind::Integer,
                "4294967296",
                RepresentationRole::ScalarUsize,
                || width64,
            )
            .is_ok()
        );

        assert_eq!(
            parse_literal(
                BoundLiteralKind::Integer,
                "2147483648",
                RepresentationRole::ScalarIsize,
                || width32,
            ),
            Err(ConstantLiteralError::NotRepresentable)
        );

        assert!(
            parse_literal(
                BoundLiteralKind::Integer,
                "2147483648",
                RepresentationRole::ScalarIsize,
                || width64,
            )
            .is_ok()
        );

        assert_eq!(
            parse_literal(
                BoundLiteralKind::Integer,
                &"1".repeat(MAX_INTEGER_LITERAL_BYTES + 1),
                RepresentationRole::ScalarI128,
                || width64,
            ),
            Err(ConstantLiteralError::SizeLimitExceeded)
        );
    }

    #[test]
    fn only_target_sized_integer_literals_request_the_target_width() {
        let observations = Cell::new(0);
        let observe_width = || {
            observations.set(observations.get() + 1);

            target_width()
        };

        assert!(
            parse_literal(
                BoundLiteralKind::Integer,
                "1",
                RepresentationRole::ScalarU16,
                observe_width,
            )
            .is_ok()
        );

        assert_eq!(observations.get(), 0);

        assert!(
            parse_literal(
                BoundLiteralKind::Integer,
                "1",
                RepresentationRole::ScalarUsize,
                observe_width,
            )
            .is_ok()
        );

        assert_eq!(observations.get(), 1);
    }

    fn target_width() -> NonZeroU16 {
        NonZeroU16::new(64).unwrap_or(NonZeroU16::MIN)
    }
}
