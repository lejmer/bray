use bray_declarations::SyntaxAnchor;
use std::borrow::Cow;

use bray_diagnostics::DiagnosticBag;
use bray_source::{LineIndex, SourceLocation, SourceSnapshot, SourceSpan, SourceStore, TextRange};
use bray_symbols::{AnySymbolId, SymbolGraph};
use bray_syntax::SyntaxTrivia;
use serde::Serialize;

use crate::output::{
    DiagnosticJson, SourceLocationOutput, SourceOriginOutput, TextRangeOutput,
};

pub(crate) struct InspectionOutput {
    stdout: String,
    diagnostics: DiagnosticBag,
}

impl InspectionOutput {
    pub(crate) const fn new(stdout: String, diagnostics: DiagnosticBag) -> Self {
        Self {
            stdout,
            diagnostics,
        }
    }

    pub(crate) fn into_parts(self) -> (String, DiagnosticBag) {
        (self.stdout, self.diagnostics)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InspectionSourceError {
    Source,
    SourceIndex,
}

pub(crate) struct InspectionSources<'source> {
    sources: &'source SourceStore,
    line_indices: Vec<LineIndex>,
}

impl<'source> InspectionSources<'source> {
    pub(crate) fn new(sources: &'source SourceStore) -> Result<Self, InspectionSourceError> {
        let line_indices = sources
            .iter()
            .map(|snapshot| LineIndex::new(snapshot.text()))
            .collect::<Result<_, _>>()
            .map_err(|_| InspectionSourceError::SourceIndex)?;

        Ok(Self {
            sources,
            line_indices,
        })
    }

    fn snapshot_and_index(
        &self,
        source_id: bray_source::SourceId,
    ) -> Result<(&SourceSnapshot, &LineIndex), InspectionSourceError> {
        let snapshot = self
            .sources
            .get(source_id)
            .ok_or(InspectionSourceError::Source)?;

        let line_index = self
            .line_indices
            .get(
                source_id
                    .to_index()
                    .ok_or(InspectionSourceError::SourceIndex)?,
            )
            .ok_or(InspectionSourceError::SourceIndex)?;

        Ok((snapshot, line_index))
    }
}

#[derive(Serialize)]
pub(crate) struct InspectionSyntaxAnchor {
    source_id: u32,
    display_name: String,
    syntax_kind: &'static str,
    span: TextRangeOutput,
    location: SourceLocationOutput,
    recovered: bool,
}

#[derive(Clone, Eq, PartialEq, Serialize)]
pub(crate) struct InspectionSymbolIdentity {
    symbol_kind: &'static str,
    id: u32,
    name: Option<String>,
}

impl InspectionSymbolIdentity {
    pub(crate) fn from_symbol(symbols: &SymbolGraph, id: AnySymbolId) -> Self {
        Self {
            symbol_kind: id.kind().as_str(),
            id: id.symbol_id().raw(),
            name: symbol_name(symbols, id),
        }
    }

    pub(crate) fn text(&self) -> String {
        let name = self
            .name
            .as_ref()
            .map(|name| format!(" {name}"))
            .unwrap_or_default();

        format!("{}{name} [symbol:{}]", self.symbol_kind, self.id)
    }

    pub(crate) fn display_name(&self) -> Cow<'_, str> {
        match &self.name {
            Some(name) => Cow::Borrowed(name),
            None => Cow::Owned(format!("<{}:{}>", self.symbol_kind, self.id)),
        }
    }
}

fn symbol_name(symbols: &SymbolGraph, id: AnySymbolId) -> Option<String> {
    match id {
        AnySymbolId::CompilerKnownEnvironment(_) => Some(String::from("compiler-known")),
        AnySymbolId::Package(id) => symbols
            .package(id)
            .map(|package| package.identity().as_str().to_owned()),
        AnySymbolId::Module(id) => symbols.module(id).map(|module| {
            let path = module.path().segments().collect::<Vec<_>>().join(".");

            if path.is_empty() {
                String::from("<recovered>")
            } else {
                path
            }
        }),
        _ => symbols.member_name(id).map(|name| name.as_str().to_owned()),
    }
}

impl InspectionSyntaxAnchor {
    pub(crate) fn from_anchor(
        sources: &InspectionSources<'_>,
        anchor: SyntaxAnchor,
    ) -> Result<Self, InspectionSourceError> {
        let (snapshot, line_index) = sources.snapshot_and_index(anchor.source_id())?;

        let location = location_for_range(snapshot, line_index, anchor.full_range())
            .ok_or(InspectionSourceError::SourceIndex)?;

        let origin = SourceOriginOutput::from_origin(snapshot.origin());

        Ok(Self {
            source_id: anchor.source_id().raw(),
            display_name: origin.display_name().to_owned(),
            syntax_kind: anchor.syntax_kind().as_str(),
            span: TextRangeOutput::from_range(anchor.full_range()),
            location,
            recovered: anchor.is_recovered(),
        })
    }

    pub(crate) fn text(&self) -> String {
        format!(
            "{} {} {} @{}{}",
            self.syntax_kind,
            self.display_name,
            location_range_text(self.location),
            range_text(self.span),
            self.recovery_text()
        )
    }

    pub(crate) fn display_name(&self) -> &str {
        &self.display_name
    }

    pub(crate) fn location_text(&self) -> String {
        format!(
            "{} @{}",
            location_range_text(self.location),
            range_text(self.span)
        )
    }

