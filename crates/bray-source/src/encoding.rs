use std::str::Utf8Error;

const UTF8_BOM_BYTES: &[u8] = b"\xEF\xBB\xBF";
const UTF8_BOM_CHARACTER: char = '\u{feff}';

/// Error returned when source bytes are not valid UTF-8.
///
/// When produced by [`SourceLoader`](crate::SourceLoader), byte offsets are
/// measured after an accepted leading UTF-8 byte order mark has been removed.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SourceUtf8Error {
    valid_up_to: usize,
    invalid_sequence_len: Option<usize>,
}

impl SourceUtf8Error {
    /// Creates a UTF-8 validation error from byte-position details.
    pub const fn new(valid_up_to: usize, invalid_sequence_len: Option<usize>) -> Self {
        Self {
            valid_up_to,
            invalid_sequence_len,
        }
    }

    /// Returns the byte offset before the invalid byte sequence.
    pub const fn valid_up_to(self) -> usize {
        self.valid_up_to
    }

    /// Returns the byte length of the invalid sequence.
    ///
    /// `None` means the input ended in an incomplete UTF-8 sequence.
    pub const fn invalid_sequence_len(self) -> Option<usize> {
        self.invalid_sequence_len
    }
}

impl From<Utf8Error> for SourceUtf8Error {
    fn from(error: Utf8Error) -> Self {
        Self::new(error.valid_up_to(), error.error_len())
    }
}

pub(crate) fn decode_source_bytes(mut bytes: Vec<u8>) -> Result<String, SourceUtf8Error> {
    if bytes.starts_with(UTF8_BOM_BYTES) {
        bytes.drain(..UTF8_BOM_BYTES.len());
    }

    match String::from_utf8(bytes) {
        Ok(text) => Ok(text),
        Err(error) => Err(SourceUtf8Error::from(error.utf8_error())),
    }
}

pub(crate) fn normalize_source_text(mut text: String) -> String {
    if text.starts_with(UTF8_BOM_CHARACTER) {
        text.drain(..UTF8_BOM_CHARACTER.len_utf8());
    }

    text
}

#[cfg(test)]
mod tests {
    use super::{SourceUtf8Error, decode_source_bytes, normalize_source_text};

    #[test]
    fn source_bytes_decode_as_utf8() {
        let text = match decode_source_bytes(Vec::from("module main\n")) {
            Ok(text) => text,
            Err(error) => panic!("test source bytes should decode as UTF-8: {error:?}"),
        };

        assert_eq!(text, "module main\n");
    }

    #[test]
    fn source_bytes_strip_leading_utf8_bom() {
        let text = match decode_source_bytes(Vec::from(b"\xEF\xBB\xBFmodule main\n")) {
            Ok(text) => text,
            Err(error) => panic!("test source bytes should decode as UTF-8: {error:?}"),
        };

        assert_eq!(text, "module main\n");
    }

    #[test]
    fn source_bytes_report_invalid_utf8() {
        let error = match decode_source_bytes(vec![b'a', 0xff, b'b']) {
            Ok(text) => panic!("test source bytes should be invalid UTF-8: {text:?}"),
            Err(error) => error,
        };

        assert_eq!(error, SourceUtf8Error::new(1, Some(1)));
    }

    #[test]
    fn source_text_strips_leading_bom_character() {
        assert_eq!(normalize_source_text(String::from("\u{feff}abc")), "abc");
    }

    #[test]
    fn source_text_preserves_non_initial_bom_character() {
        assert_eq!(
            normalize_source_text(String::from("a\u{feff}b")),
            "a\u{feff}b"
        );
    }
}
