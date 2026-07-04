use bray_source::{SourceSnapshot, TextSize};
use bray_syntax::{SyntaxKind, SyntaxToken};

use super::text::{
    character_len_at, first_character, make_token, offset_after_character, text_size_from_usize,
    text_size_to_usize,
};

pub(super) fn scan_numeric_literal(snapshot: &SourceSnapshot, start: TextSize) -> SyntaxToken {
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
        return make_invalid_numeric_token(snapshot, start);
    }

    let mut end_index = scan.end_index;
    let mut kind = scan.kind;

    if bytes.get(end_index).copied() == Some(b'i') {
        end_index += 1;
        kind = SyntaxKind::ImaginaryLiteralToken;
    }

    if is_numeric_suffix_byte(bytes.get(end_index).copied()) {
        return make_invalid_numeric_token(snapshot, start);
    }

    make_token(snapshot, kind, start, text_size_from_usize(end_index))
}

pub(super) fn scan_tuple_element_index_token(
    snapshot: &SourceSnapshot,
    start: TextSize,
) -> SyntaxToken {
    let bytes = snapshot.bytes();
    let start_index = text_size_to_usize(start);
    let end_index = decimal_digits_end(bytes, start_index);

    if !tuple_index_text_is_valid(bytes, start_index, end_index)
        || is_numeric_suffix_byte(bytes.get(end_index).copied())
    {
        return make_invalid_numeric_token(snapshot, start);
    }

    make_token(
        snapshot,
        SyntaxKind::TupleElementIndexToken,
        start,
        text_size_from_usize(end_index),
    )
}

pub(super) fn scan_character_literal(snapshot: &SourceSnapshot, start: TextSize) -> SyntaxToken {
    let start_index = text_size_to_usize(start);
    let body_index = start_index + 1;

    let body = match scan_character_literal_body(snapshot, body_index) {
        CharacterBodyScan::Valid(end_index) => end_index,
        CharacterBodyScan::Invalid => {
            return make_invalid_quoted_literal_token(snapshot, start, b'\'');
        }
    };

    if snapshot.bytes().get(body).copied() != Some(b'\'') {
        return make_invalid_quoted_literal_token(snapshot, start, b'\'');
    }

    make_token(
        snapshot,
        SyntaxKind::CharacterLiteralToken,
        start,
        text_size_from_usize(body + 1),
    )
}

