use bray_diagnostics::DiagnosticBag;
use bray_source::{SourceId, SourceSnapshot, SourceStore};
use bray_syntax::{SourceUnitSyntax, SyntaxTree};

use super::state::Parser;

/// Syntax tree plus diagnostics for a source store.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyntaxTreeResult {
    syntax_tree: SyntaxTree,
    diagnostics: DiagnosticBag,
}

impl SyntaxTreeResult {
    pub(crate) fn new(syntax_tree: SyntaxTree, diagnostics: DiagnosticBag) -> Self {
        Self {
            syntax_tree,
            diagnostics,
        }
    }

    /// Builds a syntax tree result from source-unit syntax results.
    ///
    /// The iterator order becomes the source-unit order in the syntax tree.
    pub fn from_source_unit_results(
        results: impl IntoIterator<Item = SourceUnitSyntaxResult>,
    ) -> Self {
        let mut source_units = Vec::new();
        let mut diagnostic_bags = Vec::new();

        for result in results {
            let (_source_id, source_unit, diagnostics) = result.into_parts();

            source_units.push(source_unit);
            diagnostic_bags.push(diagnostics);
        }

        Self::new(
            SyntaxTree::compilation_unit(source_units),
            DiagnosticBag::merged_all(&diagnostic_bags),
        )
    }

    /// Returns the immutable syntax tree.
    pub const fn syntax_tree(&self) -> &SyntaxTree {
        &self.syntax_tree
    }

    /// Returns diagnostics produced while building syntax.
    pub const fn diagnostics(&self) -> &DiagnosticBag {
        &self.diagnostics
    }

    /// Consumes the result and returns its tree plus diagnostics.
    pub fn into_parts(self) -> (SyntaxTree, DiagnosticBag) {
        (self.syntax_tree, self.diagnostics)
    }
}

/// Parses all source snapshots in source ID order into one compilation unit.
pub fn parse_compilation_unit(sources: &SourceStore) -> SyntaxTreeResult {
    SyntaxTreeResult::from_source_unit_results(sources.iter().map(parse_source_unit))
}

/// Source-unit syntax plus diagnostics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceUnitSyntaxResult {
    source_id: SourceId,
    source_unit: SourceUnitSyntax,
    diagnostics: DiagnosticBag,
}

impl SourceUnitSyntaxResult {
    pub(crate) const fn new(
        source_id: SourceId,
        source_unit: SourceUnitSyntax,
        diagnostics: DiagnosticBag,
    ) -> Self {
        Self {
            source_id,
            source_unit,
            diagnostics,
        }
    }

    /// Returns the source ID for this syntax result.
    pub const fn source_id(&self) -> SourceId {
        self.source_id
    }

    /// Returns the source unit syntax node.
    pub const fn source_unit(&self) -> &SourceUnitSyntax {
        &self.source_unit
    }

    /// Returns diagnostics produced while building this source unit.
    pub const fn diagnostics(&self) -> &DiagnosticBag {
        &self.diagnostics
    }

    /// Consumes the result and returns its source ID, syntax node, and diagnostics.
    pub fn into_parts(self) -> (SourceId, SourceUnitSyntax, DiagnosticBag) {
        (self.source_id, self.source_unit, self.diagnostics)
    }
}

/// Parses one source snapshot into one source-unit syntax node.
pub fn parse_source_unit(snapshot: &SourceSnapshot) -> SourceUnitSyntaxResult {
    let source_id = snapshot.source_id();

    // Parser owns a snapshot handle; cloning shares immutable source text.
    let mut parser = Parser::new(snapshot.clone());

    let source_unit = parser.parse_source_unit();
    let diagnostics = parser.finish();

    SourceUnitSyntaxResult::new(source_id, source_unit, diagnostics)
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticKind;
    use bray_source::{SourceId, TextRange, TextSize};
    use bray_syntax::{SyntaxKind, SyntaxText};
    use bray_testing::test_source_store as source_store;

    use super::{parse_compilation_unit, parse_source_unit};
    use crate::test_support::{diagnostic_kinds, token_kinds};

    #[test]
    fn parser_builds_one_compilation_unit_root() {
        let sources = source_store(["module main;\n"]);
        let result = parse_compilation_unit(&sources);
        let root = result.syntax_tree().root();

        assert_eq!(root.kind(), SyntaxKind::CompilationUnit);
        assert_eq!(root.source_units().len(), 1);
        assert_eq!(root.full_range(), TextRange::EMPTY);
    }

    #[test]
    fn parser_creates_source_units_for_every_snapshot_in_source_id_order() {
        let sources = source_store(["module first;", "module second;", "module third;"]);
        let result = parse_compilation_unit(&sources);
        let source_units = result.syntax_tree().root().source_units();

        assert_eq!(source_units.len(), 3);
        assert_eq!(source_units[0].full_text(), "module first;");
        assert_eq!(source_units[1].full_text(), "module second;");
        assert_eq!(source_units[2].full_text(), "module third;");
    }

    #[test]
    fn parser_exposes_source_unit_syntax_results_as_the_parse_primitive() {
        let sources = source_store(["first", "$"]);

        let source = match sources.get(SourceId::new(1)) {
            Some(source) => source,
            None => panic!("second source should exist"),
        };

        let result = parse_source_unit(source);

        assert_eq!(result.source_id(), SourceId::new(1));
        assert_eq!(result.source_unit().full_text(), "$");

        assert_eq!(
            diagnostic_kinds(result.diagnostics()),
            [
                DiagnosticKind::LexicalInvalidCharacter,
                DiagnosticKind::SyntaxUnexpectedEof,
                DiagnosticKind::SyntaxUnexpectedEof,
                DiagnosticKind::SyntaxUnexpectedEof
            ]
        );
    }

    #[test]
    fn source_unit_full_spans_cover_their_source_text() {
        let sources = source_store(["aé\n", ""]);
        let result = parse_compilation_unit(&sources);
        let source_units = result.syntax_tree().root().source_units();

        assert_eq!(
            source_units[0].full_range(),
            TextRange::new(TextSize::ZERO, TextSize::new(4))
        );

        assert_eq!(source_units[1].full_range(), TextRange::EMPTY);
    }

    #[test]
    fn source_unit_tokens_include_eof_and_remain_reachable() {
        let sources = source_store(["module main;"]);
        let result = parse_compilation_unit(&sources);
        let source_unit = &result.syntax_tree().root().source_units()[0];

        assert_eq!(
            token_kinds(source_unit.tokens()),
            [
                SyntaxKind::ModuleKeyword,
                SyntaxKind::IdentifierToken,
                SyntaxKind::SemicolonToken,
                SyntaxKind::EndOfFileToken
            ]
        );

        assert_eq!(
            source_unit.tokens().last().map(|token| token.kind()),
            Some(SyntaxKind::EndOfFileToken)
        );
    }

    #[test]
    fn parser_preserves_exact_source_reconstruction_with_trivia() {
        let text = "  module // hi\r\n/** docs */\nmain;\n// final\n";
        let sources = source_store([text]);
        let result = parse_compilation_unit(&sources);
        let source_unit = &result.syntax_tree().root().source_units()[0];

        assert_eq!(source_unit.full_text(), text);
        assert_eq!(result.syntax_tree().full_text(), text);
    }

    #[test]
    fn parser_returns_no_diagnostics_for_lexically_valid_sources() {
        let sources = source_store(["module main;", "module extra {}"]);
        let result = parse_compilation_unit(&sources);

        assert!(result.diagnostics().is_empty());
    }
}
