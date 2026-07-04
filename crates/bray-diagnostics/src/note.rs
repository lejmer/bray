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
    /// Source file readability requirement.
    SourceFileMustBeReadable,
    /// Source UTF-8 requirement.
    SourceMustBeUtf8,
    /// Source ID compactness limit.
    SourceIdsAreCompact,
    /// Source text offset compactness limit.
    SourceTextOffsetsAreCompact,
    /// Required source input.
    SourceInputRequired,
    /// Stable source identity requirement.
    SourceInputNeedsStableIdentity,
    /// Positive worker budget requirement.
    WorkerBudgetMustBePositive,
    /// Character rejected by the lexer.
    CharacterNotAccepted,
    /// Byte order mark placement rule.
    BomOnlyAllowedAtStart,
    /// Accepted line break spellings.
    LineBreaksMustBeLfOrCrlf,
    /// Identifier character set rule.
    IdentifiersMustBeAscii,
    /// Identifier spelling rule.
    IdentifierSpellingMustBeValid,
    /// Numeric suffix rule.
    OnlyImaginaryNumericSuffix,
    /// Character literal scalar count rule.
    CharacterLiteralMustContainOneScalar,
    /// Character literal terminator rule.
    CharacterLiteralNeedsTerminator,
    /// String literal terminator rule.
    StringLiteralNeedsTerminator,
    /// Escape sequence spelling rule.
    EscapeMustBeKnown,
    /// Unicode escape scalar value rule.
    UnicodeEscapeMustBeScalar,
    /// Block comment terminator rule.
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
