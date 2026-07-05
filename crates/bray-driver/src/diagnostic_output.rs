use std::io::{self, Write};

use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticArgValue, DiagnosticBag, DiagnosticLabel, DiagnosticNote,
};
use bray_messages::{DiagnosticRenderer, RenderedDiagnostic, RenderedDiagnosticLabel};
use bray_source::{LineIndex, SourceLocation, SourceSpan, SourceStore};
use serde::Serialize;

use crate::command::DriverOutputFormat;
use crate::output_path::path_to_output_string;
use crate::run::DriverRunResult;
use crate::source_location_output::SourceLocationOutput;
use crate::source_origin_output::SourceOriginOutput;
use crate::terminal_style::{color_note_heading, color_severity_label};

pub(crate) fn write_driver_output(
    result: &DriverRunResult,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> io::Result<()> {
    stdout.write_all(result.stdout().as_bytes())?;
    stderr.write_all(result.stderr().as_bytes())?;

    if result.has_terminal_output() {
        return Ok(());
    }

    match result.output_format() {
        DriverOutputFormat::Text => {
            write_text_diagnostics(result.diagnostics(), result.sources(), stderr)
        }
        DriverOutputFormat::Json => {
            write_json_diagnostics(result.diagnostics(), result.sources(), stdout)
        }
    }
}

fn write_text_diagnostics(
    diagnostics: &DiagnosticBag,
    sources: Option<&SourceStore>,
    writer: &mut impl Write,
) -> io::Result<()> {
    let renderer = DiagnosticRenderer::english();
    let source_map = DiagnosticSourceMap::new(sources);

    for diagnostic in renderer.render_bag(diagnostics) {
        write_text_diagnostic(renderer, &source_map, &diagnostic, writer)?;
    }

    Ok(())
}

fn write_text_diagnostic(
    renderer: DiagnosticRenderer,
    source_map: &DiagnosticSourceMap<'_>,
    diagnostic: &RenderedDiagnostic,
    writer: &mut impl Write,
) -> io::Result<()> {
    writeln!(
        writer,
        "{}[{:04}]: {}",
        color_severity_label(
            diagnostic.severity(),
            renderer.render_severity(diagnostic.severity())
        ),
        diagnostic.code().raw(),
        diagnostic.message()
    )?;

    if let Some(primary_span) = diagnostic.primary_span() {
        writeln!(
            writer,
            "  --> {}",
            source_map.render(renderer, primary_span)
        )?;
    }

    for label in diagnostic.labels() {
        write_text_label(renderer, source_map, label, writer)?;
    }

    for note in diagnostic.notes() {
        writeln!(
            writer,
            "  {}: {}",
            color_note_heading(renderer.render_note_heading()),
            note.message()
        )?;
    }

    Ok(())
}

fn write_text_label(
    renderer: DiagnosticRenderer,
    source_map: &DiagnosticSourceMap<'_>,
    label: &RenderedDiagnosticLabel,
    writer: &mut impl Write,
) -> io::Result<()> {
    writeln!(
        writer,
        "  = {} {}: {}",
        renderer.render_label_style(label.style()),
        source_map.render(renderer, label.span()),
        label.message()
    )
}

fn write_json_diagnostics(
    diagnostics: &DiagnosticBag,
    sources: Option<&SourceStore>,
    writer: &mut impl Write,
) -> io::Result<()> {
    let source_map = DiagnosticSourceMap::new(sources);
    let report = DiagnosticJsonReport::from_bag(diagnostics, &source_map);

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

struct DiagnosticSourceMap<'source> {
    sources: Option<&'source SourceStore>,
    line_indexes: Vec<Option<LineIndex>>,
}

impl<'source> DiagnosticSourceMap<'source> {
    fn new(sources: Option<&'source SourceStore>) -> Self {
        let line_indexes = match sources {
            Some(sources) => sources
                .iter()
                .map(|snapshot| LineIndex::new(snapshot.text()).ok())
                .collect(),
            None => Vec::new(),
        };

        Self {
            sources,
            line_indexes,
        }
    }

    fn resolve(&self, span: SourceSpan) -> Option<SourceLocation<'source>> {
        let sources = self.sources?;
        let index = span.source_id().to_index()?;
        let snapshot = sources.get(span.source_id())?;
        let line_index = self.line_indexes.get(index)?.as_ref()?;

        SourceLocation::resolve(snapshot, line_index, span)
    }

    fn source_origin(&self, span: SourceSpan) -> Option<SourceOriginOutput> {
        self.sources?
            .get(span.source_id())
            .map(|snapshot| SourceOriginOutput::from_origin(snapshot.origin()))
    }

    fn render(&self, renderer: DiagnosticRenderer, span: SourceSpan) -> String {
        match self.resolve(span) {
            Some(location) => renderer.render_source_location(location),
            None => renderer.render_source_span(span),
        }
    }
}

