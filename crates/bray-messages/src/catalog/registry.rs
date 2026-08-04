use bray_diagnostics::{
    DiagnosticKind, DiagnosticLabelKind, DiagnosticLabelStyle, DiagnosticNoteKind, SeverityKind,
};

use crate::locale::DiagnosticLocale;
use crate::rendered_diagnostic::RenderedDiagnosticNoteKind;
use crate::{
    BuildProgressAction, BuildProgressConfiguration, BuildProgressField, BuildProgressLineKind,
    BuildProgressOperation, LanguageServerMessage, TestReportOutcome,
};

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

    pub(crate) const fn language_server_message(
        self,
        message: LanguageServerMessage,
    ) -> &'static str {
        match self.locale {
            DiagnosticLocale::English => super::english::language_server_message(message),
        }
    }

    pub(crate) fn build_progress_fields(
        self,
        kind: BuildProgressLineKind,
    ) -> &'static [BuildProgressField] {
        match self.locale {
            DiagnosticLocale::English => super::english::build_progress_fields(kind),
        }
    }

    pub(crate) fn build_progress_heading(
        self,
        product: &str,
        configuration: BuildProgressConfiguration,
    ) -> String {
        match self.locale {
            DiagnosticLocale::English => {
                super::english::build_progress_heading(product, configuration)
            }
        }
    }

    pub(crate) const fn build_progress_operation(
        self,
        operation: BuildProgressOperation,
    ) -> &'static str {
        match self.locale {
            DiagnosticLocale::English => super::english::build_progress_operation(operation),
        }
    }

    pub(crate) const fn build_progress_action(self, action: BuildProgressAction) -> &'static str {
        match self.locale {
            DiagnosticLocale::English => super::english::build_progress_action(action),
        }
    }

    pub(crate) fn build_progress_unit_count(self, completed: u64, total: u64) -> String {
        match self.locale {
            DiagnosticLocale::English => {
                super::english::build_progress_unit_count(completed, total)
            }
        }
    }

    pub(crate) fn build_progress_duration(self, milliseconds: u128) -> String {
        match self.locale {
            DiagnosticLocale::English => super::english::build_progress_duration(milliseconds),
        }
    }

    pub(crate) fn build_progress_percentage(self, percentage: u64) -> String {
        match self.locale {
            DiagnosticLocale::English => super::english::build_progress_percentage(percentage),
        }
    }

    pub(crate) fn test_report_heading(self, count: usize) -> String {
        match self.locale {
            DiagnosticLocale::English => super::english::test_report_heading(count),
        }
    }

    pub(crate) fn test_report_result(
        self,
        outcome: TestReportOutcome,
        identity: &str,
    ) -> String {
        match self.locale {
            DiagnosticLocale::English => super::english::test_report_result(outcome, identity),
        }
    }

    pub(crate) fn test_report_summary(self, passed: usize, failed: usize) -> String {
        match self.locale {
            DiagnosticLocale::English => super::english::test_report_summary(passed, failed),
        }
    }

    pub(crate) const fn test_report_captured_stream(self, standard_error: bool) -> &'static str {
        match self.locale {
            DiagnosticLocale::English => {
                super::english::test_report_captured_stream(standard_error)
            }
        }
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
