use std::borrow::Borrow;

use bray_diagnostics::{DiagnosticBag, DiagnosticKind};
use bray_source::{SourceId, SourceSnapshot, SourceStore};
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
