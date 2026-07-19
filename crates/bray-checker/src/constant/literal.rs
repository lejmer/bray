use std::num::NonZeroU16;

use rustc_apfloat::Float;
use rustc_apfloat::ieee::{Double, Half, Quad, Single};

use bray_bound_tree::BoundLiteralKind;
use bray_compiler_known::{IntegerRepresentation, RepresentationRole};
use bray_symbols::{ConstantValueKind, IntegerConstant, IntegerSign, RealConstantBits};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum LiteralValueError {
    Invalid,
    NotRepresentable,
    SizeLimitExceeded,
    TargetIntegerWidthRequired,
}

const MAX_INTEGER_LITERAL_BYTES: usize = 4 * 1024;

pub(super) fn parse_literal(
    kind: BoundLiteralKind,
    text: &str,
    representation: RepresentationRole,
    target_integer_width_bits: Option<NonZeroU16>,
) -> Result<ConstantValueKind, LiteralValueError> {
    parse_literal_with_target_policy(
        kind,
        text,
        representation,
        TargetIntegerWidthPolicy::Validate(target_integer_width_bits),
    )
}

pub(super) fn parse_literal_without_target_width(
    kind: BoundLiteralKind,
    text: &str,
    representation: RepresentationRole,
) -> Result<ConstantValueKind, LiteralValueError> {
    parse_literal_with_target_policy(
        kind,
        text,
        representation,
        TargetIntegerWidthPolicy::Deferred,
    )
}

fn parse_literal_with_target_policy(
    kind: BoundLiteralKind,
    text: &str,
    representation: RepresentationRole,
    target_integer_width: TargetIntegerWidthPolicy,
) -> Result<ConstantValueKind, LiteralValueError> {
    match kind {
        BoundLiteralKind::Integer => parse_integer(text, representation, target_integer_width),
        BoundLiteralKind::Real => parse_real(text, representation).map(ConstantValueKind::Real),
        BoundLiteralKind::Imaginary => parse_imaginary(text, representation),
        BoundLiteralKind::Boolean => parse_boolean(text),
        BoundLiteralKind::Character => parse_character(text),
        BoundLiteralKind::String => parse_string(text),
    }
}

fn parse_integer(
    text: &str,
    representation: RepresentationRole,
    target_integer_width: TargetIntegerWidthPolicy,
) -> Result<ConstantValueKind, LiteralValueError> {
    if text.len() > MAX_INTEGER_LITERAL_BYTES {
        return Err(LiteralValueError::SizeLimitExceeded);
    }

    let Some(integer_representation) = representation.integer_representation() else {
        return Err(LiteralValueError::Invalid);
    };

    let (radix, digits) = integer_digits(text)?;
    let magnitude = parse_unsigned_magnitude(digits, radix)?;

    if !integer_literal_fits(&magnitude, integer_representation, target_integer_width)? {
        return Err(LiteralValueError::NotRepresentable);
    }

    Ok(ConstantValueKind::Integer(IntegerConstant::new(
        IntegerSign::NonNegative,
        magnitude,
    )))
}

fn integer_digits(text: &str) -> Result<(u8, &str), LiteralValueError> {
    if let Some(digits) = text.strip_prefix("0b").or_else(|| text.strip_prefix("0B")) {
        Ok((2, digits))
    } else if let Some(digits) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        Ok((16, digits))
    } else {
        Ok((10, text))
    }
}

