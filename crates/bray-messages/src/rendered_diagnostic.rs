use bray_diagnostics::{
    DiagnosticCode, DiagnosticId, DiagnosticKind, DiagnosticLabelKind, DiagnosticLabelStyle,
    DiagnosticNoteKind, DiagnosticRelatedLocationKind, DiagnosticSourceEdit,
    DiagnosticSuggestionApplicability, DiagnosticSuggestionKind, SeverityKind,
};
use bray_source::SourceSpan;

/// User-facing rendering of one structured diagnostic.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderedDiagnostic {
    id: DiagnosticId,
    kind: DiagnosticKind,
    severity: SeverityKind,
    primary_span: Option<SourceSpan>,
    message: String,
    labels: Vec<RenderedDiagnosticLabel>,
    notes: Vec<RenderedDiagnosticNote>,
    related_locations: Vec<RenderedDiagnosticRelatedLocation>,
    suggestions: Vec<RenderedDiagnosticSuggestion>,
}

impl RenderedDiagnostic {
    pub(crate) fn new(
        id: DiagnosticId,
        kind: DiagnosticKind,
        severity: SeverityKind,
        primary_span: Option<SourceSpan>,
        message: String,
        labels: Vec<RenderedDiagnosticLabel>,
        notes: Vec<RenderedDiagnosticNote>,
        related_locations: Vec<RenderedDiagnosticRelatedLocation>,
        suggestions: Vec<RenderedDiagnosticSuggestion>,
    ) -> Self {
        Self {
            id,
            kind,
            severity,
            primary_span,
            message,
            labels,
            notes,
            related_locations,
            suggestions,
        }
    }

    /// Returns the diagnostic record identity.
    pub const fn id(&self) -> DiagnosticId {
        self.id
    }

    /// Returns the stable diagnostic category.
    pub const fn kind(&self) -> DiagnosticKind {
        self.kind
    }

    /// Returns the stable diagnostic category code.
    pub const fn code(&self) -> DiagnosticCode {
        self.kind.code()
    }

    /// Returns the diagnostic severity.
    pub const fn severity(&self) -> SeverityKind {
        self.severity
    }

    /// Returns the primary source span, when one exists.
    pub const fn primary_span(&self) -> Option<SourceSpan> {
        self.primary_span
    }

    /// Returns the rendered diagnostic message.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns the rendered source labels.
    pub fn labels(&self) -> &[RenderedDiagnosticLabel] {
        &self.labels
    }

    /// Returns the rendered notes.
    pub fn notes(&self) -> &[RenderedDiagnosticNote] {
        &self.notes
    }

    /// Returns rendered supporting source locations.
    pub fn related_locations(&self) -> &[RenderedDiagnosticRelatedLocation] {
        &self.related_locations
    }

    /// Returns rendered actionable corrections.
    pub fn suggestions(&self) -> &[RenderedDiagnosticSuggestion] {
        &self.suggestions
    }

    /// Returns whether every user-facing component was rendered completely.
    pub fn is_complete(&self) -> bool {
        message_is_complete(&self.message)
            && self
                .labels
                .iter()
                .all(|label| message_is_complete(label.message()))
            && self
                .notes
                .iter()
                .all(|note| message_is_complete(note.message()))
            && self
                .related_locations
                .iter()
                .all(|location| message_is_complete(location.message()))
            && self
                .suggestions
                .iter()
                .all(|suggestion| message_is_complete(suggestion.message()))
    }
}

fn message_is_complete(message: &str) -> bool {
    !message.is_empty() && !message.contains("[missing ")
}

/// User-facing rendering of one supporting source location.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderedDiagnosticRelatedLocation {
    kind: DiagnosticRelatedLocationKind,
    span: SourceSpan,
    message: String,
}

impl RenderedDiagnosticRelatedLocation {
    pub(crate) fn new(
        kind: DiagnosticRelatedLocationKind,
        span: SourceSpan,
        message: String,
    ) -> Self {
        Self {
            kind,
            span,
            message,
        }
    }

    /// Returns the stable relationship category.
    pub const fn kind(&self) -> DiagnosticRelatedLocationKind {
        self.kind
    }

    /// Returns the supporting source span.
    pub const fn span(&self) -> SourceSpan {
        self.span
    }

    /// Returns the rendered relationship message.
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// User-facing rendering of one actionable correction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderedDiagnosticSuggestion {
    kind: DiagnosticSuggestionKind,
    applicability: DiagnosticSuggestionApplicability,
    edits: Vec<DiagnosticSourceEdit>,
    message: String,
}

impl RenderedDiagnosticSuggestion {
    pub(crate) fn new(
        kind: DiagnosticSuggestionKind,
        applicability: DiagnosticSuggestionApplicability,
        edits: Vec<DiagnosticSourceEdit>,
        message: String,
    ) -> Self {
        Self {
            kind,
            applicability,
            edits,
            message,
        }
    }

    /// Returns the stable suggestion category.
    pub const fn kind(&self) -> DiagnosticSuggestionKind {
        self.kind
    }

    /// Returns how safely tooling can apply this suggestion.
    pub const fn applicability(&self) -> DiagnosticSuggestionApplicability {
        self.applicability
    }

    /// Returns the ordered source edits.
    pub fn edits(&self) -> &[DiagnosticSourceEdit] {
        &self.edits
    }

    /// Returns the rendered correction message.
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// User-facing rendering of one diagnostic source label.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderedDiagnosticLabel {
    kind: DiagnosticLabelKind,
    style: DiagnosticLabelStyle,
    span: SourceSpan,
    message: String,
}

impl RenderedDiagnosticLabel {
    pub(crate) fn new(
        kind: DiagnosticLabelKind,
        style: DiagnosticLabelStyle,
        span: SourceSpan,
        message: String,
    ) -> Self {
        Self {
            kind,
            style,
            span,
            message,
        }
    }

    /// Returns the stable label category.
    pub const fn kind(&self) -> DiagnosticLabelKind {
        self.kind
    }

    /// Returns whether the label is primary or secondary.
    pub const fn style(&self) -> DiagnosticLabelStyle {
        self.style
    }

    /// Returns the labeled source span.
    pub const fn span(&self) -> SourceSpan {
        self.span
    }

    /// Returns the rendered label message.
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// User-facing rendering of one structured diagnostic note.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderedDiagnosticNote {
    kind: DiagnosticNoteKind,
    rendered_kind: RenderedDiagnosticNoteKind,
    message: String,
}

impl RenderedDiagnosticNote {
    pub(crate) fn new(
        kind: DiagnosticNoteKind,
        rendered_kind: RenderedDiagnosticNoteKind,
        message: String,
    ) -> Self {
        Self {
            kind,
            rendered_kind,
            message,
        }
    }

    /// Returns the stable note category.
    pub const fn kind(&self) -> DiagnosticNoteKind {
        self.kind
    }

    /// Returns the rendered note heading category.
    pub const fn rendered_kind(&self) -> RenderedDiagnosticNoteKind {
        self.rendered_kind
    }

    /// Returns the rendered note message.
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// Rendered category for a diagnostic note heading.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RenderedDiagnosticNoteKind {
    /// Informational context.
    Note,
    /// Actionable guidance.
    Help,
}
