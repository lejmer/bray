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
    /// Note explaining that a block comment needs a closing terminator.
    BlockCommentNeedsTerminator,
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

        assert_eq!(
            note.args(),
            &[DiagnosticArg::new(
                DiagnosticArgName::ByteCount,
                DiagnosticArgValue::ByteCount(2)
            )]
        );
    }
}