fn parse_unsigned_magnitude(text: &str, radix: u8) -> Result<Vec<u8>, LiteralValueError> {
    let mut magnitude = Vec::new();
    let mut saw_digit = false;

    for character in text.chars() {
        if character == '_' {
            continue;
        }

        let Some(digit) = character.to_digit(u32::from(radix)) else {
            return Err(LiteralValueError::Invalid);
        };

        let digit = u8::try_from(digit).map_err(|_| LiteralValueError::Invalid)?;

        saw_digit = true;

        multiply_add_magnitude(&mut magnitude, radix, digit);
    }

    if !saw_digit {
        return Err(LiteralValueError::Invalid);
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
    target_integer_width: TargetIntegerWidthPolicy,
) -> Result<bool, LiteralValueError> {
    let significant_bits = magnitude.first().map_or(0, |first| {
        magnitude.len() * 8 - first.leading_zeros() as usize
    });

    let fits = match representation {
        IntegerRepresentation::Signed(width) => significant_bits < usize::from(width),
        IntegerRepresentation::Unsigned(width) => significant_bits <= usize::from(width),
        IntegerRepresentation::TargetSigned => {
            let Some(width) = target_integer_width.width()? else {
                return Ok(true);
            };

            significant_bits < usize::from(width.get())
        }
        IntegerRepresentation::TargetUnsigned => {
            let Some(width) = target_integer_width.width()? else {
                return Ok(true);
            };

            significant_bits <= usize::from(width.get())
        }
    };

    Ok(fits)
}

#[derive(Clone, Copy)]
enum TargetIntegerWidthPolicy {
    Validate(Option<NonZeroU16>),
    Deferred,
}

impl TargetIntegerWidthPolicy {
    const fn width(self) -> Result<Option<NonZeroU16>, LiteralValueError> {
        match self {
            Self::Validate(Some(width)) => Ok(Some(width)),
            Self::Validate(None) => Err(LiteralValueError::TargetIntegerWidthRequired),
            Self::Deferred => Ok(None),
        }
    }
}

fn parse_real(
    text: &str,
    representation: RepresentationRole,
) -> Result<RealConstantBits, LiteralValueError> {
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
        _ => Err(LiteralValueError::Invalid),
    }
}

fn parse_float_bits<F, B>(text: &str) -> Result<B, LiteralValueError>
where
    F: Float,
    B: TryFrom<u128>,
{
    let value = parse_float::<F>(text)?;

    B::try_from(value.to_bits()).map_err(|_| LiteralValueError::Invalid)
}

fn parse_float<F>(text: &str) -> Result<F, LiteralValueError>
where
    F: Float,
{
    let value = text.parse::<F>().map_err(|_| LiteralValueError::Invalid)?;

    if !value.is_finite() {
        return Err(LiteralValueError::NotRepresentable);
    }

    Ok(value)
}

