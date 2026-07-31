use bray_compilation::Compilation;
use bray_source::{LineIndex, SourceNewlinePolicy, SourceSnapshot};
use serde::Serialize;

use crate::OutputFormat;
use crate::inspection::{
    push_indented_report_value, push_report_value, render_pretty_json,
};
use crate::output::SourceOriginOutput;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SourceInspectionRenderError {
    SourceIndex,
    Json,
}

pub(crate) fn render_source_inspection(
    compilation: &Compilation,
    output_format: OutputFormat,
) -> Result<String, SourceInspectionRenderError> {
    let report = SourceInspectionReport::from_compilation(compilation)?;

    match output_format {
        OutputFormat::Text => Ok(render_text_report(&report)),
        OutputFormat::Json => {
            render_pretty_json(&report).map_err(|_| SourceInspectionRenderError::Json)
        }
    }
}

#[derive(Serialize)]
struct SourceInspectionReport<'source> {
    kind: &'static str,
    source_count: usize,
    sources: Vec<SourceInspection<'source>>,
}

impl<'source> SourceInspectionReport<'source> {
    fn from_compilation(
        compilation: &'source Compilation,
    ) -> Result<SourceInspectionReport<'source>, SourceInspectionRenderError> {
        let sources = compilation
            .sources()
            .iter()
            .map(SourceInspection::from_snapshot)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(SourceInspectionReport {
            kind: "source_inspection",
            source_count: sources.len(),
            sources,
        })
    }
}

#[derive(Serialize)]
struct SourceInspection<'source> {
    source_id: u32,
    identity: u32,
    version: u64,
    origin: SourceOriginOutput,
    checksum: u64,
    byte_len: u32,
    line_count: usize,
    line_starts: Vec<u32>,
    newline_policy: &'static str,
    text: &'source str,
}

impl<'source> SourceInspection<'source> {
    fn from_snapshot(
        snapshot: &'source SourceSnapshot,
    ) -> Result<Self, SourceInspectionRenderError> {
        let line_index = LineIndex::new(snapshot.text())
            .map_err(|_| SourceInspectionRenderError::SourceIndex)?;

        Ok(Self {
            source_id: snapshot.source_id().raw(),
            identity: snapshot.identity().raw(),
            version: snapshot.version().raw(),
            origin: SourceOriginOutput::from_origin(snapshot.origin()),
            checksum: snapshot.checksum().raw(),
            byte_len: snapshot.text_len().bytes(),
            line_count: line_index.line_count(),
            line_starts: line_start_offsets(&line_index)?,
            newline_policy: SourceNewlinePolicy::DEFAULT.as_str(),
            text: snapshot.text(),
        })
    }
}

fn line_start_offsets(index: &LineIndex) -> Result<Vec<u32>, SourceInspectionRenderError> {
    let mut starts = Vec::with_capacity(index.line_count());

    for line in 0..index.line_count() {
        let line = u32::try_from(line).map_err(|_| SourceInspectionRenderError::SourceIndex)?;

        let start = index
            .line_start(line)
            .ok_or(SourceInspectionRenderError::SourceIndex)?;

        starts.push(start.bytes());
    }

    Ok(starts)
}

fn render_text_report(report: &SourceInspectionReport<'_>) -> String {
    let mut output = String::new();

    push_report_value(&mut output, "kind", report.kind);
    push_report_value(&mut output, "source_count", report.source_count);

    for source in &report.sources {
        output.push('\n');
        push_text_source(&mut output, source);
    }

    output
}

fn push_text_source(output: &mut String, source: &SourceInspection) {
    push_report_value(output, "source_id", source.source_id);

    push_indented_report_value(output, "identity", source.identity);
    push_indented_report_value(output, "version", source.version);
    push_indented_report_value(output, "origin_kind", source.origin.kind());
    push_text_origin_detail(output, &source.origin);
    push_indented_report_value(output, "checksum", format!("0x{:016x}", source.checksum));
    push_indented_report_value(output, "byte_len", source.byte_len);
    push_indented_report_value(output, "line_count", source.line_count);

    push_indented_report_value(
        output,
        "line_starts",
        format_line_starts(&source.line_starts),
    );

    push_indented_report_value(output, "newline_policy", source.newline_policy);

    output.push_str("  text:\n");
    output.push_str(source.text);

    if !source.text.ends_with('\n') {
        output.push('\n');
    }
}

