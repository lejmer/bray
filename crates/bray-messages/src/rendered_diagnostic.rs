use bray_diagnostics::{
    DiagnosticCode, DiagnosticId, DiagnosticKind, DiagnosticLabelKind, DiagnosticLabelStyle,
    DiagnosticNoteKind, SeverityKind,
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
    ) -> Self {
        Self {
            id,
            kind,
            severity,
            primary_span,
            message,
            labels,
            notes,
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
