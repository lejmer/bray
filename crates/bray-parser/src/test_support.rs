use std::borrow::Borrow;

use bray_diagnostics::{DiagnosticArg, DiagnosticBag, DiagnosticKind, SeverityKind};
use bray_source::{SourceId, SourceSnapshot, SourceStore, TextRange, TextSize};
use bray_syntax::{SyntaxKind, SyntaxToken};

use crate::SyntaxTreeResult;

pub(crate) fn source(sources: &SourceStore, index: u32) -> SourceSnapshot {
    match sources.get(SourceId::new(index)) {
        Some(snapshot) => snapshot.clone(),
        None => panic!("source should exist"),
    }
}

pub(crate) fn token_kinds<Token>(tokens: impl IntoIterator<Item = Token>) -> Vec<SyntaxKind>
where
    Token: Borrow<SyntaxToken>,
{
    tokens
        .into_iter()
        .map(|token| token.borrow().kind())
        .collect()
}

pub(crate) fn parse_diagnostic_kinds(result: &SyntaxTreeResult) -> Vec<DiagnosticKind> {
    diagnostic_kinds(result.diagnostics())
}

pub(crate) fn diagnostic_kinds(diagnostics: &DiagnosticBag) -> Vec<DiagnosticKind> {
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.kind())
        .collect()
}

pub(crate) fn marker_offset(source: &str, marker: &str) -> TextSize {
    let offset = match source.find(marker) {
        Some(offset) => offset,
        None => panic!("test source should contain marker {marker:?}"),
    };

    let offset = match u32::try_from(offset) {
        Ok(offset) => offset,
        Err(_) => panic!("test source marker offset should fit in TextSize"),
    };

    TextSize::new(offset)
}

pub(crate) fn assert_missing_semicolon_diagnostic(
    result: &SyntaxTreeResult,
    insertion: TextSize,
    actual_kind: SyntaxKind,
    actual_text: &str,
    expected_kinds: &[DiagnosticKind],
) {
    let expected_diagnostic = match result
        .diagnostics()
        .iter()
        .find(|diagnostic| diagnostic.kind() == DiagnosticKind::SyntaxExpectedToken)
    {
        Some(diagnostic) => diagnostic,
        None => panic!("expected missing semicolon diagnostic"),
    };

    assert_eq!(parse_diagnostic_kinds(result).as_slice(), expected_kinds);
    assert_eq!(expected_diagnostic.severity(), SeverityKind::Error);

    assert_eq!(
        expected_diagnostic.primary_span().map(|span| span.range()),
        Some(TextRange::empty(insertion))
    );

    assert_eq!(
        expected_diagnostic.args(),
        &[
            DiagnosticArg::expected_syntax_kind(SyntaxKind::SemicolonToken),
            DiagnosticArg::actual_syntax_kind(actual_kind),
            DiagnosticArg::token_text(actual_text),
        ]
    );
}
