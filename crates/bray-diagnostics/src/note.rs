use crate::argument::DiagnosticArg;

/// Structured note attached to a diagnostic.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct DiagnosticNote {
    kind: DiagnosticNoteKind,
    args: Vec<DiagnosticArg>,
}

impl DiagnosticNote {
    /// Creates a note with no message arguments.
    pub fn new(kind: DiagnosticNoteKind) -> Self {
        Self {
            kind,
            args: Vec::new(),
        }
    }

    /// Adds one typed argument to the note.
    pub fn with_arg(mut self, arg: DiagnosticArg) -> Self {
        self.args.push(arg);

        self
    }

    /// Returns the stable note category.
    pub const fn kind(&self) -> DiagnosticNoteKind {
        self.kind
    }

    /// Returns the typed note arguments.
    pub fn args(&self) -> &[DiagnosticArg] {
        &self.args
    }
}

/// Stable category for a diagnostic note.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticNoteKind {
    /// Note explaining that a source file must be readable before compilation.
    SourceFileMustBeReadable,
    /// Note explaining that source inputs must be valid UTF-8.
    SourceMustBeUtf8,
    /// Note explaining that source IDs use a compact representation.
    SourceIdsAreCompact,
    /// Note explaining that source text offsets use a compact representation.
    SourceTextOffsetsAreCompact,
    /// Note explaining that at least one source input is required.
    SourceInputRequired,
    /// Note explaining that CLI source inputs need stable source identities.
    SourceInputNeedsStableIdentity,
    /// Note explaining that worker budgets must be positive.
    WorkerBudgetMustBePositive,
    /// Note explaining that a character is not accepted by the lexer.
    CharacterNotAccepted,
    /// Note explaining that a byte order mark is accepted only at source start.
    BomOnlyAllowedAtStart,
    /// Note explaining that accepted line breaks are LF and CRLF.
    LineBreaksMustBeLfOrCrlf,
    /// Note explaining that identifiers are ASCII.
    IdentifiersMustBeAscii,
    /// Note explaining that identifiers have a constrained spelling.
    IdentifierSpellingMustBeValid,
    /// Note explaining that only the imaginary suffix is accepted.
    OnlyImaginaryNumericSuffix,
    /// Note explaining that a character literal contains one scalar value.
    CharacterLiteralMustContainOneScalar,
    /// Note explaining that a character literal needs a closing quote.
    CharacterLiteralNeedsTerminator,
    /// Note explaining that a string literal needs a closing quote.
    StringLiteralNeedsTerminator,
    /// Note explaining that escape sequences must be known.
    EscapeMustBeKnown,
    /// Note explaining that Unicode escapes must denote scalar values.
    UnicodeEscapeMustBeScalar,
    /// Note explaining that a block comment needs a closing terminator.
    BlockCommentNeedsTerminator,
}

impl DiagnosticNoteKind {
    /// Returns the stable machine key for this note category.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SourceFileMustBeReadable => "source_file_must_be_readable",
            Self::SourceMustBeUtf8 => "source_must_be_utf8",
            Self::SourceIdsAreCompact => "source_ids_are_compact",
            Self::SourceTextOffsetsAreCompact => "source_text_offsets_are_compact",
            Self::SourceInputRequired => "source_input_required",
            Self::SourceInputNeedsStableIdentity => "source_input_needs_stable_identity",
            Self::WorkerBudgetMustBePositive => "worker_budget_must_be_positive",
            Self::CharacterNotAccepted => "character_not_accepted",
            Self::BomOnlyAllowedAtStart => "bom_only_allowed_at_start",
            Self::LineBreaksMustBeLfOrCrlf => "line_breaks_must_be_lf_or_crlf",
            Self::IdentifiersMustBeAscii => "identifiers_must_be_ascii",
            Self::IdentifierSpellingMustBeValid => "identifier_spelling_must_be_valid",
            Self::OnlyImaginaryNumericSuffix => "only_imaginary_numeric_suffix",
            Self::CharacterLiteralMustContainOneScalar => {
                "character_literal_must_contain_one_scalar"
            }
            Self::CharacterLiteralNeedsTerminator => "character_literal_needs_terminator",
            Self::StringLiteralNeedsTerminator => "string_literal_needs_terminator",
            Self::EscapeMustBeKnown => "escape_must_be_known",
            Self::UnicodeEscapeMustBeScalar => "unicode_escape_must_be_scalar",
            Self::BlockCommentNeedsTerminator => "block_comment_needs_terminator",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DiagnosticNote, DiagnosticNoteKind};
    use crate::{DiagnosticArg, DiagnosticArgName, DiagnosticArgValue};

    #[test]
    fn notes_carry_kind_and_typed_args() {
        let note =
            DiagnosticNote::new(DiagnosticNoteKind::SourceMustBeUtf8).with_arg(DiagnosticArg::new(
                DiagnosticArgName::ByteCount,
                DiagnosticArgValue::ByteCount(2),
            ));

        assert_eq!(note.kind(), DiagnosticNoteKind::SourceMustBeUtf8);
        assert_eq!(note.kind().as_str(), "source_must_be_utf8");

        assert_eq!(
            note.args(),
            &[DiagnosticArg::new(
                DiagnosticArgName::ByteCount,
                DiagnosticArgValue::ByteCount(2)
            )]
        );
    }
}
