use bray_source::SourceSpan;

use crate::argument::DiagnosticArg;
use crate::id::DiagnosticId;
use crate::kind::DiagnosticKind;
use crate::label::DiagnosticLabel;
use crate::note::DiagnosticNote;
use crate::related::DiagnosticRelatedLocation;
use crate::severity::SeverityKind;
use crate::suggestion::DiagnosticSuggestion;

/// Locale-neutral diagnostic record produced by a compiler phase.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Diagnostic {
    id: DiagnosticId,
    kind: DiagnosticKind,
    severity: SeverityKind,
    primary_span: Option<SourceSpan>,
    labels: Vec<DiagnosticLabel>,
    notes: Vec<DiagnosticNote>,
    related_locations: Vec<DiagnosticRelatedLocation>,
    suggestions: Vec<DiagnosticSuggestion>,
    args: Vec<DiagnosticArg>,
}

impl Diagnostic {
    /// Creates a diagnostic record with no span, labels, notes, related locations,
    /// suggestions, or arguments.
    pub fn new(id: DiagnosticId, kind: DiagnosticKind, severity: SeverityKind) -> Self {
        Self {
            id,
            kind,
            severity,
            primary_span: None,
            labels: Vec::new(),
            notes: Vec::new(),
            related_locations: Vec::new(),
            suggestions: Vec::new(),
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
        if !self.notes.contains(&note) {
            self.notes.push(note);
        }

        self
    }

    /// Adds a typed argument when the producing boundary has one.
    pub fn with_optional_arg(self, arg: Option<DiagnosticArg>) -> Self {
        match arg {
            Some(arg) => self.with_arg(arg),
            None => self,
        }
    }

    /// Adds one supporting source location.
    pub fn with_related_location(mut self, location: DiagnosticRelatedLocation) -> Self {
        if !self.related_locations.contains(&location) {
            self.related_locations.push(location);
        }

        self
    }

    /// Adds one actionable correction.
    pub fn with_suggestion(mut self, suggestion: DiagnosticSuggestion) -> Self {
        self.suggestions.push(suggestion);

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

    /// Returns supporting source locations involved in this diagnostic.
    pub fn related_locations(&self) -> &[DiagnosticRelatedLocation] {
        &self.related_locations
    }

    /// Returns actionable corrections associated with this diagnostic.
    pub fn suggestions(&self) -> &[DiagnosticSuggestion] {
        &self.suggestions
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct DiagnosticDuplicateKey<'diagnostic> {
    kind: DiagnosticKind,
    severity: SeverityKind,
    primary_span: Option<SourceSpan>,
    labels: &'diagnostic [DiagnosticLabel],
    notes: &'diagnostic [DiagnosticNote],
    related_locations: &'diagnostic [DiagnosticRelatedLocation],
    suggestions: &'diagnostic [DiagnosticSuggestion],
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
            related_locations: &self.related_locations,
            suggestions: &self.suggestions,
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
        DiagnosticLabel, DiagnosticLabelKind, DiagnosticNote, DiagnosticNoteKind,
        DiagnosticRelatedLocation, DiagnosticRelatedLocationKind, DiagnosticSourceEdit,
        DiagnosticSuggestion, DiagnosticSuggestionApplicability, DiagnosticSuggestionKind,
        SeverityKind,
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

        let related =
            DiagnosticRelatedLocation::new(DiagnosticRelatedLocationKind::RequirementOrigin, span);

        let suggestion = DiagnosticSuggestion::try_edits(
            DiagnosticSuggestionKind::ReplaceWithLineFeed,
            DiagnosticSuggestionApplicability::MachineApplicable,
            [DiagnosticSourceEdit::new(span, "\n")],
        )
        .unwrap_or_else(|error| panic!("test suggestion should be valid: {error:?}"));

        let diagnostic = Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::LexicalInvalidCharacter,
            SeverityKind::Error,
        )
        .with_primary_span(span)
        .with_arg(arg.clone())
        .with_label(label.clone())
        .with_note(note.clone())
        .with_related_location(related.clone())
        .with_related_location(related.clone())
        .with_suggestion(suggestion.clone());

        assert_eq!(diagnostic.id(), DiagnosticId::new(0));
        assert_eq!(diagnostic.kind(), DiagnosticKind::LexicalInvalidCharacter);
        assert_eq!(diagnostic.severity(), SeverityKind::Error);
        assert_eq!(diagnostic.primary_span(), Some(span));
        assert_eq!(diagnostic.args(), &[arg]);
        assert_eq!(diagnostic.labels(), &[label]);
        assert_eq!(diagnostic.notes(), &[note]);

        assert_eq!(diagnostic.related_locations(), &[related]);
        assert_eq!(diagnostic.suggestions(), &[suggestion]);
    }
}
