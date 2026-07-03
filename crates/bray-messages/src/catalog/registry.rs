use bray_diagnostics::{DiagnosticKind, DiagnosticLabelKind, DiagnosticNoteKind};

use crate::locale::DiagnosticLocale;

use super::english::{english_diagnostic_template, english_label_template, english_note_template};
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

    pub(crate) const fn label_template(self, kind: DiagnosticLabelKind) -> MessageTemplate {
        match self.locale {
            DiagnosticLocale::English => english_label_template(kind),
        }
    }

    pub(crate) const fn note_template(self, kind: DiagnosticNoteKind) -> MessageTemplate {
        match self.locale {
            DiagnosticLocale::English => english_note_template(kind),
        }
    }
}