fn parse_imaginary(
    text: &str,
    representation: RepresentationRole,
) -> Result<ConstantValueKind, LiteralValueError> {
    let Some(text) = text.strip_suffix('i') else {
        return Err(LiteralValueError::Invalid);
    };

    if representation.numeric_kind() == Some(bray_compiler_known::NumericRepresentationKind::Real) {
        return parse_real(text, representation).map(ConstantValueKind::Real);
    }

    let Some(component) = representation.complex_component() else {
        return Err(LiteralValueError::Invalid);
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

fn parse_boolean(text: &str) -> Result<ConstantValueKind, LiteralValueError> {
    match text {
        "true" => Ok(ConstantValueKind::Boolean(true)),
        "false" => Ok(ConstantValueKind::Boolean(false)),
        _ => Err(LiteralValueError::Invalid),
    }
}

fn parse_character(text: &str) -> Result<ConstantValueKind, LiteralValueError> {
    let Some(content) = text
        .strip_prefix('\'')
        .and_then(|text| text.strip_suffix('\''))
    else {
        return Err(LiteralValueError::Invalid);
    };

    let decoded = decode_quoted_content(content, BoundLiteralKind::Character)?;

    let mut characters = decoded.chars();

    let Some(character) = characters.next() else {
        return Err(LiteralValueError::Invalid);
    };

    if characters.next().is_some() {
        return Err(LiteralValueError::Invalid);
    }

    Ok(ConstantValueKind::Character(character))
}

fn parse_string(text: &str) -> Result<ConstantValueKind, LiteralValueError> {
    let Some(content) = text
        .strip_prefix('"')
        .and_then(|text| text.strip_suffix('"'))
    else {
        return Err(LiteralValueError::Invalid);
    };

    decode_quoted_content(content, BoundLiteralKind::String).map(ConstantValueKind::string)
}

fn decode_quoted_content(
    content: &str,
    kind: BoundLiteralKind,
) -> Result<String, LiteralValueError> {
    let mut decoded = String::new();
    let mut characters = content.chars();

    while let Some(character) = characters.next() {
        if character != '\\' {
            decoded.push(character);
            continue;
        }

        let Some(escaped) = characters.next() else {
            return Err(LiteralValueError::Invalid);
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
            _ => return Err(LiteralValueError::Invalid),
        }
    }

    Ok(decoded)
}

fn decode_unicode_escape(
    characters: &mut impl Iterator<Item = char>,
) -> Result<char, LiteralValueError> {
    if characters.next() != Some('{') {
        return Err(LiteralValueError::Invalid);
    }

    let mut value = 0_u32;
    let mut digits = 0_u8;

    loop {
        let Some(character) = characters.next() else {
            return Err(LiteralValueError::Invalid);
        };

        if character == '}' {
            break;
        }

        if digits == 6 {
            return Err(LiteralValueError::Invalid);
        }

        let Some(digit) = character.to_digit(16) else {
            return Err(LiteralValueError::Invalid);
        };

        value = value * 16 + digit;

        digits += 1;
    }

    if digits == 0 {
        return Err(LiteralValueError::Invalid);
    }

    char::from_u32(value).ok_or(LiteralValueError::Invalid)
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU16;

    use bray_bound_tree::BoundLiteralKind;
    use bray_compiler_known::RepresentationRole;
    use bray_symbols::{ConstantValueKind, IntegerSign, RealConstantBits};

    use super::{
        LiteralValueError, MAX_INTEGER_LITERAL_BYTES, parse_literal,
        parse_literal_without_target_width,
    };

    #[test]
    fn source_literals_are_canonicalized_for_their_selected_types() {
        let integer = parse_literal(
            BoundLiteralKind::Integer,
            "0X00_ff",
            RepresentationRole::ScalarU16,
            None,
        );

        let string = parse_literal(
            BoundLiteralKind::String,
            r#""a\n\u{62}""#,
            RepresentationRole::String,
            None,
        );

        let real = parse_literal(
            BoundLiteralKind::Real,
            "1.5",
            RepresentationRole::ScalarR32,
            None,
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
                None,
            ),
            Err(LiteralValueError::Invalid)
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
        assert_eq!(
            parse_literal(
                BoundLiteralKind::Integer,
                "128",
                RepresentationRole::ScalarI8,
                None,
            ),
            Err(LiteralValueError::NotRepresentable)
        );

        assert_eq!(
            parse_literal(
                BoundLiteralKind::Integer,
                "1",
                RepresentationRole::ScalarUsize,
                None,
            ),
            Err(LiteralValueError::TargetIntegerWidthRequired)
        );

        assert_eq!(
            parse_literal(
                BoundLiteralKind::Integer,
                "256",
                RepresentationRole::ScalarU8,
                None,
            ),
            Err(LiteralValueError::NotRepresentable)
        );
    }

    #[test]
    fn target_sized_and_oversized_integer_literals_are_bounded() {
        let width32 = NonZeroU16::new(32);
        let width64 = NonZeroU16::new(64);

        assert_eq!(
            parse_literal(
                BoundLiteralKind::Integer,
                "4294967296",
                RepresentationRole::ScalarUsize,
                width32,
            ),
            Err(LiteralValueError::NotRepresentable)
        );

        assert!(
            parse_literal(
                BoundLiteralKind::Integer,
                "4294967296",
                RepresentationRole::ScalarUsize,
                width64,
            )
            .is_ok()
        );

        assert!(matches!(
            parse_literal_without_target_width(
                BoundLiteralKind::Integer,
                "4294967296",
                RepresentationRole::ScalarUsize,
            ),
            Ok(ConstantValueKind::Integer(_))
        ));

        assert_eq!(
            parse_literal(
                BoundLiteralKind::Integer,
                "2147483648",
                RepresentationRole::ScalarIsize,
                width32,
            ),
            Err(LiteralValueError::NotRepresentable)
        );

        assert!(
            parse_literal(
                BoundLiteralKind::Integer,
                "2147483648",
                RepresentationRole::ScalarIsize,
                width64,
            )
            .is_ok()
        );

        assert_eq!(
            parse_literal(
                BoundLiteralKind::Integer,
                &"1".repeat(MAX_INTEGER_LITERAL_BYTES + 1),
                RepresentationRole::ScalarI128,
                None,
            ),
            Err(LiteralValueError::SizeLimitExceeded)
        );
    }
}