    pub(crate) const fn recovery_text(&self) -> &'static str {
        if self.recovered { " [recovered]" } else { "" }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InspectionTriviaError {
    SourceIndex,
    Text,
}

#[derive(Serialize)]
pub(crate) struct InspectionTrivia {
    kind: &'static str,
    text: String,
    escaped_text: String,
    span: TextRangeOutput,
    location: SourceLocationOutput,
}

impl InspectionTrivia {
    fn from_trivia(
        snapshot: &SourceSnapshot,
        line_index: &LineIndex,
        trivia: &SyntaxTrivia,
    ) -> Result<Self, InspectionTriviaError> {
        let text = trivia
            .text(snapshot.text())
            .ok_or(InspectionTriviaError::Text)?;

        let location = location_for_range(snapshot, line_index, trivia.range())
            .ok_or(InspectionTriviaError::SourceIndex)?;

        Ok(Self {
            kind: trivia.kind().as_str(),
            text: text.to_owned(),
            escaped_text: escaped_text(text),
            span: TextRangeOutput::from_range(trivia.range()),
            location,
        })
    }
}

pub(crate) fn trivia_entries(
    snapshot: &SourceSnapshot,
    line_index: &LineIndex,
    trivia: &[SyntaxTrivia],
) -> Result<Vec<InspectionTrivia>, InspectionTriviaError> {
    trivia
        .iter()
        .map(|trivia| InspectionTrivia::from_trivia(snapshot, line_index, trivia))
        .collect()
}

pub(crate) fn trivia_summary(
    leading: &[InspectionTrivia],
    trailing: &[InspectionTrivia],
) -> String {
    let mut output = String::new();

    push_trivia_summary(&mut output, "leading", leading);
    push_trivia_summary(&mut output, "trailing", trailing);

    output
}

fn push_trivia_summary(output: &mut String, label: &str, trivia: &[InspectionTrivia]) {
    if trivia.is_empty() {
        return;
    }

    output.push(' ');
    output.push_str(label);
    output.push('=');

    for (index, trivia) in trivia.iter().enumerate() {
        if index > 0 {
            output.push_str(", ");
        }

        output.push_str(trivia.kind);
        output.push('@');
        output.push_str(&range_text(trivia.span));
        output.push_str(":\"");
        output.push_str(&trivia.escaped_text);
        output.push('"');
    }
}

#[derive(Default)]
pub(crate) struct TreeWriter {
    output: String,
    prefix: String,
    continuations: Vec<bool>,
}

impl TreeWriter {
    pub(crate) fn new(prefix: impl Into<String>) -> Self {
        Self {
            output: String::new(),
            prefix: prefix.into(),
            continuations: Vec::new(),
        }
    }

    pub(crate) fn push_line(&mut self, is_last: bool, text: &str) {
        self.output.push_str(&self.prefix);

        for continues in &self.continuations {
            self.output.push_str(if *continues { "│  " } else { "   " });
        }

        self.output.push_str(if is_last { "└─ " } else { "├─ " });
        self.output.push_str(text);
        self.output.push('\n');
    }

    pub(crate) fn enter_children(&mut self, parent_is_last: bool) {
        self.continuations.push(!parent_is_last);
    }

    pub(crate) fn leave_children(&mut self) {
        if self.continuations.pop().is_none() {
            panic!("tree writer cannot leave a missing child level");
        }
    }

    pub(crate) fn into_string(self) -> String {
        self.output
    }
}

pub(crate) fn location_for_range(
    snapshot: &SourceSnapshot,
    line_index: &LineIndex,
    range: TextRange,
) -> Option<SourceLocationOutput> {
    let span = SourceSpan::new(snapshot.source_id(), range);
    let location = SourceLocation::resolve(snapshot, line_index, span)?;

    Some(SourceLocationOutput::from_location(location))
}

pub(crate) fn location_start_text(location: SourceLocationOutput) -> String {
    let start = location.start();

    format!("{}:{}", start.line(), start.column())
}

pub(crate) fn location_range_text(location: SourceLocationOutput) -> String {
    let start = location.start();
    let end = location.end();

    if start == end {
        return format!("{}:{}", start.line(), start.column());
    }

    format!(
        "{}:{}..{}:{}",
        start.line(),
        start.column(),
        end.line(),
        end.column()
    )
}

pub(crate) fn range_text(range: TextRangeOutput) -> String {
    format!("{}..{}", range.start(), range.end())
}

pub(crate) fn escaped_text(text: &str) -> String {
    text.escape_debug().to_string()
}

pub(crate) fn quoted_text(text: &str) -> String {
    format!("\"{text}\"")
}

pub(crate) fn push_text_diagnostic(output: &mut String, diagnostic: &DiagnosticJson) {
    output.push_str("    ");
    output.push_str(diagnostic.severity());
    output.push('[');
    output.push_str(&diagnostic.code().to_string());
    output.push_str("]: ");
    output.push_str(diagnostic.kind());

    if let Some(primary_span) = diagnostic.primary_span() {
        if let Some(location) = primary_span.location() {
            output.push(' ');
            output.push_str(&location_start_text(location));
        }

        output.push(' ');
        output.push_str(&primary_span.start().to_string());
        output.push_str("..");
        output.push_str(&primary_span.end().to_string());
    }

    output.push('\n');
}

#[cfg(test)]
mod tests {
    use super::TreeWriter;

    #[test]
    fn tree_writer_keeps_ancestor_guides_connected() {
        let mut writer = TreeWriter::default();

        writer.push_line(false, "first");
        writer.enter_children(false);
        writer.push_line(true, "nested");
        writer.leave_children();
        writer.push_line(true, "second");

        assert_eq!(writer.into_string(), "├─ first\n│  └─ nested\n└─ second\n");
    }
}
