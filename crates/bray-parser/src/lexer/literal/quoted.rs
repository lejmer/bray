use bray_source::{SourceSnapshot, TextRange, TextSize};
use bray_syntax::SyntaxKind;

use super::super::diagnostic;
use super::super::scan::TokenScan;
use super::super::text::{
    character_len_at, first_character, make_token, text_size_from_usize, text_size_to_usize,
};

pub(in crate::lexer) fn scan_character_literal(
    snapshot: &SourceSnapshot,
    start: TextSize,
) -> TokenScan {
    let start_index = text_size_to_usize(start);
    let body_index = start_index + 1;

    let body = match scan_character_literal_body(snapshot, body_index) {
        CharacterBodyScan::Valid(end_index) => end_index,
        CharacterBodyScan::Invalid(error) => {
            return make_invalid_character_literal_token(snapshot, start, error);
        }
    };

    match snapshot.bytes().get(body).copied() {
        Some(b'\'') => TokenScan::clean(make_token(
            SyntaxKind::CharacterLiteralToken,
            start,
            text_size_from_usize(body + 1),
        )),
        Some(b'\n' | b'\r') | None => make_invalid_character_literal_token(
            snapshot,
            start,
            CharacterLiteralError::Unterminated,
        ),
        Some(_) => {
            make_invalid_character_literal_token(snapshot, start, CharacterLiteralError::Malformed)
        }
    }
}

