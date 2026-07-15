use bray_diagnostics::{
    DiagnosticKind, DiagnosticLabelKind, DiagnosticLabelStyle, DiagnosticNoteKind, SeverityKind,
};

use crate::locale::DiagnosticLocale;
use crate::rendered_diagnostic::RenderedDiagnosticNoteKind;

use super::english::{
    diagnostic_template as english_diagnostic_template, label_style as english_label_style,
    label_template as english_label_template, note_heading as english_note_heading,
    note_kind as english_note_kind, note_template as english_note_template,
    severity_label as english_severity_label,
};
use super::template::MessageTemplate;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MessageCatalog {
    locale: DiagnosticLocale,
}

impl MessageCatalog {
    pub(crate) const fn new(locale: DiagnosticLocale) -> Self {
        Self { locale }
    }

    pub(crate) const fn diagnostic_template(self, kind: DiagnosticKind) -> MessageTemplate {
        match self.locale {
            DiagnosticLocale::English => english_diagnostic_template(kind),
        }
    }

    pub(crate) const fn severity_label(self, severity: SeverityKind) -> &'static str {
        match self.locale {
            DiagnosticLocale::English => english_severity_label(severity),
        }
    }

    pub(crate) const fn label_template(self, kind: DiagnosticLabelKind) -> MessageTemplate {
        match self.locale {
            DiagnosticLocale::English => english_label_template(kind),
        }
    }

    pub(crate) const fn label_style(self, style: DiagnosticLabelStyle) -> &'static str {
        match self.locale {
            DiagnosticLocale::English => english_label_style(style),
        }
    }

    pub(crate) const fn note_template(self, kind: DiagnosticNoteKind) -> MessageTemplate {
        match self.locale {
            DiagnosticLocale::English => english_note_template(kind),
        }
    }

    pub(crate) const fn note_kind(self, kind: DiagnosticNoteKind) -> RenderedDiagnosticNoteKind {
        match self.locale {
            DiagnosticLocale::English => english_note_kind(kind),
        }
    }

    pub(crate) const fn note_heading(self, kind: RenderedDiagnosticNoteKind) -> &'static str {
        match self.locale {
            DiagnosticLocale::English => english_note_heading(kind),
        }
    }
}