pub(super) fn scan_string_literal(snapshot: &SourceSnapshot, start: TextSize) -> SyntaxToken {
    let mut index = text_size_to_usize(start) + 1;

    let bytes = snapshot.bytes();

    loop {
        match bytes.get(index).copied() {
            Some(b'"') => {
                return make_token(
                    snapshot,
                    SyntaxKind::StringLiteralToken,
                    start,
                    text_size_from_usize(index + 1),
                );
            }
            Some(b'\n' | b'\r') | None => {
                return make_invalid_quoted_literal_token(snapshot, start, b'"');
            }
            Some(b'\\') => match scan_escape_sequence(snapshot, index, EscapeMode::String) {
                EscapeScan::Valid(end_index) => {
                    index = end_index;
                }
                EscapeScan::Invalid => {
                    return make_invalid_quoted_literal_token(snapshot, start, b'"');
                }
            },
            Some(byte) if byte.is_ascii() => {
                index += 1;
            }
            Some(_) => match character_len_at(snapshot, index) {
                Some(character_len) => {
                    index += character_len;
                }
                None => return make_invalid_quoted_literal_token(snapshot, start, b'"'),
            },
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct NumericScan {
    kind: SyntaxKind,
    end_index: usize,
    valid: bool,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CharacterBodyScan {
    Valid(usize),
    Invalid,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EscapeMode {
    Character,
    String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EscapeScan {
    Valid(usize),
    Invalid,
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

fn is_numeric_suffix_byte(byte: Option<u8>) -> bool {
    matches!(byte, Some(byte) if byte.is_ascii_alphanumeric() || byte == b'_')
}

fn make_invalid_numeric_token(snapshot: &SourceSnapshot, start: TextSize) -> SyntaxToken {
    let end = invalid_numeric_like_end(snapshot, start);

    make_token(snapshot, SyntaxKind::InvalidToken, start, end)
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

fn scan_character_literal_body(snapshot: &SourceSnapshot, index: usize) -> CharacterBodyScan {
    match snapshot.bytes().get(index).copied() {
        Some(b'\\') => match scan_escape_sequence(snapshot, index, EscapeMode::Character) {
            EscapeScan::Valid(end_index) => CharacterBodyScan::Valid(end_index),
            EscapeScan::Invalid => CharacterBodyScan::Invalid,
        },
        Some(b'\'' | b'\n' | b'\r') | None => CharacterBodyScan::Invalid,
        Some(byte) if byte.is_ascii() => CharacterBodyScan::Valid(index + 1),
        Some(_) => match character_len_at(snapshot, index) {
            Some(character_len) => CharacterBodyScan::Valid(index + character_len),
            None => CharacterBodyScan::Invalid,
        },
    }
}

fn scan_escape_sequence(
    snapshot: &SourceSnapshot,
    backslash_index: usize,
    mode: EscapeMode,
) -> EscapeScan {
    let bytes = snapshot.bytes();

    match bytes.get(backslash_index + 1).copied() {
        Some(b'"' | b'\\' | b'n' | b'r' | b't' | b'0') => EscapeScan::Valid(backslash_index + 2),
        Some(b'\'') if mode == EscapeMode::Character => EscapeScan::Valid(backslash_index + 2),
        Some(b'u') => scan_unicode_escape(bytes, backslash_index),
        _ => EscapeScan::Invalid,
    }
}

fn scan_unicode_escape(bytes: &[u8], backslash_index: usize) -> EscapeScan {
    let mut index = backslash_index + 2;

    if bytes.get(index).copied() != Some(b'{') {
        return EscapeScan::Invalid;
    }

    index += 1;

    let digits_start = index;

    let mut value = 0_u32;

    while let Some(byte) = bytes.get(index).copied() {
        if byte == b'}' {
            break;
        }

        if index - digits_start == 6 {
            return EscapeScan::Invalid;
        }

        let digit = match hex_digit_value(byte) {
            Some(digit) => digit,
            None => return EscapeScan::Invalid,
        };

        value = match value
            .checked_mul(16)
            .and_then(|value| value.checked_add(digit))
        {
            Some(value) => value,
            None => return EscapeScan::Invalid,
        };

        index += 1;
    }

    if index == digits_start || bytes.get(index).copied() != Some(b'}') {
        return EscapeScan::Invalid;
    }

    if char::from_u32(value).is_none() {
        return EscapeScan::Invalid;
    }

    EscapeScan::Valid(index + 1)
}

fn hex_digit_value(byte: u8) -> Option<u32> {
    match byte {
        b'0'..=b'9' => Some(u32::from(byte - b'0')),
        b'a'..=b'f' => Some(u32::from(byte - b'a') + 10),
        b'A'..=b'F' => Some(u32::from(byte - b'A') + 10),
        _ => None,
    }
}

fn make_invalid_quoted_literal_token(
    snapshot: &SourceSnapshot,
    start: TextSize,
    quote: u8,
) -> SyntaxToken {
    make_token(
        snapshot,
        SyntaxKind::InvalidToken,
        start,
        quoted_literal_recovery_end(snapshot, start, quote),
    )
}

fn quoted_literal_recovery_end(snapshot: &SourceSnapshot, start: TextSize, quote: u8) -> TextSize {
    let bytes = snapshot.bytes();

    let mut index = text_size_to_usize(start) + 1;

    while let Some(byte) = bytes.get(index).copied() {
        if matches!(byte, b'\n' | b'\r') {
            break;
        }

        if byte == quote {
            index += 1;
            break;
        }

        if byte == b'\\' {
            index += 1;

            if let Some(character_len) = character_len_at(snapshot, index) {
                index += character_len;
            }

            continue;
        }

        match character_len_at(snapshot, index) {
            Some(character_len) => {
                index += character_len;
            }
            None => break,
        }
    }

    text_size_from_usize(index)
}
