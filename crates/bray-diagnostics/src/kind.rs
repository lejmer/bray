/// Stable category for a compiler diagnostic.
///
/// The kind is locale-neutral and survives wording changes in rendered
/// diagnostics. Variants are grouped by owning phase.
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
    /// A block comment reaches the end of input before its terminator.
    LexicalUnterminatedBlockComment,
}

impl DiagnosticKind {
    /// Returns the stable machine key for this diagnostic category.
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
}
