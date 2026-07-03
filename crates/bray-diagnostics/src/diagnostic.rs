use bray_source::SourceSpan;

use crate::argument::DiagnosticArg;
use crate::id::DiagnosticId;
use crate::kind::DiagnosticKind;
use crate::label::DiagnosticLabel;
use crate::note::DiagnosticNote;
use crate::severity::SeverityKind;

/// Locale-neutral diagnostic record published by a compiler phase.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Diagnostic {
    id: DiagnosticId,
    kind: DiagnosticKind,
    severity: SeverityKind,
    primary_span: Option<SourceSpan>,
    labels: Vec<DiagnosticLabel>,
    notes: Vec<DiagnosticNote>,
    args: Vec<DiagnosticArg>,
}

impl Diagnostic {
    /// Creates a diagnostic record with no span, labels, notes, or arguments.
    pub fn new(id: DiagnosticId, kind: DiagnosticKind, severity: SeverityKind) -> Self {
        Self {
            id,
            kind,
            severity,
            primary_span: None,
            labels: Vec::new(),
            notes: Vec::new(),
            args: Vec::new(),
        }
    }

    /// Adds the primary source span.
    pub const fn with_primary_span(mut self, primary_span: SourceSpan) -> Self {
        self.primary_span = Some(primary_span);

        self
    }

    /// Adds one typed argument to the diagnostic.
    pub fn with_arg(mut self, arg: DiagnosticArg) -> Self {
        self.args.push(arg);

        self
    }

    /// Adds one source label to the diagnostic.
    pub fn with_label(mut self, label: DiagnosticLabel) -> Self {
        self.labels.push(label);

        self
    }

    /// Adds one structured note to the diagnostic.
    pub fn with_note(mut self, note: DiagnosticNote) -> Self {
        self.notes.push(note);

        self
    }

    /// Returns the diagnostic record identity.
    pub const fn id(&self) -> DiagnosticId {
        self.id
    }

    /// Returns the stable diagnostic category.
    pub const fn kind(&self) -> DiagnosticKind {
        self.kind
    }

    /// Returns the diagnostic severity.
    pub const fn severity(&self) -> SeverityKind {
        self.severity
    }

    /// Returns the primary source span, when the diagnostic has one.
    pub const fn primary_span(&self) -> Option<SourceSpan> {
        self.primary_span
    }

    /// Returns the typed diagnostic arguments.
    pub fn args(&self) -> &[DiagnosticArg] {
        &self.args
    }

    /// Returns the source labels attached to the diagnostic.
    pub fn labels(&self) -> &[DiagnosticLabel] {
        &self.labels
    }

    /// Returns the structured notes attached to the diagnostic.
    pub fn notes(&self) -> &[DiagnosticNote] {
        &self.notes
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct DiagnosticDuplicateKey<'diagnostic> {
    kind: DiagnosticKind,
    severity: SeverityKind,
    primary_span: Option<SourceSpan>,
    labels: &'diagnostic [DiagnosticLabel],
    notes: &'diagnostic [DiagnosticNote],
    args: &'diagnostic [DiagnosticArg],
}

impl Diagnostic {
    pub(crate) fn duplicate_key(&self) -> DiagnosticDuplicateKey<'_> {
        DiagnosticDuplicateKey {
            kind: self.kind,
            severity: self.severity,
            primary_span: self.primary_span,
            labels: &self.labels,
            notes: &self.notes,
            args: &self.args,
        }
    }
}

#[cfg(test)]
mod tests {
    use bray_source::{SourceId, SourceSpan, TextRange, TextSize};

    use super::Diagnostic;
    use crate::{
        DiagnosticArg, DiagnosticArgName, DiagnosticArgValue, DiagnosticId, DiagnosticKind,
        DiagnosticLabel, DiagnosticLabelKind, DiagnosticNote, DiagnosticNoteKind, SeverityKind,
    };

    #[test]
    fn diagnostics_publish_structured_locale_neutral_data() {
        let span = SourceSpan::new(
            SourceId::new(1),
            TextRange::new(TextSize::new(3), TextSize::new(4)),
        );

        let arg = DiagnosticArg::new(
            DiagnosticArgName::Character,
            DiagnosticArgValue::Character('\u{0}'),
        );

        let label = DiagnosticLabel::primary(DiagnosticLabelKind::InvalidCharacter, span)
            .with_arg(arg.clone());

        let note = DiagnosticNote::new(DiagnosticNoteKind::CharacterNotAccepted);

        let diagnostic = Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::LexicalInvalidCharacter,
            SeverityKind::Error,
        )
        .with_primary_span(span)
        .with_arg(arg.clone())
        .with_label(label.clone())
        .with_note(note.clone());

        assert_eq!(diagnostic.id(), DiagnosticId::new(0));
        assert_eq!(diagnostic.kind(), DiagnosticKind::LexicalInvalidCharacter);
        assert_eq!(diagnostic.severity(), SeverityKind::Error);
        assert_eq!(diagnostic.primary_span(), Some(span));
        assert_eq!(diagnostic.args(), &[arg]);
        assert_eq!(diagnostic.labels(), &[label]);
        assert_eq!(diagnostic.notes(), &[note]);
    }
}
