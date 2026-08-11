use std::io::{self, Write};

use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticLabel, DiagnosticNote,
    DiagnosticRelatedLocation, DiagnosticSourceEdit, DiagnosticSuggestion,
};
use bray_source::SourceStore;
use serde::Serialize;

use super::{DiagnosticArgValueJson, SourceSpanJson};
use crate::output::diagnostic::source_map::DiagnosticSourceMap;

pub(crate) fn write_json_diagnostics(
    diagnostics: &DiagnosticBag,
    sources: Option<&SourceStore>,
    writer: &mut impl Write,
) -> io::Result<()> {
    write_json_diagnostic_groups([(diagnostics, sources)], writer)
}

pub(in crate::output::diagnostic) fn write_json_diagnostic_groups<'diagnostic>(
    groups: impl IntoIterator<Item = (&'diagnostic DiagnosticBag, Option<&'diagnostic SourceStore>)>,
    writer: &mut impl Write,
) -> io::Result<()> {
    let mut has_errors = false;
    let mut diagnostics = Vec::new();

    for (bag, sources) in groups {
        has_errors |= bag.has_errors();

        diagnostics.extend(diagnostic_jsons(bag, sources));
    }

    let report = DiagnosticJsonReport {
        has_errors,
        diagnostics,
    };

    serde_json::to_writer_pretty(&mut *writer, &report).map_err(io::Error::other)?;

    writeln!(writer)
}

pub(crate) fn diagnostic_jsons(
    diagnostics: &DiagnosticBag,
    sources: Option<&SourceStore>,
) -> Vec<DiagnosticJson> {
    let source_map = DiagnosticSourceMap::new(sources);

    diagnostic_jsons_from_map(diagnostics, &source_map)
}

fn diagnostic_jsons_from_map(
    diagnostics: &DiagnosticBag,
    source_map: &DiagnosticSourceMap<'_>,
) -> Vec<DiagnosticJson> {
    diagnostics
        .iter()
        .map(|diagnostic| DiagnosticJson::from_diagnostic(diagnostic, source_map))
        .collect()
}

#[derive(Serialize)]
struct DiagnosticJsonReport {
    has_errors: bool,
    diagnostics: Vec<DiagnosticJson>,
}

#[derive(Serialize)]
pub(crate) struct DiagnosticJson {
    id: u32,
    code: u32,
    kind: &'static str,
    severity: &'static str,
    primary_span: Option<SourceSpanJson>,
    labels: Vec<DiagnosticLabelJson>,
    notes: Vec<DiagnosticNoteJson>,
    related_locations: Vec<DiagnosticRelatedLocationJson>,
    suggestions: Vec<DiagnosticSuggestionJson>,
    args: Vec<DiagnosticArgJson>,
}

impl DiagnosticJson {
    fn from_diagnostic(diagnostic: &Diagnostic, source_map: &DiagnosticSourceMap<'_>) -> Self {
        Self {
            id: diagnostic.id().raw(),
            code: diagnostic.kind().code().raw(),
            kind: diagnostic.kind().as_str(),
            severity: diagnostic.severity().as_str(),
            primary_span: diagnostic
                .primary_span()
                .map(|span| SourceSpanJson::from_span(span, source_map)),
            labels: diagnostic
                .labels()
                .iter()
                .map(|label| DiagnosticLabelJson::from_label(label, source_map))
                .collect(),
            notes: diagnostic
                .notes()
                .iter()
                .map(|note| DiagnosticNoteJson::from_note(note, source_map))
                .collect(),
            related_locations: diagnostic
                .related_locations()
                .iter()
                .map(|location| DiagnosticRelatedLocationJson::from_location(location, source_map))
                .collect(),
            suggestions: diagnostic
                .suggestions()
                .iter()
                .map(|suggestion| DiagnosticSuggestionJson::from_suggestion(suggestion, source_map))
                .collect(),
            args: diagnostic
                .args()
                .iter()
                .map(|arg| DiagnosticArgJson::from_arg(arg, source_map))
                .collect(),
        }
    }

    #[cfg(feature = "analysis")]
    pub(crate) const fn code(&self) -> u32 {
        self.code
    }

