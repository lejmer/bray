use bray_compilation::Compilation;
use bray_diagnostics::DiagnosticBag;
use bray_parser::lex_source_unit;
use bray_source::{LineIndex, SourceSnapshot, SourceStore};
use bray_syntax::SyntaxToken;
use serde::Serialize;

use crate::command::DriverOutputFormat;
use crate::inspection::{
    InspectionOutput, InspectionTrivia, InspectionTriviaError, escaped_text, location_for_range,
    location_start_text, push_text_diagnostic, quoted_text, range_text, trivia_entries,
    trivia_summary,
};
use crate::output::{
    DiagnosticJson, SourceLocationOutput, SourceOriginOutput, TextRangeOutput, diagnostic_jsons,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TokenInspectionRenderError {
    SourceIndex,
    TokenText,
    TriviaText,
    Json,
}

pub(crate) fn render_token_inspection(
    compilation: &Compilation,
    output_format: DriverOutputFormat,
) -> Result<InspectionOutput, TokenInspectionRenderError> {
    let (report, diagnostics) = TokenInspectionReport::from_compilation(compilation)?;

    let stdout = match output_format {
        DriverOutputFormat::Text => render_text_report(&report),
        DriverOutputFormat::Json => render_json_report(&report)?,
    };

    Ok(InspectionOutput::new(stdout, diagnostics))
}

#[derive(Serialize)]
struct TokenInspectionReport {
    kind: &'static str,
    source_count: usize,
    has_errors: bool,
    sources: Vec<TokenInspectionSource>,
}

impl TokenInspectionReport {
    fn from_compilation(
        compilation: &Compilation,
    ) -> Result<(Self, DiagnosticBag), TokenInspectionRenderError> {
        let mut sources = Vec::with_capacity(compilation.source_count());
        let mut diagnostic_bags = Vec::with_capacity(compilation.source_count());

        for snapshot in compilation.sources() {
            let (source, diagnostics) =
                TokenInspectionSource::from_snapshot(snapshot, compilation.sources())?;

            sources.push(source);
            diagnostic_bags.push(diagnostics);
        }

        let diagnostics = DiagnosticBag::merged_all(diagnostic_bags.iter());
        let has_errors = diagnostics.has_errors();

        Ok((
            Self {
                kind: "token_inspection",
                source_count: sources.len(),
                has_errors,
                sources,
            },
            diagnostics,
        ))
    }
}

#[derive(Serialize)]
struct TokenInspectionSource {
    unit_kind: &'static str,
    source_id: u32,
    identity: u32,
    version: u64,
    origin: SourceOriginOutput,
    display_name: String,
    token_count: usize,
    diagnostic_count: usize,
    tokens: Vec<TokenInspectionToken>,
    diagnostics: Vec<DiagnosticJson>,
}

impl TokenInspectionSource {
    fn from_snapshot(
        snapshot: &SourceSnapshot,
        sources: &SourceStore,
    ) -> Result<(Self, DiagnosticBag), TokenInspectionRenderError> {
        let line_index =
            LineIndex::new(snapshot.text()).map_err(|_| TokenInspectionRenderError::SourceIndex)?;

        let lex_result = lex_source_unit(snapshot);

        let (lexed_tokens, diagnostics) = lex_result.into_parts();

        let tokens = lexed_tokens
            .iter()
            .map(|token| TokenInspectionToken::from_token(snapshot, &line_index, token))
            .collect::<Result<Vec<_>, _>>()?;

        let diagnostic_count = diagnostics.len();
        let diagnostic_json = diagnostic_jsons(&diagnostics, Some(sources));
        let origin = SourceOriginOutput::from_origin(snapshot.origin());
        let display_name = origin.display_name().to_owned();

        Ok((
            Self {
                unit_kind: "source_unit",
                source_id: snapshot.source_id().raw(),
                identity: snapshot.identity().raw(),
                version: snapshot.version().raw(),
                origin,
                display_name,
                token_count: tokens.len(),
                diagnostic_count,
                tokens,
                diagnostics: diagnostic_json,
            },
            diagnostics,
        ))
    }
}

#[derive(Serialize)]
struct TokenInspectionToken {
    kind: &'static str,
    text: String,
    escaped_text: String,
    span: TextRangeOutput,
    location: SourceLocationOutput,
    leading_trivia: Vec<InspectionTrivia>,
    trailing_trivia: Vec<InspectionTrivia>,
}

impl TokenInspectionToken {
    fn from_token(
        snapshot: &SourceSnapshot,
        line_index: &LineIndex,
        token: &SyntaxToken,
    ) -> Result<Self, TokenInspectionRenderError> {
        let text = match token.text(snapshot.text()) {
            Some(text) => text,
            None => return Err(TokenInspectionRenderError::TokenText),
        };

        Ok(Self {
            kind: token.kind().as_str(),
            text: text.to_owned(),
            escaped_text: escaped_text(text),
            span: TextRangeOutput::from_range(token.range()),
            location: location_for_range(snapshot, line_index, token.range())
                .ok_or(TokenInspectionRenderError::SourceIndex)?,
            leading_trivia: trivia_entries(snapshot, line_index, token.leading_trivia())
                .map_err(map_trivia_error)?,
            trailing_trivia: trivia_entries(snapshot, line_index, token.trailing_trivia())
                .map_err(map_trivia_error)?,
        })
    }
}

const fn map_trivia_error(error: InspectionTriviaError) -> TokenInspectionRenderError {
    match error {
        InspectionTriviaError::SourceIndex => TokenInspectionRenderError::SourceIndex,
        InspectionTriviaError::Text => TokenInspectionRenderError::TriviaText,
    }
}

fn render_text_report(report: &TokenInspectionReport) -> String {
    let mut output = String::new();

    output.push_str("kind: ");
    output.push_str(report.kind);
    output.push('\n');

    push_value(&mut output, "source_count", report.source_count);
    push_value(&mut output, "has_errors", report.has_errors);

    for source in &report.sources {
        output.push('\n');
        push_text_source(&mut output, source);
    }

    output
}

fn push_text_source(output: &mut String, source: &TokenInspectionSource) {
    output.push_str("source_unit: ");
    output.push_str(source.origin.kind());
    output.push(' ');
    output.push_str(&source.display_name);
    output.push('\n');

    push_indented_value(output, "source_id", source.source_id);
    push_indented_value(output, "identity", source.identity);
    push_indented_value(output, "version", source.version);
    push_indented_value(output, "token_count", source.token_count);
    push_indented_value(output, "diagnostic_count", source.diagnostic_count);

    output.push_str("  tokens:\n");

    let token_rows = source
        .tokens
        .iter()
        .map(TokenTextRow::from_token)
        .collect::<Vec<_>>();

    let column_widths = TokenColumnWidths::from_rows(&token_rows);

    for row in &token_rows {
        push_text_token(output, row, column_widths);
    }

    output.push_str("  diagnostics:\n");

    if source.diagnostics.is_empty() {
        output.push_str("    none\n");
        return;
    }

    for diagnostic in &source.diagnostics {
        push_text_diagnostic(output, diagnostic);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TokenColumnWidths {
    location: usize,
    span: usize,
    kind: usize,
    text: usize,
}

impl TokenColumnWidths {
    fn from_rows(rows: &[TokenTextRow]) -> Self {
        let mut widths = Self {
            location: 0,
            span: 0,
            kind: 0,
            text: 0,
        };

        for row in rows {
            widths.location = widths.location.max(row.location.len());
            widths.span = widths.span.max(row.span.len());
            widths.kind = widths.kind.max(row.kind.len());
            widths.text = widths.text.max(row.text.len());
        }

        widths
    }
}

#[derive(Debug, Eq, PartialEq)]
struct TokenTextRow {
    location: String,
    span: String,
    kind: &'static str,
    text: String,
    trivia: String,
}

impl TokenTextRow {
    fn from_token(token: &TokenInspectionToken) -> Self {
        Self {
            location: location_text(token.location),
            span: range_text(token.span),
            kind: token.kind,
            text: quoted_text(&token.escaped_text),
            trivia: trivia_summary(&token.leading_trivia, &token.trailing_trivia),
        }
    }
}

fn push_text_token(output: &mut String, row: &TokenTextRow, widths: TokenColumnWidths) {
    output.push_str("    ");
    push_padded(output, &row.location, widths.location);
    output.push_str("  ");
    push_padded(output, &row.span, widths.span);
    output.push_str("  ");
    push_padded(output, row.kind, widths.kind);
    output.push_str("  ");

    if row.trivia.is_empty() {
        output.push_str(&row.text);
    } else {
        push_padded(output, &row.text, widths.text);
        output.push_str("  ");
        output.push_str(&row.trivia);
    }

    output.push('\n');
}

fn location_text(location: SourceLocationOutput) -> String {
    location_start_text(location)
}

fn push_padded(output: &mut String, value: &str, width: usize) {
    output.push_str(value);

    for _ in value.len()..width {
        output.push(' ');
    }
}

fn push_value<T: ToString>(output: &mut String, key: &str, value: T) {
    output.push_str(key);
    output.push_str(": ");
    output.push_str(&value.to_string());
    output.push('\n');
}

fn push_indented_value<T: ToString>(output: &mut String, key: &str, value: T) {
    output.push_str("  ");
    push_value(output, key, value);
}

fn render_json_report(
    report: &TokenInspectionReport,
) -> Result<String, TokenInspectionRenderError> {
    let mut output =
        serde_json::to_string_pretty(report).map_err(|_| TokenInspectionRenderError::Json)?;

    output.push('\n');

    Ok(output)
}

#[cfg(test)]
mod tests {
    use bray_compilation::Compilation;
    use bray_diagnostics::DiagnosticKind;
    use bray_source::{SourceIdentity, SourceInput, SourceVersion};

    use super::render_token_inspection;
    use crate::DriverOutputFormat;
    use crate::test_support::package_identity;

    #[test]
    fn text_inspection_renders_tokens_trivia_and_source_grouping() {
        let compilation = compilation_from_inputs(vec![
            SourceInput::virtual_text(
                SourceIdentity::new(0),
                "first",
                SourceVersion::new(0),
                "  func // tail\nmain",
            ),
            SourceInput::virtual_text(SourceIdentity::new(1), "second", SourceVersion::new(0), "$"),
        ]);

        let output = match render_token_inspection(&compilation, DriverOutputFormat::Text) {
            Ok(output) => output,
            Err(error) => panic!("token inspection should render: {error:?}"),
        };

        let (stdout, diagnostics) = output.into_parts();

        assert_eq!(
            diagnostics
                .by_kind(DiagnosticKind::LexicalInvalidCharacter)
                .count(),
            1
        );

        assert!(stdout.contains("kind: token_inspection"));
        assert!(stdout.contains("source_count: 2"));
        assert!(stdout.contains("source_unit: virtual first"));
        assert!(stdout.contains("source_unit: virtual second"));
        assert!(stdout.contains("1:3  2..6    func_keyword"));
        assert!(stdout.contains("func_keyword       \"func\""));
        assert!(stdout.contains("leading=whitespace_trivia@0..2:\"  \""));
        assert!(stdout.contains("trailing=whitespace_trivia@6..7:\" \""));
        assert!(stdout.contains("line_comment_trivia@7..14:\"// tail\""));
        assert!(stdout.contains("error[2001]: lexical_invalid_character 1:1 0..1"));
    }

    #[test]
    fn json_inspection_renders_structured_tokens_trivia_and_diagnostics() {
        let compilation = compilation_from_inputs(vec![SourceInput::virtual_text(
            SourceIdentity::new(0),
            "main",
            SourceVersion::new(3),
            "func\n$",
        )]);

        let output = match render_token_inspection(&compilation, DriverOutputFormat::Json) {
            Ok(output) => output,
            Err(error) => panic!("token inspection should render: {error:?}"),
        };

        let (stdout, diagnostics) = output.into_parts();

        assert!(diagnostics.has_errors());

        let output_json: serde_json::Value = match serde_json::from_str(&stdout) {
            Ok(value) => value,
            Err(error) => panic!("token inspection JSON should parse: {error:?}"),
        };

        assert_eq!(output_json["kind"], "token_inspection");
        assert_eq!(output_json["source_count"], 1);
        assert_eq!(output_json["has_errors"], true);
        assert_eq!(output_json["sources"][0]["unit_kind"], "source_unit");
        assert_eq!(output_json["sources"][0]["source_id"], 0);
        assert_eq!(output_json["sources"][0]["identity"], 0);
        assert_eq!(output_json["sources"][0]["version"], 3);
        assert_eq!(output_json["sources"][0]["origin"]["kind"], "virtual");
        assert_eq!(output_json["sources"][0]["display_name"], "main");

        assert_eq!(
            output_json["sources"][0]["tokens"][0]["kind"],
            "func_keyword"
        );

        assert_eq!(output_json["sources"][0]["tokens"][0]["text"], "func");
        assert_eq!(output_json["sources"][0]["tokens"][0]["span"]["start"], 0);

        assert_eq!(
            output_json["sources"][0]["tokens"][0]["location"]["start"]["line"],
            1
        );

        assert_eq!(
            output_json["sources"][0]["tokens"][0]["trailing_trivia"][0]["kind"],
            "whitespace_trivia"
        );

        assert_eq!(
            output_json["sources"][0]["diagnostics"][0]["kind"],
            "lexical_invalid_character"
        );

        assert_eq!(output_json["sources"][0]["diagnostics"][0]["code"], 2001);
    }

    fn compilation_from_inputs(inputs: Vec<SourceInput>) -> Compilation {
        match Compilation::load_sources(package_identity(), inputs) {
            Ok(compilation) => compilation,
            Err(error) => panic!("test compilation should load: {error:?}"),
        }
    }
}
