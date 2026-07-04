use crate::code::DiagnosticCode;

/// Locale-neutral category for a compiler diagnostic.
///
/// Variants are grouped by owning phase.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticKind {
    /// A requested source file could not be read.
    SourceFileReadFailed,
    /// Source input contains bytes that are not valid UTF-8.
    SourceInvalidUtf8,
    /// Source input count cannot fit in compact source IDs.
    SourceTooManyInputs,
    /// Source text is too large for compact byte offsets.
    SourceTextTooLarge,
    /// The compilation request does not contain any source inputs.
    RequestMissingSourceInput,
    /// A compilation request source input is not valid.
    RequestInvalidSourceInput,
    /// The requested worker budget is not valid.
    RequestInvalidWorkerBudget,
    /// Source input contains a character that the lexer cannot accept.
    LexicalInvalidCharacter,
    /// Source input contains a byte order mark after the start of the source.
    LexicalMisplacedBom,
    /// Source input contains a lone carriage return outside a block comment.
    LexicalLoneCarriageReturn,
    /// Source input contains non-ASCII identifier text.
    LexicalNonAsciiIdentifier,
    /// Source input contains an identifier spelling that is not valid.
    LexicalInvalidIdentifier,
    /// Source input contains an operator or punctuation spelling that is not valid.
    LexicalInvalidOperatorOrPunctuation,
    /// Source input contains a malformed numeric literal spelling.
    LexicalMalformedNumericLiteral,
    /// Source input contains a numeric suffix other than imaginary `i`.
    LexicalInvalidNumericSuffix,
    /// Source input contains a malformed character literal spelling.
    LexicalMalformedCharacterLiteral,
    /// A character literal reaches the end of input or a line break before its terminator.
    LexicalUnterminatedCharacterLiteral,
    /// A string literal reaches the end of input or a line break before its terminator.
    LexicalUnterminatedStringLiteral,
    /// Source input contains an escape sequence that is not accepted.
    LexicalUnknownEscape,
    /// Source input contains a Unicode escape that is not valid.
    LexicalInvalidUnicodeEscape,
    /// A block comment reaches the end of input before its terminator.
    LexicalUnterminatedBlockComment,
}

impl DiagnosticKind {
    /// Returns the numeric code for this diagnostic category.
    pub const fn code(self) -> DiagnosticCode {
        let raw = match self {
            Self::SourceFileReadFailed => 1001,
            Self::SourceInvalidUtf8 => 1002,
            Self::SourceTooManyInputs => 1003,
            Self::SourceTextTooLarge => 1004,
            Self::RequestMissingSourceInput => 1101,
            Self::RequestInvalidSourceInput => 1102,
            Self::RequestInvalidWorkerBudget => 1103,
            Self::LexicalInvalidCharacter => 2001,
            Self::LexicalMisplacedBom => 2002,
            Self::LexicalLoneCarriageReturn => 2003,
            Self::LexicalNonAsciiIdentifier => 2004,
            Self::LexicalInvalidIdentifier => 2005,
            Self::LexicalInvalidOperatorOrPunctuation => 2006,
            Self::LexicalMalformedNumericLiteral => 2007,
            Self::LexicalInvalidNumericSuffix => 2008,
            Self::LexicalMalformedCharacterLiteral => 2009,
            Self::LexicalUnterminatedCharacterLiteral => 2010,
            Self::LexicalUnterminatedStringLiteral => 2011,
            Self::LexicalUnknownEscape => 2012,
            Self::LexicalInvalidUnicodeEscape => 2013,
            Self::LexicalUnterminatedBlockComment => 2014,
        };

        DiagnosticCode::new(raw)
    }