pub(in crate::lexer) fn scan_string_literal(
    snapshot: &SourceSnapshot,
    start: TextSize,
    byte_string: bool,
) -> TokenScan {
    let mut index = text_size_to_usize(start) + 1 + usize::from(byte_string);
    let bytes = snapshot.bytes();

    loop {
        match bytes.get(index).copied() {
            Some(b'"') => {
                return TokenScan::clean(make_token(
                    if byte_string {
                        SyntaxKind::ByteStringLiteralToken
                    } else {
                        SyntaxKind::StringLiteralToken
                    },
                    start,
                    text_size_from_usize(index + 1),
                ));
            }
            Some(b'\n' | b'\r') | None => {
                return make_invalid_string_literal_token(
                    snapshot,
                    start,
                    StringLiteralError::Unterminated,
                );
            }
            Some(b'\\') => match scan_escape_sequence(
                snapshot,
                index,
                if byte_string {
                    EscapeMode::ByteString
                } else {
                    EscapeMode::String
                },
            ) {
                EscapeScan::Valid(end_index) => {
                    index = end_index;
                }
                EscapeScan::Invalid(EscapeError::Unterminated) => {
                    return make_invalid_string_literal_token(
                        snapshot,
                        start,
                        StringLiteralError::Unterminated,
                    );
                }
                EscapeScan::Invalid(EscapeError::Unknown(character)) => {
                    return make_invalid_string_literal_token(
                        snapshot,
                        start,
                        StringLiteralError::UnknownEscape(character),
                    );
                }
                EscapeScan::Invalid(EscapeError::InvalidUnicode) => {
                    return make_invalid_string_literal_token(
                        snapshot,
                        start,
                        StringLiteralError::InvalidUnicodeEscape,
                    );
                }
            },
            Some(byte) if byte.is_ascii() => {
                index += 1;
            }
            Some(_) => match character_len_at(snapshot, index) {
                Some(character_len) => {
                    index += character_len;
                }
                None => {
                    return make_invalid_string_literal_token(
                        snapshot,
                        start,
                        StringLiteralError::Unterminated,
                    );
                }
            },
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CharacterBodyScan {
    Valid(usize),
    Invalid(CharacterLiteralError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CharacterLiteralError {
    InvalidUnicodeEscape,
    Malformed,
    UnknownEscape(Option<char>),
    Unterminated,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StringLiteralError {
    InvalidUnicodeEscape,
    UnknownEscape(Option<char>),
    Unterminated,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EscapeMode {
    Character,
    String,
    ByteString,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EscapeScan {
    Valid(usize),
    Invalid(EscapeError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EscapeError {
    InvalidUnicode,
    Unknown(Option<char>),
    Unterminated,
}

fn scan_character_literal_body(snapshot: &SourceSnapshot, index: usize) -> CharacterBodyScan {
    match snapshot.bytes().get(index).copied() {
        Some(b'\\') => match scan_escape_sequence(snapshot, index, EscapeMode::Character) {
            EscapeScan::Valid(end_index) => CharacterBodyScan::Valid(end_index),
            EscapeScan::Invalid(EscapeError::Unterminated) => {
                CharacterBodyScan::Invalid(CharacterLiteralError::Unterminated)
            }
            EscapeScan::Invalid(EscapeError::Unknown(character)) => {
                CharacterBodyScan::Invalid(CharacterLiteralError::UnknownEscape(character))
            }
            EscapeScan::Invalid(EscapeError::InvalidUnicode) => {
                CharacterBodyScan::Invalid(CharacterLiteralError::InvalidUnicodeEscape)
            }
        },
        Some(b'\'') => CharacterBodyScan::Invalid(CharacterLiteralError::Malformed),
        Some(b'\n' | b'\r') | None => {
            CharacterBodyScan::Invalid(CharacterLiteralError::Unterminated)
        }
        Some(byte) if byte.is_ascii() => CharacterBodyScan::Valid(index + 1),
        Some(_) => match character_len_at(snapshot, index) {
            Some(character_len) => CharacterBodyScan::Valid(index + character_len),
            None => CharacterBodyScan::Invalid(CharacterLiteralError::Unterminated),
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
        Some(b'x') if mode == EscapeMode::ByteString => {
            if bytes
                .get(backslash_index + 2..backslash_index + 4)
                .is_some_and(|digits| digits.iter().all(|digit| hex_digit_value(*digit).is_some()))
            {
                EscapeScan::Valid(backslash_index + 4)
            } else {
                EscapeScan::Invalid(EscapeError::Unknown(Some('x')))
            }
        }
        Some(b'u') => scan_unicode_escape(bytes, backslash_index),
        Some(b'\n' | b'\r') | None => EscapeScan::Invalid(EscapeError::Unterminated),
        Some(_) => EscapeScan::Invalid(EscapeError::Unknown(first_character(
            snapshot,
            text_size_from_usize(backslash_index + 1),
        ))),
    }
}

fn scan_unicode_escape(bytes: &[u8], backslash_index: usize) -> EscapeScan {
    let mut index = backslash_index + 2;

    if bytes.get(index).copied() != Some(b'{') {
        return EscapeScan::Invalid(EscapeError::InvalidUnicode);
    }

    index += 1;

    let digits_start = index;

    let mut value = 0_u32;

    while let Some(byte) = bytes.get(index).copied() {
        if byte == b'}' {
            break;
        }

        if index - digits_start == 6 {
            return EscapeScan::Invalid(EscapeError::InvalidUnicode);
        }

        let digit = match hex_digit_value(byte) {
            Some(digit) => digit,
            None => return EscapeScan::Invalid(EscapeError::InvalidUnicode),
        };

        value = match value
            .checked_mul(16)
            .and_then(|value| value.checked_add(digit))
        {
            Some(value) => value,
            None => return EscapeScan::Invalid(EscapeError::InvalidUnicode),
        };

        index += 1;
    }

    if index == digits_start || bytes.get(index).copied() != Some(b'}') {
        return EscapeScan::Invalid(EscapeError::InvalidUnicode);
    }

    if char::from_u32(value).is_none() {
        return EscapeScan::Invalid(EscapeError::InvalidUnicode);
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

fn make_invalid_character_literal_token(
    snapshot: &SourceSnapshot,
    start: TextSize,
    error: CharacterLiteralError,
) -> TokenScan {
    let end = quoted_literal_recovery_end(snapshot, start, b'\'');
    let range = TextRange::new(start, end);
    let token = make_token(SyntaxKind::InvalidToken, start, end);

    let diagnostic = match error {
        CharacterLiteralError::InvalidUnicodeEscape => {
            diagnostic::invalid_unicode_escape(snapshot, range)
        }
        CharacterLiteralError::Malformed => {
            diagnostic::malformed_character_literal(snapshot, range)
        }
        CharacterLiteralError::UnknownEscape(character) => {
            diagnostic::unknown_escape(snapshot, range, character)
        }
        CharacterLiteralError::Unterminated => {
            diagnostic::unterminated_character_literal(snapshot, range)
        }
    };

    TokenScan::with_diagnostic(token, diagnostic)
}

fn make_invalid_string_literal_token(
    snapshot: &SourceSnapshot,
    start: TextSize,
    error: StringLiteralError,
) -> TokenScan {
    let end = quoted_literal_recovery_end(snapshot, start, b'"');
    let range = TextRange::new(start, end);
    let token = make_token(SyntaxKind::InvalidToken, start, end);

    let diagnostic = match error {
        StringLiteralError::InvalidUnicodeEscape => {
            diagnostic::invalid_unicode_escape(snapshot, range)
        }
        StringLiteralError::UnknownEscape(character) => {
            diagnostic::unknown_escape(snapshot, range, character)
        }
        StringLiteralError::Unterminated => {
            diagnostic::unterminated_string_literal(snapshot, range)
        }
    };

    TokenScan::with_diagnostic(token, diagnostic)
}

fn quoted_literal_recovery_end(snapshot: &SourceSnapshot, start: TextSize, quote: u8) -> TextSize {
    let bytes = snapshot.bytes();

    let mut index = text_size_to_usize(start) + 1;

    if quote == b'"' && bytes.get(text_size_to_usize(start)) == Some(&b'b') {
        index += 1;
    }

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
