use bray_source::{SourceSnapshot, TextRange, TextSize};
use bray_syntax::SyntaxKind;

use super::super::diagnostic;
use super::super::scan::TokenScan;
use super::super::text::{
    first_character, make_token, offset_after_character, text_size_from_usize, text_size_to_usize,
};

pub(in crate::lexer) fn scan_numeric_literal(
    snapshot: &SourceSnapshot,
    start: TextSize,
) -> TokenScan {
    let bytes = snapshot.bytes();
    let start_index = text_size_to_usize(start);

    let scan = if has_radix_prefix(bytes, start_index, b'b', b'B') {
        scan_radix_integer(bytes, start_index, Radix::Binary)
    } else if has_radix_prefix(bytes, start_index, b'x', b'X') {
        scan_radix_integer(bytes, start_index, Radix::Hexadecimal)
    } else {
        scan_decimal_numeric(bytes, start_index)
    };

    if !scan.valid {
        return make_invalid_numeric_token(snapshot, start, NumericLiteralError::Malformed);
    }

    let mut end_index = scan.end_index;
    let mut kind = scan.kind;
    let mut consumed_imaginary_suffix = false;

    if bytes.get(end_index).copied() == Some(b'i') {
        end_index += 1;
        kind = SyntaxKind::ImaginaryLiteralToken;
        consumed_imaginary_suffix = true;
    }

    if let Some(suffix_byte) = bytes
        .get(end_index)
        .copied()
        .filter(|byte| is_numeric_suffix_byte(*byte))
    {
        let error = if !consumed_imaginary_suffix && suffix_byte.is_ascii_digit() {
            NumericLiteralError::Malformed
        } else {
            NumericLiteralError::InvalidSuffix(first_character(
                snapshot,
                text_size_from_usize(end_index),
            ))
        };

        return make_invalid_numeric_token(snapshot, start, error);
    }

    TokenScan::clean(make_token(
        snapshot,
        kind,
        start,
        text_size_from_usize(end_index),
    ))
}