    #[cfg(feature = "analysis")]
    pub(crate) const fn kind(&self) -> &'static str {
        self.kind
    }

    #[cfg(feature = "analysis")]
    pub(crate) const fn severity(&self) -> &'static str {
        self.severity
    }

    #[cfg(feature = "analysis")]
    pub(crate) const fn primary_span(&self) -> Option<&SourceSpanJson> {
        self.primary_span.as_ref()
    }
}

#[derive(Serialize)]
struct DiagnosticRelatedLocationJson {
    kind: &'static str,
    span: SourceSpanJson,
    args: Vec<DiagnosticArgJson>,
}

impl DiagnosticRelatedLocationJson {
    fn from_location(
        location: &DiagnosticRelatedLocation,
        source_map: &DiagnosticSourceMap<'_>,
    ) -> Self {
        Self {
            kind: location.kind().as_str(),
            span: SourceSpanJson::from_span(location.span(), source_map),
            args: location
                .args()
                .iter()
                .map(|arg| DiagnosticArgJson::from_arg(arg, source_map))
                .collect(),
        }
    }
}

#[derive(Serialize)]
struct DiagnosticSuggestionJson {
    kind: &'static str,
    applicability: &'static str,
    edits: Vec<DiagnosticSourceEditJson>,
    args: Vec<DiagnosticArgJson>,
}

impl DiagnosticSuggestionJson {
    fn from_suggestion(
        suggestion: &DiagnosticSuggestion,
        source_map: &DiagnosticSourceMap<'_>,
    ) -> Self {
        Self {
            kind: suggestion.kind().as_str(),
            applicability: suggestion.applicability().as_str(),
            edits: suggestion
                .edits()
                .iter()
                .map(|edit| DiagnosticSourceEditJson::from_edit(edit, source_map))
                .collect(),
            args: suggestion
                .args()
                .iter()
                .map(|arg| DiagnosticArgJson::from_arg(arg, source_map))
                .collect(),
        }
    }
}

#[derive(Serialize)]
struct DiagnosticSourceEditJson {
    span: SourceSpanJson,
    replacement: String,
}

impl DiagnosticSourceEditJson {
    fn from_edit(edit: &DiagnosticSourceEdit, source_map: &DiagnosticSourceMap<'_>) -> Self {
        Self {
            span: SourceSpanJson::from_span(edit.span(), source_map),
            replacement: edit.replacement().to_owned(),
        }
    }
}

#[derive(Serialize)]
struct DiagnosticLabelJson {
    kind: &'static str,
    style: &'static str,
    span: SourceSpanJson,
    args: Vec<DiagnosticArgJson>,
}

impl DiagnosticLabelJson {
    fn from_label(label: &DiagnosticLabel, source_map: &DiagnosticSourceMap<'_>) -> Self {
        Self {
            kind: label.kind().as_str(),
            style: label.style().as_str(),
            span: SourceSpanJson::from_span(label.span(), source_map),
            args: label
                .args()
                .iter()
                .map(|arg| DiagnosticArgJson::from_arg(arg, source_map))
                .collect(),
        }
    }
}

#[derive(Serialize)]
struct DiagnosticNoteJson {
    kind: &'static str,
    args: Vec<DiagnosticArgJson>,
}

impl DiagnosticNoteJson {
    fn from_note(note: &DiagnosticNote, source_map: &DiagnosticSourceMap<'_>) -> Self {
        Self {
            kind: note.kind().as_str(),
            args: note
                .args()
                .iter()
                .map(|arg| DiagnosticArgJson::from_arg(arg, source_map))
                .collect(),
        }
    }
}

#[derive(Serialize)]
struct DiagnosticArgJson {
    name: &'static str,
    value: DiagnosticArgValueJson,
}

impl DiagnosticArgJson {
    fn from_arg(arg: &DiagnosticArg, source_map: &DiagnosticSourceMap<'_>) -> Self {
        Self {
            name: arg.name().as_str(),
            value: DiagnosticArgValueJson::from_value(arg.value(), source_map),
        }
    }
}
