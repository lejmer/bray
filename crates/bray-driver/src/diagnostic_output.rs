use std::io::{self, Write};

use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticArgValue, DiagnosticBag, DiagnosticLabel, DiagnosticNote,
};
use bray_messages::{DiagnosticRenderer, RenderedDiagnostic, RenderedDiagnosticLabel};
use bray_source::SourceSpan;
use serde::Serialize;

use crate::command::DriverOutputFormat;
use crate::output_path::path_to_output_string;
use crate::run::DriverRunResult;
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
        DriverOutputFormat::Text => write_text_diagnostics(result.diagnostics(), stderr),
        DriverOutputFormat::Json => write_json_diagnostics(result.diagnostics(), stdout),
    }
}

fn write_text_diagnostics(diagnostics: &DiagnosticBag, writer: &mut impl Write) -> io::Result<()> {
    let renderer = DiagnosticRenderer::english();

    for diagnostic in renderer.render_bag(diagnostics) {
        write_text_diagnostic(renderer, &diagnostic, writer)?;
    }

    Ok(())
}

fn write_text_diagnostic(
    renderer: DiagnosticRenderer,
    diagnostic: &RenderedDiagnostic,
    writer: &mut impl Write,
) -> io::Result<()> {
    writeln!(
        writer,
        "{}[{}] {}: {}",
        color_severity_label(
            diagnostic.severity(),
            renderer.render_severity(diagnostic.severity())
        ),
        diagnostic.id().raw(),
        diagnostic.kind().as_str(),
        diagnostic.message()
    )?;

    if let Some(primary_span) = diagnostic.primary_span() {
        writeln!(
            writer,
            "  --> {}",
            renderer.render_source_span(primary_span)
        )?;
    }

    for label in diagnostic.labels() {
        write_text_label(renderer, label, writer)?;
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
    label: &RenderedDiagnosticLabel,
    writer: &mut impl Write,
) -> io::Result<()> {
    writeln!(
        writer,
        "  = {} {}: {}",
        renderer.render_label_style(label.style()),
        renderer.render_source_span(label.span()),
        label.message()
    )
}

fn write_json_diagnostics(diagnostics: &DiagnosticBag, writer: &mut impl Write) -> io::Result<()> {
    let report = DiagnosticJsonReport::from_bag(diagnostics);

    serde_json::to_writer_pretty(&mut *writer, &report).map_err(io::Error::other)?;
    writeln!(writer)
}

#[derive(Serialize)]
struct DiagnosticJsonReport {
    has_errors: bool,
    diagnostics: Vec<DiagnosticJson>,
}

impl DiagnosticJsonReport {
    fn from_bag(bag: &DiagnosticBag) -> Self {
        Self {
            has_errors: bag.has_errors(),
            diagnostics: bag.iter().map(DiagnosticJson::from_diagnostic).collect(),
        }
    }
}

#[derive(Serialize)]
struct DiagnosticJson {
    id: u32,
    kind: &'static str,
    severity: &'static str,
    primary_span: Option<SourceSpanJson>,
    labels: Vec<DiagnosticLabelJson>,
    notes: Vec<DiagnosticNoteJson>,
    args: Vec<DiagnosticArgJson>,
}

impl DiagnosticJson {
    fn from_diagnostic(diagnostic: &Diagnostic) -> Self {
        Self {
            id: diagnostic.id().raw(),
            kind: diagnostic.kind().as_str(),
            severity: diagnostic.severity().as_str(),
            primary_span: diagnostic.primary_span().map(SourceSpanJson::from_span),
            labels: diagnostic
                .labels()
                .iter()
                .map(DiagnosticLabelJson::from_label)
                .collect(),
            notes: diagnostic
                .notes()
                .iter()
                .map(DiagnosticNoteJson::from_note)
                .collect(),
            args: diagnostic
                .args()
                .iter()
                .map(DiagnosticArgJson::from_arg)
                .collect(),
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
    fn from_label(label: &DiagnosticLabel) -> Self {
        Self {
            kind: label.kind().as_str(),
            style: label.style().as_str(),
            span: SourceSpanJson::from_span(label.span()),
            args: label
                .args()
                .iter()
                .map(DiagnosticArgJson::from_arg)
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
    fn from_note(note: &DiagnosticNote) -> Self {
        Self {
            kind: note.kind().as_str(),
            args: note
                .args()
                .iter()
                .map(DiagnosticArgJson::from_arg)
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
    fn from_arg(arg: &DiagnosticArg) -> Self {
        Self {
            name: arg.name().as_str(),
            value: DiagnosticArgValueJson::from_value(arg.value()),
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
    fn from_value(value: &DiagnosticArgValue) -> Self {
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
                Self::SourceSpan(SourceSpanJson::from_span(*span))
            }
            DiagnosticArgValue::WorkerCount(worker_count) => Self::WorkerCount(*worker_count),
        }
    }
}

#[derive(Serialize)]
struct SourceSpanJson {
    source_id: u32,
    start: u32,
    end: u32,
}

impl SourceSpanJson {
    const fn from_span(span: SourceSpan) -> Self {
        Self {
            source_id: span.source_id().raw(),
            start: span.start().bytes(),
            end: span.end().bytes(),
        }
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{
        Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticKind, DiagnosticNote,
        DiagnosticNoteKind, SeverityKind,
    };
    use bray_source::TextSize;

    use super::{write_json_diagnostics, write_text_diagnostics};

    #[test]
    fn text_output_renders_colored_human_diagnostics() {
        let diagnostic = Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::SourceInvalidUtf8,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::text_offset(TextSize::new(4)))
        .with_note(DiagnosticNote::new(DiagnosticNoteKind::SourceMustBeUtf8));

        let bag = DiagnosticBag::single(diagnostic);

        let mut output = Vec::new();

        match write_text_diagnostics(&bag, &mut output) {
            Ok(()) => {}
            Err(error) => panic!("text diagnostics should write: {error:?}"),
        }

        let output = match String::from_utf8(output) {
            Ok(output) => output,
            Err(error) => panic!("text diagnostics should be UTF-8: {error:?}"),
        };

        assert!(output.contains("\x1b[31merror\x1b[0m[0] source_invalid_utf8"));
        assert!(output.contains("source input contains invalid UTF-8 at byte offset 4"));
        assert!(output.contains("\x1b[36mnote\x1b[0m: source inputs must be valid UTF-8"));
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

        match write_json_diagnostics(&bag, &mut output) {
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
        assert_eq!(diagnostic_json["kind"], "source_invalid_utf8");
        assert_eq!(diagnostic_json["severity"], "error");
        assert!(diagnostic_json.get("message").is_none());

        let arg_json = &diagnostic_json["args"][0];

        assert_eq!(arg_json["name"], "text_offset");
        assert_eq!(arg_json["value"]["kind"], "text_offset");
        assert_eq!(arg_json["value"]["value"], 5);
        assert!(!output.contains("source input contains invalid UTF-8 at byte offset"));
    }
}