pub(in crate::lexer) fn scan_tuple_element_index_token(
    snapshot: &SourceSnapshot,
    start: TextSize,
) -> TokenScan {
    let bytes = snapshot.bytes();
    let start_index = text_size_to_usize(start);
    let end_index = decimal_digits_end(bytes, start_index);

    if !tuple_index_text_is_valid(bytes, start_index, end_index)
        || bytes
            .get(end_index)
            .copied()
            .is_some_and(is_numeric_suffix_byte)
    {
        return make_invalid_numeric_token(snapshot, start, NumericLiteralError::Malformed);
    }

    TokenScan::clean(make_token(
        snapshot,
        SyntaxKind::TupleElementIndexToken,
        start,
        text_size_from_usize(end_index),
    ))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct NumericScan {
    kind: SyntaxKind,
    end_index: usize,
    valid: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NumericLiteralError {
    InvalidSuffix(Option<char>),
    Malformed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Radix {
    Binary,
    Hexadecimal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DigitSequenceScan {
    end_index: usize,
    saw_digit: bool,
    valid: bool,
}

fn has_radix_prefix(bytes: &[u8], start_index: usize, lower: u8, upper: u8) -> bool {
    bytes.get(start_index).copied() == Some(b'0')
        && matches!(bytes.get(start_index + 1).copied(), Some(byte) if byte == lower || byte == upper)
}

fn scan_radix_integer(bytes: &[u8], start_index: usize, radix: Radix) -> NumericScan {
    let digits = scan_digit_sequence(bytes, start_index + 2, |byte| match radix {
        Radix::Binary => matches!(byte, b'0' | b'1'),
        Radix::Hexadecimal => byte.is_ascii_hexdigit(),
    });

    NumericScan {
        kind: match radix {
            Radix::Binary => SyntaxKind::BinaryIntegerLiteralToken,
            Radix::Hexadecimal => SyntaxKind::HexadecimalIntegerLiteralToken,
        },
        end_index: digits.end_index,
        valid: digits.valid && digits.saw_digit,
    }
}

fn scan_decimal_numeric(bytes: &[u8], start_index: usize) -> NumericScan {
    let integer_digits = scan_digit_sequence(bytes, start_index, |byte| byte.is_ascii_digit());

    let mut end_index = integer_digits.end_index;
    let mut kind = SyntaxKind::DecimalIntegerLiteralToken;
    let mut valid = integer_digits.valid && integer_digits.saw_digit;

    if bytes.get(end_index).copied() == Some(b'.')
        && matches!(bytes.get(end_index + 1).copied(), Some(byte) if byte.is_ascii_digit())
    {
        let fraction_digits =
            scan_digit_sequence(bytes, end_index + 1, |byte| byte.is_ascii_digit());

        end_index = fraction_digits.end_index;
        kind = SyntaxKind::RealLiteralToken;
        valid = valid && fraction_digits.valid && fraction_digits.saw_digit;

        if matches!(bytes.get(end_index).copied(), Some(b'e' | b'E')) {
            let exponent = scan_exponent_part(bytes, end_index);

            end_index = exponent.end_index;
            valid = valid && exponent.valid;
        }
    } else if matches!(bytes.get(end_index).copied(), Some(b'e' | b'E')) {
        let exponent = scan_exponent_part(bytes, end_index);

        end_index = exponent.end_index;
        kind = SyntaxKind::RealLiteralToken;
        valid = valid && exponent.valid;
    }

    NumericScan {
        kind,
        end_index,
        valid,
    }
}

fn scan_exponent_part(bytes: &[u8], marker_index: usize) -> DigitSequenceScan {
    let mut digit_start = marker_index + 1;

    if matches!(bytes.get(digit_start).copied(), Some(b'+' | b'-')) {
        digit_start += 1;
    }

    let digits = scan_digit_sequence(bytes, digit_start, |byte| byte.is_ascii_digit());

    DigitSequenceScan {
        end_index: digits.end_index,
        saw_digit: digits.saw_digit,
        valid: digits.valid && digits.saw_digit,
    }
}

fn scan_digit_sequence(
    bytes: &[u8],
    start_index: usize,
    is_digit: impl Fn(u8) -> bool,
) -> DigitSequenceScan {
    let mut index = start_index;
    let mut saw_digit = false;
    let mut previous_was_separator = false;
    let mut valid = true;

    while let Some(byte) = bytes.get(index).copied() {
        if is_digit(byte) {
            index += 1;

            saw_digit = true;
            previous_was_separator = false;

            continue;
        }

        if byte == b'_' {
            let next_is_digit =
                matches!(bytes.get(index + 1).copied(), Some(next) if is_digit(next));

            if !saw_digit || previous_was_separator || !next_is_digit {
                valid = false;
            }

            index += 1;

            previous_was_separator = true;

            continue;
        }

        break;
    }

    DigitSequenceScan {
        end_index: index,
        saw_digit,
        valid: valid && saw_digit && !previous_was_separator,
    }
}

fn decimal_digits_end(bytes: &[u8], start_index: usize) -> usize {
    let mut index = start_index;

    while matches!(bytes.get(index).copied(), Some(byte) if byte.is_ascii_digit()) {
        index += 1;
    }

    index
}

fn tuple_index_text_is_valid(bytes: &[u8], start_index: usize, end_index: usize) -> bool {
    if end_index == start_index + 1 {
        return true;
    }

    matches!(bytes.get(start_index).copied(), Some(b'1'..=b'9'))
}

fn is_numeric_suffix_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn make_invalid_numeric_token(
    snapshot: &SourceSnapshot,
    start: TextSize,
    error: NumericLiteralError,
) -> TokenScan {
    let end = invalid_numeric_like_end(snapshot, start);
    let range = TextRange::new(start, end);
    let token = make_token(snapshot, SyntaxKind::InvalidToken, start, end);

    let diagnostic = match error {
        NumericLiteralError::InvalidSuffix(suffix_start) => {
            diagnostic::invalid_numeric_suffix(snapshot, range, suffix_start)
        }
        NumericLiteralError::Malformed => diagnostic::malformed_numeric_literal(snapshot, range),
    };

    TokenScan::with_diagnostic(token, diagnostic)
}

fn invalid_numeric_like_end(snapshot: &SourceSnapshot, start: TextSize) -> TextSize {
    let bytes = snapshot.bytes();

    let mut index = text_size_to_usize(start);
    let mut previous_was_exponent_marker = false;

    while let Some(byte) = bytes.get(index).copied() {
        if byte.is_ascii_alphanumeric() || byte == b'_' {
            previous_was_exponent_marker = matches!(byte, b'e' | b'E');

            index += 1;

            continue;
        }

        if byte == b'.' && bytes.get(index + 1).copied() != Some(b'.') {
            previous_was_exponent_marker = false;

            index += 1;

            continue;
        }

        if matches!(byte, b'+' | b'-') && previous_was_exponent_marker {
            previous_was_exponent_marker = false;

            index += 1;

            continue;
        }

        break;
    }

    if index == text_size_to_usize(start) {
        match first_character(snapshot, start) {
            Some(character) => offset_after_character(start, character),
            None => start,
        }
    } else {
        text_size_from_usize(index)
    }
}