#[derive(Serialize)]
struct DiagnosticJsonReport {
    has_errors: bool,
    diagnostics: Vec<DiagnosticJson>,
}

impl DiagnosticJsonReport {
    fn from_bag(bag: &DiagnosticBag, source_map: &DiagnosticSourceMap<'_>) -> Self {
        Self {
            has_errors: bag.has_errors(),
            diagnostics: diagnostic_jsons_from_map(bag, source_map),
        }
    }
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
            args: diagnostic
                .args()
                .iter()
                .map(|arg| DiagnosticArgJson::from_arg(arg, source_map))
                .collect(),
        }
    }

    pub(crate) const fn code(&self) -> u32 {
        self.code
    }

    pub(crate) const fn kind(&self) -> &'static str {
        self.kind
    }

    pub(crate) const fn severity(&self) -> &'static str {
        self.severity
    }

    pub(crate) const fn primary_span(&self) -> Option<&SourceSpanJson> {
        self.primary_span.as_ref()
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

#[derive(Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
enum DiagnosticArgValueJson {
    Byte(u8),
    ByteCount(u64),
    Character(char),
    FilePath(String),
    InputIndex(u64),
    IoErrorKind(&'static str),
    SourceName(String),
    SourceCount(u64),
    SourceInputKind(&'static str),
    TextOffset(u32),
    Uri(String),
    SourceSpan(SourceSpanJson),
    WorkerCount(u64),
}

impl DiagnosticArgValueJson {
    fn from_value(value: &DiagnosticArgValue, source_map: &DiagnosticSourceMap<'_>) -> Self {
        match value {
            DiagnosticArgValue::Byte(byte) => Self::Byte(*byte),
            DiagnosticArgValue::ByteCount(byte_count) => Self::ByteCount(*byte_count),
            DiagnosticArgValue::Character(character) => Self::Character(*character),
            DiagnosticArgValue::FilePath(path) => Self::FilePath(path_to_output_string(path)),
            DiagnosticArgValue::InputIndex(input_index) => Self::InputIndex(*input_index),
            DiagnosticArgValue::IoErrorKind(kind) => Self::IoErrorKind((*kind).as_str()),
            DiagnosticArgValue::SourceName(name) => Self::SourceName(name.clone()),
            DiagnosticArgValue::SourceCount(source_count) => Self::SourceCount(*source_count),
            DiagnosticArgValue::SourceInputKind(kind) => Self::SourceInputKind((*kind).as_str()),
            DiagnosticArgValue::TextOffset(offset) => Self::TextOffset(offset.bytes()),
            DiagnosticArgValue::Uri(uri) => Self::Uri(uri.clone()),
            DiagnosticArgValue::SourceSpan(span) => {
                Self::SourceSpan(SourceSpanJson::from_span(*span, source_map))
            }
            DiagnosticArgValue::WorkerCount(worker_count) => Self::WorkerCount(*worker_count),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct SourceSpanJson {
    source_id: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_origin: Option<SourceOriginOutput>,
    start: u32,
    end: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    location: Option<SourceLocationOutput>,
}

impl SourceSpanJson {
    fn from_span(span: SourceSpan, source_map: &DiagnosticSourceMap<'_>) -> Self {
        Self {
            source_id: span.source_id().raw(),
            source_origin: source_map.source_origin(span),
            start: span.start().bytes(),
            end: span.end().bytes(),
            location: source_map
                .resolve(span)
                .map(SourceLocationOutput::from_location),
        }
    }

    pub(crate) const fn start(&self) -> u32 {
        self.start
    }

    pub(crate) const fn end(&self) -> u32 {
        self.end
    }

    pub(crate) const fn location(&self) -> Option<SourceLocationOutput> {
        self.location
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{
        Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticKind, DiagnosticLabel,
        DiagnosticLabelKind, DiagnosticNote, DiagnosticNoteKind, SeverityKind,
    };
    use bray_source::{
        SourceIdentity, SourceOrigin, SourceSpan, SourceStore, SourceVersion, TextRange, TextSize,
    };

    use super::{write_json_diagnostics, write_text_diagnostics};

    #[test]
    fn text_output_renders_colored_terminal_diagnostics() {
        let diagnostic = Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::SourceInvalidUtf8,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::text_offset(TextSize::new(4)))
        .with_note(DiagnosticNote::new(DiagnosticNoteKind::SourceMustBeUtf8));

        let bag = DiagnosticBag::single(diagnostic);

        let mut output = Vec::new();

        match write_text_diagnostics(&bag, None, &mut output) {
            Ok(()) => {}
            Err(error) => panic!("text diagnostics should write: {error:?}"),
        }

        let output = match String::from_utf8(output) {
            Ok(output) => output,
            Err(error) => panic!("text diagnostics should be UTF-8: {error:?}"),
        };

        assert!(output.contains(
            "\x1b[31merror\x1b[0m[1002]: source input contains invalid UTF-8 at byte offset 4"
        ));

        assert!(!output.contains("\x1b[31merror\x1b[0m[0]"));
        assert!(!output.contains("source_invalid_utf8"));
        assert!(output.contains("source input contains invalid UTF-8 at byte offset 4"));
        assert!(output.contains("\x1b[36mnote\x1b[0m: source inputs must be valid UTF-8"));
    }

    #[test]
    fn text_output_resolves_source_spans_to_line_columns() {
        let sources = source_store("ok\n$");

        let span = SourceSpan::new(
            bray_source::SourceId::new(0),
            TextRange::new(TextSize::new(3), TextSize::new(4)),
        );

        let diagnostic = Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::LexicalInvalidCharacter,
            SeverityKind::Error,
        )
        .with_primary_span(span)
        .with_label(DiagnosticLabel::primary(
            DiagnosticLabelKind::InvalidCharacter,
            span,
        ));

        let bag = DiagnosticBag::single(diagnostic);

        let mut output = Vec::new();

        match write_text_diagnostics(&bag, Some(&sources), &mut output) {
            Ok(()) => {}
            Err(error) => panic!("text diagnostics should write: {error:?}"),
        }

        let output = match String::from_utf8(output) {
            Ok(output) => output,
            Err(error) => panic!("text diagnostics should be UTF-8: {error:?}"),
        };

        assert!(output.contains("main.bray:2:1..2:1"));
        assert!(!output.contains("source 0:3..4"));
    }

    #[test]
    fn text_output_does_not_render_line_column_end_past_same_line_span() {
        let sources = source_store("abcdefghijkl");

        let span = SourceSpan::new(
            bray_source::SourceId::new(0),
            TextRange::new(TextSize::ZERO, TextSize::new(12)),
        );

        let diagnostic = Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::LexicalInvalidCharacter,
            SeverityKind::Error,
        )
        .with_primary_span(span);

        let bag = DiagnosticBag::single(diagnostic);

        let mut output = Vec::new();

        match write_text_diagnostics(&bag, Some(&sources), &mut output) {
            Ok(()) => {}
            Err(error) => panic!("text diagnostics should write: {error:?}"),
        }

        let output = match String::from_utf8(output) {
            Ok(output) => output,
            Err(error) => panic!("text diagnostics should be UTF-8: {error:?}"),
        };

        assert!(output.contains("main.bray:1:1..1:12"));
        assert!(!output.contains("main.bray:1:1..1:13"));
    }

    #[test]
    fn json_output_serializes_structured_diagnostics() {
        let diagnostic = Diagnostic::new(
            DiagnosticId::new(3),
            DiagnosticKind::SourceInvalidUtf8,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::text_offset(TextSize::new(5)))
        .with_note(DiagnosticNote::new(DiagnosticNoteKind::SourceMustBeUtf8));

        let bag = DiagnosticBag::single(diagnostic);

        let mut output = Vec::new();

        match write_json_diagnostics(&bag, None, &mut output) {
            Ok(()) => {}
            Err(error) => panic!("JSON diagnostics should write: {error:?}"),
        }

        let output = match String::from_utf8(output) {
            Ok(output) => output,
            Err(error) => panic!("JSON diagnostics should be UTF-8: {error:?}"),
        };

        let output_json: serde_json::Value = match serde_json::from_str(&output) {
            Ok(value) => value,
            Err(error) => panic!("JSON diagnostics should parse: {error:?}"),
        };

        assert_eq!(output_json["has_errors"], true);

        let diagnostic_json = &output_json["diagnostics"][0];

        assert_eq!(diagnostic_json["id"], 3);
        assert_eq!(diagnostic_json["code"], 1002);
        assert_eq!(diagnostic_json["kind"], "source_invalid_utf8");
        assert_eq!(diagnostic_json["severity"], "error");
        assert!(diagnostic_json.get("message").is_none());

        let arg_json = &diagnostic_json["args"][0];

        assert_eq!(arg_json["name"], "text_offset");
        assert_eq!(arg_json["value"]["kind"], "text_offset");
        assert_eq!(arg_json["value"]["value"], 5);
        assert!(!output.contains("source input contains invalid UTF-8 at byte offset"));
    }

    #[test]
    fn json_output_serializes_resolved_source_locations() {
        let sources = source_store("ok\n$");

        let span = SourceSpan::new(
            bray_source::SourceId::new(0),
            TextRange::new(TextSize::new(3), TextSize::new(4)),
        );

        let diagnostic = Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::LexicalInvalidCharacter,
            SeverityKind::Error,
        )
        .with_primary_span(span);

        let bag = DiagnosticBag::single(diagnostic);

        let mut output = Vec::new();

        match write_json_diagnostics(&bag, Some(&sources), &mut output) {
            Ok(()) => {}
            Err(error) => panic!("JSON diagnostics should write: {error:?}"),
        }

        let output = match String::from_utf8(output) {
            Ok(output) => output,
            Err(error) => panic!("JSON diagnostics should be UTF-8: {error:?}"),
        };

        let output_json: serde_json::Value = match serde_json::from_str(&output) {
            Ok(value) => value,
            Err(error) => panic!("JSON diagnostics should parse: {error:?}"),
        };

        let location = &output_json["diagnostics"][0]["primary_span"]["location"];

        assert_eq!(
            output_json["diagnostics"][0]["primary_span"]["source_origin"]["kind"],
            "file"
        );

        assert_eq!(
            output_json["diagnostics"][0]["primary_span"]["source_origin"]["file_path"],
            "main.bray"
        );

        assert_eq!(location["start"]["line"], 2);
        assert_eq!(location["start"]["column"], 1);
        assert_eq!(location["end"]["line"], 2);
        assert_eq!(location["end"]["column"], 1);
        assert_eq!(location["lsp_start"]["line"], 1);
        assert_eq!(location["lsp_start"]["character"], 0);
        assert_eq!(location["lsp_end"]["line"], 1);
        assert_eq!(location["lsp_end"]["character"], 1);
    }

    fn source_store(text: &str) -> SourceStore {
        let mut sources = SourceStore::new();

        match sources.insert(
            SourceIdentity::new(0),
            SourceOrigin::file("main.bray"),
            SourceVersion::new(0),
            text,
        ) {
            Ok(_) => {}
            Err(error) => panic!("test source should insert: {error:?}"),
        }

        sources
    }
}
