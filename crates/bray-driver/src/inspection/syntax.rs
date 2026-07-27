use bray_compilation::Compilation;
use bray_parser::SourceUnitSyntaxResult;
use bray_source::{LineIndex, SourceSnapshot, SourceStore};
use bray_syntax::{
    SourceUnitSyntax, SyntaxToken, SyntaxWalkControl, SyntaxWalkEvent, walk_source_unit,
};
use serde::Serialize;

use crate::command::DriverOutputFormat;
use crate::inspection::{
    InspectionOutput, InspectionTrivia, InspectionTriviaError, TreeWriter, escaped_text,
    location_for_range, location_range_text, push_text_diagnostic, quoted_text, range_text,
    trivia_entries, trivia_summary,
};
use crate::output::{
    DiagnosticJson, SourceLocationOutput, SourceOriginOutput, TextRangeOutput, diagnostic_jsons,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SyntaxInspectionRenderError {
    SourceIndex,
    SourceMismatch,
    TokenText,
    TriviaText,
    TreeStructure,
    Json,
}

pub(crate) fn render_syntax_inspection(
    compilation: &Compilation,
    output_format: DriverOutputFormat,
) -> Result<InspectionOutput, SyntaxInspectionRenderError> {
    let diagnostics = compilation.syntax_tree_result().diagnostics().clone();
    let report = SyntaxInspectionReport::from_compilation(compilation)?;

    let stdout = match output_format {
        DriverOutputFormat::Text => render_text_report(&report),
        DriverOutputFormat::Json => render_json_report(&report)?,
    };

    Ok(InspectionOutput::new(stdout, diagnostics))
}

#[derive(Serialize)]
struct SyntaxInspectionReport {
    kind: &'static str,
    source_count: usize,
    has_errors: bool,
    sources: Vec<SyntaxInspectionSource>,
}

impl SyntaxInspectionReport {
    fn from_compilation(
        compilation: &Compilation,
    ) -> Result<Self, SyntaxInspectionRenderError> {
        let mut sources = Vec::with_capacity(compilation.source_count());

        for snapshot in compilation.sources() {
            let result = compilation
                .source_unit_syntax(snapshot.source_id())
                .ok_or(SyntaxInspectionRenderError::SourceMismatch)?;

            sources.push(SyntaxInspectionSource::from_result(
                snapshot,
                result,
                compilation.sources(),
            )?);
        }

        Ok(Self {
            kind: "syntax_inspection",
            source_count: sources.len(),
            has_errors: compilation.syntax_tree_result().diagnostics().has_errors(),
            sources,
        })
    }
}

#[derive(Serialize)]
struct SyntaxInspectionSource {
    unit_kind: &'static str,
    source_id: u32,
    identity: u32,
    version: u64,
    origin: SourceOriginOutput,
    display_name: String,
    node_count: usize,
    token_count: usize,
    diagnostic_count: usize,
    tree: SyntaxInspectionElement,
    diagnostics: Vec<DiagnosticJson>,
}

impl SyntaxInspectionSource {
    fn from_result(
        snapshot: &SourceSnapshot,
        result: &SourceUnitSyntaxResult,
        sources: &SourceStore,
    ) -> Result<Self, SyntaxInspectionRenderError> {
        if result.source_id() != snapshot.source_id() {
            return Err(SyntaxInspectionRenderError::SourceMismatch);
        }

        let line_index =
            LineIndex::new(snapshot.text()).map_err(|_| SyntaxInspectionRenderError::SourceIndex)?;

        let (tree, node_count, token_count) =
            syntax_tree_elements(snapshot, &line_index, result.source_unit())?;

        let origin = SourceOriginOutput::from_origin(snapshot.origin());
        let display_name = origin.display_name().to_owned();
        let diagnostics = diagnostic_jsons(result.diagnostics(), Some(sources));

        Ok(Self {
            unit_kind: "source_unit",
            source_id: snapshot.source_id().raw(),
            identity: snapshot.identity().raw(),
            version: snapshot.version().raw(),
            origin,
            display_name,
            node_count,
            token_count,
            diagnostic_count: diagnostics.len(),
            tree,
            diagnostics,
        })
    }
}

#[derive(Serialize)]
#[serde(tag = "element_kind", rename_all = "snake_case")]
enum SyntaxInspectionElement {
    Node {
        kind: &'static str,
        span: TextRangeOutput,
        location: SourceLocationOutput,
        recovered: bool,
        children: Vec<SyntaxInspectionElement>,
    },
    Token {
        kind: &'static str,
        text: String,
        escaped_text: String,
        span: TextRangeOutput,
        location: SourceLocationOutput,
        missing: bool,
        end_of_file: bool,
        leading_trivia: Vec<InspectionTrivia>,
        trailing_trivia: Vec<InspectionTrivia>,
    },
}

impl SyntaxInspectionElement {
    fn node(
        snapshot: &SourceSnapshot,
        line_index: &LineIndex,
        node: bray_syntax::SyntaxNodeView<'_>,
    ) -> Result<Self, SyntaxInspectionRenderError> {
        let range = node.full_range();

        let location = location_for_range(snapshot, line_index, range)
            .ok_or(SyntaxInspectionRenderError::SourceIndex)?;

        Ok(Self::Node {
            kind: node.kind().as_str(),
            span: TextRangeOutput::from_range(range),
            location,
            recovered: node.is_recovered(),
            children: Vec::new(),
        })
    }

    fn token(
        snapshot: &SourceSnapshot,
        line_index: &LineIndex,
        token: &SyntaxToken,
    ) -> Result<Self, SyntaxInspectionRenderError> {
        let text = token
            .text(snapshot.text())
            .ok_or(SyntaxInspectionRenderError::TokenText)?;

        let location = location_for_range(snapshot, line_index, token.range())
            .ok_or(SyntaxInspectionRenderError::SourceIndex)?;

        Ok(Self::Token {
            kind: token.kind().as_str(),
            text: text.to_owned(),
            escaped_text: escaped_text(text),
            span: TextRangeOutput::from_range(token.range()),
            location,
            missing: token.is_missing(),
            end_of_file: token.is_end_of_file(),
            leading_trivia: trivia_entries(snapshot, line_index, token.leading_trivia())
                .map_err(map_trivia_error)?,
            trailing_trivia: trivia_entries(snapshot, line_index, token.trailing_trivia())
                .map_err(map_trivia_error)?,
        })
    }

    fn children_mut(&mut self) -> Option<&mut Vec<Self>> {
        match self {
            Self::Node { children, .. } => Some(children),
            Self::Token { .. } => None,
        }
    }

    fn push_text(&self, writer: &mut TreeWriter, is_last: bool) {
        writer.push_line(is_last, &self.text_line());

        let Self::Node { children, .. } = self else {
            return;
        };

        writer.enter_children(is_last);

        for (index, child) in children.iter().enumerate() {
            child.push_text(writer, index + 1 == children.len());
        }

        writer.leave_children();
    }

    fn text_line(&self) -> String {
        match self {
            Self::Node {
                kind,
                span,
                location,
                recovered,
                ..
            } => {
                let recovery = if *recovered { " [recovered]" } else { "" };

                format!(
                    "{kind} {} @{}{recovery}",
                    location_range_text(*location),
                    range_text(*span)
                )
            }
            Self::Token {
                kind,
                escaped_text,
                span,
                location,
                missing,
                leading_trivia,
                trailing_trivia,
                ..
            } => {
                let recovery = if *missing { " [missing]" } else { "" };
                let trivia = trivia_summary(leading_trivia, trailing_trivia);

                format!(
                    "{kind} {} @{} {}{recovery}{trivia}",
                    location_range_text(*location),
                    range_text(*span),
                    quoted_text(escaped_text)
                )
            }
        }
    }
}

fn syntax_tree_elements(
    snapshot: &SourceSnapshot,
    line_index: &LineIndex,
    source_unit: &SourceUnitSyntax,
) -> Result<(SyntaxInspectionElement, usize, usize), SyntaxInspectionRenderError> {
    let mut stack = Vec::new();
    let mut root = None;
    let mut error = None;
    let mut node_count = 0;
    let mut token_count = 0;

    walk_source_unit(source_unit, |event| {
        let result = match event {
            SyntaxWalkEvent::EnterNode(node) => {
                node_count += 1;

                SyntaxInspectionElement::node(snapshot, line_index, node)
                    .map(|node| stack.push(node))
            }
            SyntaxWalkEvent::Token(token) => {
                token_count += 1;

                SyntaxInspectionElement::token(snapshot, line_index, &token).and_then(|token| {
                    let parent = stack
                        .last_mut()
                        .and_then(SyntaxInspectionElement::children_mut)
                        .ok_or(SyntaxInspectionRenderError::TreeStructure)?;

                    parent.push(token);

                    Ok(())
                })
            }
            SyntaxWalkEvent::ExitNode(_) => finish_node(&mut stack, &mut root),
        };

        match result {
            Ok(()) => SyntaxWalkControl::Continue,
            Err(failure) => {
                error = Some(failure);

                SyntaxWalkControl::Stop
            }
        }
    });

    if let Some(error) = error {
        return Err(error);
    }

    if !stack.is_empty() {
        return Err(SyntaxInspectionRenderError::TreeStructure);
    }

    let root = root.ok_or(SyntaxInspectionRenderError::TreeStructure)?;

    Ok((root, node_count, token_count))
}

fn finish_node(
    stack: &mut Vec<SyntaxInspectionElement>,
    root: &mut Option<SyntaxInspectionElement>,
) -> Result<(), SyntaxInspectionRenderError> {
    let node = stack
        .pop()
        .ok_or(SyntaxInspectionRenderError::TreeStructure)?;

    match stack.last_mut() {
        Some(parent) => parent
            .children_mut()
            .ok_or(SyntaxInspectionRenderError::TreeStructure)?
            .push(node),
        None if root.is_none() => *root = Some(node),
        None => return Err(SyntaxInspectionRenderError::TreeStructure),
    }

    Ok(())
}

const fn map_trivia_error(error: InspectionTriviaError) -> SyntaxInspectionRenderError {
    match error {
        InspectionTriviaError::SourceIndex => SyntaxInspectionRenderError::SourceIndex,
        InspectionTriviaError::Text => SyntaxInspectionRenderError::TriviaText,
    }
}

fn render_text_report(report: &SyntaxInspectionReport) -> String {
    let mut output = String::new();

    output.push_str("kind: ");
    output.push_str(report.kind);
    output.push('\n');
    output.push_str("source_count: ");
    output.push_str(&report.source_count.to_string());
    output.push('\n');
    output.push_str("has_errors: ");
    output.push_str(&report.has_errors.to_string());
    output.push('\n');

    for source in &report.sources {
        output.push('\n');
        push_text_source(&mut output, source);
    }

    output
}

fn push_text_source(output: &mut String, source: &SyntaxInspectionSource) {
    output.push_str("source_unit: ");
    output.push_str(source.origin.kind());
    output.push(' ');
    output.push_str(&source.display_name);
    output.push('\n');

    push_indented_value(output, "source_id", source.source_id);
    push_indented_value(output, "identity", source.identity);
    push_indented_value(output, "version", source.version);
    push_indented_value(output, "node_count", source.node_count);
    push_indented_value(output, "token_count", source.token_count);
    push_indented_value(output, "diagnostic_count", source.diagnostic_count);

    output.push_str("  tree:\n");

    let mut writer = TreeWriter::new("    ");

    source.tree.push_text(&mut writer, true);
    output.push_str(&writer.into_string());

    output.push_str("  diagnostics:\n");

    if source.diagnostics.is_empty() {
        output.push_str("    none\n");

        return;
    }

    for diagnostic in &source.diagnostics {
        push_text_diagnostic(output, diagnostic);
    }
}

fn push_indented_value<T: ToString>(output: &mut String, key: &str, value: T) {
    output.push_str("  ");
    output.push_str(key);
    output.push_str(": ");
    output.push_str(&value.to_string());
    output.push('\n');
}

fn render_json_report(
    report: &SyntaxInspectionReport,
) -> Result<String, SyntaxInspectionRenderError> {
    let mut output =
        serde_json::to_string_pretty(report).map_err(|_| SyntaxInspectionRenderError::Json)?;

    output.push('\n');

    Ok(output)
}

#[cfg(test)]
mod tests {
    use bray_compilation::Compilation;
    use bray_source::{SourceIdentity, SourceInput, SourceVersion};

    use super::render_syntax_inspection;
    use crate::DriverOutputFormat;
    use crate::test_support::package_identity;

    #[test]
    fn text_inspection_renders_connected_tree_and_recovery() {
        let compilation = compilation("module main\n");

        let output = render_syntax_inspection(&compilation, DriverOutputFormat::Text)
            .unwrap_or_else(|error| panic!("syntax inspection should render: {error:?}"));

        let (stdout, diagnostics) = output.into_parts();

        assert!(diagnostics.has_errors());
        assert!(stdout.contains("kind: syntax_inspection"));
        assert!(stdout.contains("└─ source_unit"));
        assert!(stdout.contains("├─ module_keyword"));
        assert!(stdout.contains("│  "));
        assert!(stdout.contains("semicolon_token"));
        assert!(stdout.contains("[missing]"));
    }

    #[test]
    fn json_inspection_distinguishes_missing_tokens_and_present_eof() {
        let compilation = compilation("module main\n");

        let output = render_syntax_inspection(&compilation, DriverOutputFormat::Json)
            .unwrap_or_else(|error| panic!("syntax inspection should render: {error:?}"));

        let (stdout, diagnostics) = output.into_parts();

        assert!(diagnostics.has_errors());

        let json: serde_json::Value = serde_json::from_str(&stdout)
            .unwrap_or_else(|error| panic!("syntax inspection should be JSON: {error:?}"));

        let tree = &json["sources"][0]["tree"];
        let tokens = descendant_tokens(tree);

        let semicolon = tokens
            .iter()
            .find(|token| token["kind"] == "semicolon_token")
            .unwrap_or_else(|| panic!("missing semicolon should remain in the tree"));

        let eof = tokens
            .iter()
            .find(|token| token["kind"] == "end_of_file_token")
            .unwrap_or_else(|| panic!("EOF should remain in the tree"));

        assert_eq!(semicolon["missing"], true);
        assert_eq!(semicolon["span"]["start"], semicolon["span"]["end"]);
        assert_eq!(eof["missing"], false);
        assert_eq!(eof["end_of_file"], true);

        assert_eq!(reconstructed_source(tree), "module main\n");
    }

    #[test]
    fn json_inspection_preserves_skipped_syntax_and_later_valid_source() {
        let compilation = compilation(
            "module main;\n\
             func main()\n\
             {\n\
                 @value;\n\
                 return;\n\
             }\n",
        );

        let output = render_syntax_inspection(&compilation, DriverOutputFormat::Json)
            .unwrap_or_else(|error| panic!("syntax inspection should render: {error:?}"));

        let (stdout, diagnostics) = output.into_parts();

        assert!(diagnostics.has_errors());

        let json: serde_json::Value = serde_json::from_str(&stdout)
            .unwrap_or_else(|error| panic!("syntax inspection should be JSON: {error:?}"));

        let tree = &json["sources"][0]["tree"];

        let skipped = descendant_elements(tree)
            .into_iter()
            .find(|element| element["kind"] == "skipped_syntax")
            .unwrap_or_else(|| panic!("invalid expression syntax should remain reachable"));

        assert_eq!(skipped["recovered"], true);

        assert_eq!(
            reconstructed_source(tree),
            "module main;\nfunc main()\n{\n@value;\nreturn;\n}\n"
        );
    }

    fn descendant_tokens(root: &serde_json::Value) -> Vec<&serde_json::Value> {
        descendant_elements(root)
            .into_iter()
            .filter(|element| element["element_kind"] == "token")
            .collect()
    }

    fn descendant_elements(root: &serde_json::Value) -> Vec<&serde_json::Value> {
        let mut elements = Vec::new();
        let mut pending = vec![root];

        while let Some(element) = pending.pop() {
            elements.push(element);

            if let Some(children) = element["children"].as_array() {
                pending.extend(children.iter().rev());
            }
        }

        elements
    }

    fn reconstructed_source(root: &serde_json::Value) -> String {
        let mut source = String::new();

        for token in descendant_tokens(root) {
            for trivia in token["leading_trivia"]
                .as_array()
                .unwrap_or_else(|| panic!("token leading trivia should be an array"))
            {
                source.push_str(
                    trivia["text"]
                        .as_str()
                        .unwrap_or_else(|| panic!("trivia text should be a string")),
                );
            }

            source.push_str(
                token["text"]
                    .as_str()
                    .unwrap_or_else(|| panic!("token text should be a string")),
            );

            for trivia in token["trailing_trivia"]
                .as_array()
                .unwrap_or_else(|| panic!("token trailing trivia should be an array"))
            {
                source.push_str(
                    trivia["text"]
                        .as_str()
                        .unwrap_or_else(|| panic!("trivia text should be a string")),
                );
            }
        }

        source
    }

    fn compilation(source: &str) -> Compilation {
        match Compilation::load_sources(
            package_identity(),
            vec![SourceInput::virtual_text(
                SourceIdentity::new(0),
                "syntax",
                SourceVersion::new(0),
                source,
            )],
        ) {
            Ok(compilation) => compilation,
            Err(error) => panic!("test compilation should load: {error:?}"),
        }
    }
}