fn push_text_origin_detail(output: &mut String, origin: &SourceOriginOutput) {
    if let Some(path) = origin.file_path() {
        push_indented_report_value(output, "file_path", path);
    }

    if let Some(name) = origin.virtual_name() {
        push_indented_report_value(output, "virtual_name", name);
    }

    if let Some(name) = origin.generated_name() {
        push_indented_report_value(output, "generated_name", name);
    }

    if let Some(uri) = origin.lsp_uri() {
        push_indented_report_value(output, "lsp_uri", uri);
    }

    if let Some(name) = origin.test_fixture_name() {
        push_indented_report_value(output, "test_fixture_name", name);
    }
}

fn format_line_starts(starts: &[u32]) -> String {
    let mut output = String::from("[");

    for (index, start) in starts.iter().enumerate() {
        if index > 0 {
            output.push_str(", ");
        }

        output.push_str(&start.to_string());
    }

    output.push(']');

    output
}

#[cfg(test)]
mod tests {
    use bray_compilation::Compilation;
    use bray_source::{SourceIdentity, SourceInput, SourceVersion};

    use super::render_source_inspection;
    use crate::OutputFormat;
    use crate::test_support::package_identity;

    #[test]
    fn text_inspection_renders_loaded_source_metadata_and_text() {
        let compilation = compilation_from_text("module main\r\nfunc main() {}\n");

        let output = match render_source_inspection(&compilation, OutputFormat::Text) {
            Ok(output) => output,
            Err(error) => panic!("source inspection should render: {error:?}"),
        };

        assert!(output.contains("kind: source_inspection"));
        assert!(output.contains("source_count: 1"));
        assert!(output.contains("source_id: 0"));
        assert!(output.contains("  identity: 0"));
        assert!(output.contains("  origin_kind: virtual"));
        assert!(output.contains("  virtual_name: main"));
        assert!(output.contains("  byte_len: 28"));
        assert!(output.contains("  line_count: 3"));
        assert!(output.contains("  line_starts: [0, 13, 28]"));
        assert!(output.contains("  newline_policy: preserve_exact_text"));
        assert!(output.contains("module main\r\nfunc main() {}\n"));
    }

    #[test]
    fn json_inspection_renders_structured_loaded_source_metadata() {
        let compilation = compilation_from_text("module main\n");

        let output = match render_source_inspection(&compilation, OutputFormat::Json) {
            Ok(output) => output,
            Err(error) => panic!("source inspection should render: {error:?}"),
        };

        let output_json: serde_json::Value = match serde_json::from_str(&output) {
            Ok(value) => value,
            Err(error) => panic!("source inspection JSON should parse: {error:?}"),
        };

        assert_eq!(output_json["kind"], "source_inspection");
        assert_eq!(output_json["source_count"], 1);
        assert_eq!(output_json["sources"][0]["source_id"], 0);
        assert_eq!(output_json["sources"][0]["identity"], 0);
        assert_eq!(output_json["sources"][0]["origin"]["kind"], "virtual");
        assert_eq!(output_json["sources"][0]["origin"]["virtual_name"], "main");
        assert_eq!(output_json["sources"][0]["byte_len"], 12);
        assert_eq!(output_json["sources"][0]["line_count"], 2);

        assert_eq!(
            output_json["sources"][0]["line_starts"],
            serde_json::json!([0, 12])
        );

        assert_eq!(output_json["sources"][0]["text"], "module main\n");
    }

    fn compilation_from_text(text: &str) -> Compilation {
        let input =
            SourceInput::virtual_text(SourceIdentity::new(0), "main", SourceVersion::new(0), text);

        match Compilation::load_sources(package_identity(), vec![input]) {
            Ok(compilation) => compilation,
            Err(error) => panic!("test compilation should build: {error:?}"),
        }
    }
}
