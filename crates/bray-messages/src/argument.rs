use bray_diagnostics::{DiagnosticArg, DiagnosticArgName};
use bray_source::{SourceLocation, SourceSpan};

use crate::catalog::{
    format_source_location as format_english_source_location,
    format_source_span as format_english_source_span, format_value as format_english_value,
};
use crate::locale::DiagnosticLocale;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ArgumentFormatter {
    locale: DiagnosticLocale,
}

impl ArgumentFormatter {
    pub(crate) const fn new(locale: DiagnosticLocale) -> Self {
        Self { locale }
    }

    pub(crate) fn format_named_arg(
        self,
        args: &[DiagnosticArg],
        name: DiagnosticArgName,
    ) -> String {
        match args.iter().find(|arg| arg.name() == name) {
            Some(arg) => match self.locale {
                DiagnosticLocale::English => format_english_value(name, arg.value()),
            },
            None => format_missing_arg(name),
        }
    }
}

pub(crate) fn format_source_span(locale: DiagnosticLocale, span: SourceSpan) -> String {
    match locale {
        DiagnosticLocale::English => format_english_source_span(span),
    }
}

pub(crate) fn format_source_location(
    locale: DiagnosticLocale,
    location: SourceLocation<'_>,
) -> String {
    match locale {
        DiagnosticLocale::English => format_english_source_location(location),
    }
}

fn format_missing_arg(name: DiagnosticArgName) -> String {
    format!("{{{}}}", name.as_str())
}