    /// Returns the machine key for this diagnostic category.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SourceFileReadFailed => "source_file_read_failed",
            Self::SourceInvalidUtf8 => "source_invalid_utf8",
            Self::SourceTooManyInputs => "source_too_many_inputs",
            Self::SourceTextTooLarge => "source_text_too_large",
            Self::RequestMissingSourceInput => "request_missing_source_input",
            Self::RequestInvalidSourceInput => "request_invalid_source_input",
            Self::RequestInvalidWorkerBudget => "request_invalid_worker_budget",
            Self::LexicalInvalidCharacter => "lexical_invalid_character",
            Self::LexicalMisplacedBom => "lexical_misplaced_bom",
            Self::LexicalLoneCarriageReturn => "lexical_lone_carriage_return",
            Self::LexicalNonAsciiIdentifier => "lexical_non_ascii_identifier",
            Self::LexicalInvalidIdentifier => "lexical_invalid_identifier",
            Self::LexicalInvalidOperatorOrPunctuation => "lexical_invalid_operator_or_punctuation",
            Self::LexicalMalformedNumericLiteral => "lexical_malformed_numeric_literal",
            Self::LexicalInvalidNumericSuffix => "lexical_invalid_numeric_suffix",
            Self::LexicalMalformedCharacterLiteral => "lexical_malformed_character_literal",
            Self::LexicalUnterminatedCharacterLiteral => "lexical_unterminated_character_literal",
            Self::LexicalUnterminatedStringLiteral => "lexical_unterminated_string_literal",
            Self::LexicalUnknownEscape => "lexical_unknown_escape",
            Self::LexicalInvalidUnicodeEscape => "lexical_invalid_unicode_escape",
            Self::LexicalUnterminatedBlockComment => "lexical_unterminated_block_comment",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::DiagnosticKind;

    #[test]
    fn diagnostic_kinds_expose_stable_machine_keys() {
        assert_eq!(
            DiagnosticKind::LexicalUnterminatedBlockComment.as_str(),
            "lexical_unterminated_block_comment"
        );
    }

    #[test]
    fn diagnostic_kinds_expose_stable_numeric_codes() {
        assert_eq!(DiagnosticKind::SourceInvalidUtf8.code().raw(), 1002);
        assert_eq!(
            DiagnosticKind::LexicalUnterminatedBlockComment.code().raw(),
            2014
        );
    }

    #[test]
    fn diagnostic_kind_codes_are_unique() {
        let mut codes = Vec::new();

        for kind in all_diagnostic_kinds() {
            assert!(
                !codes.contains(&kind.code()),
                "duplicate diagnostic code for {kind:?}"
            );

            codes.push(kind.code());
        }
    }

    fn all_diagnostic_kinds() -> [DiagnosticKind; 21] {
        [
            DiagnosticKind::SourceFileReadFailed,
            DiagnosticKind::SourceInvalidUtf8,
            DiagnosticKind::SourceTooManyInputs,
            DiagnosticKind::SourceTextTooLarge,
            DiagnosticKind::RequestMissingSourceInput,
            DiagnosticKind::RequestInvalidSourceInput,
            DiagnosticKind::RequestInvalidWorkerBudget,
            DiagnosticKind::LexicalInvalidCharacter,
            DiagnosticKind::LexicalMisplacedBom,
            DiagnosticKind::LexicalLoneCarriageReturn,
            DiagnosticKind::LexicalNonAsciiIdentifier,
            DiagnosticKind::LexicalInvalidIdentifier,
            DiagnosticKind::LexicalInvalidOperatorOrPunctuation,
            DiagnosticKind::LexicalMalformedNumericLiteral,
            DiagnosticKind::LexicalInvalidNumericSuffix,
            DiagnosticKind::LexicalMalformedCharacterLiteral,
            DiagnosticKind::LexicalUnterminatedCharacterLiteral,
            DiagnosticKind::LexicalUnterminatedStringLiteral,
            DiagnosticKind::LexicalUnknownEscape,
            DiagnosticKind::LexicalInvalidUnicodeEscape,
            DiagnosticKind::LexicalUnterminatedBlockComment,
        ]
    }
}
