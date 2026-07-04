use bray_diagnostics::DiagnosticBag;
use bray_source::{SourceSnapshot, SourceStore};
use bray_syntax::{SourceUnitSyntax, SyntaxTree};

use crate::lexer::lex_source_unit;

/// Result of parsing a source store into an immutable syntax tree.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParseResult {
    syntax_tree: SyntaxTree,
    diagnostics: DiagnosticBag,
}

impl ParseResult {
    pub(crate) fn new(syntax_tree: SyntaxTree, diagnostics: DiagnosticBag) -> Self {
        Self {
            syntax_tree,
            diagnostics,
        }
    }

    /// Returns the parsed immutable syntax tree.
    pub const fn syntax_tree(&self) -> &SyntaxTree {
        &self.syntax_tree
    }

    /// Returns diagnostics produced while parsing.
    pub const fn diagnostics(&self) -> &DiagnosticBag {
        &self.diagnostics
    }

    /// Consumes the result and returns its tree plus diagnostics.
    pub fn into_parts(self) -> (SyntaxTree, DiagnosticBag) {
        (self.syntax_tree, self.diagnostics)
    }
}

/// Parses all source snapshots in source ID order into one compilation unit.
pub fn parse_compilation_unit(sources: &SourceStore) -> ParseResult {
    let mut source_units = Vec::with_capacity(sources.len());
    let mut diagnostic_bags = Vec::with_capacity(sources.len());

    for snapshot in sources {
        let parsed = parse_source_unit(snapshot);

        source_units.push(parsed.source_unit);
        diagnostic_bags.push(parsed.diagnostics);
    }

    ParseResult::new(
        SyntaxTree::compilation_unit(source_units),
        DiagnosticBag::merged_all(&diagnostic_bags),
    )
}

#[derive(Debug)]
struct ParsedSourceUnit {
    source_unit: SourceUnitSyntax,
    diagnostics: DiagnosticBag,
}

fn parse_source_unit(snapshot: &SourceSnapshot) -> ParsedSourceUnit {
    let lex_result = lex_source_unit(snapshot);
    let (tokens, diagnostics) = lex_result.into_parts();

    ParsedSourceUnit {
        source_unit: SourceUnitSyntax::builder(snapshot.clone())
            .tokens(tokens)
            .build(),
        diagnostics,
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticKind;
    use bray_source::{TextRange, TextSize};
    use bray_syntax::{SyntaxKind, SyntaxText, SyntaxToken};
    use bray_testing::test_source_store as source_store;

    use super::{ParseResult, parse_compilation_unit};

    #[test]
    fn parser_builds_one_compilation_unit_root() {
        let sources = source_store(["module main\n"]);
        let result = parse_compilation_unit(&sources);
        let root = result.syntax_tree().root();

        assert_eq!(root.kind(), SyntaxKind::CompilationUnit);
        assert_eq!(root.source_units().len(), 1);
        assert_eq!(root.full_range(), TextRange::EMPTY);
    }

    #[test]
    fn parser_creates_source_units_for_every_snapshot_in_source_id_order() {
        let sources = source_store(["first", "second", "third"]);
        let result = parse_compilation_unit(&sources);
        let source_units = result.syntax_tree().root().source_units();

        assert_eq!(source_units.len(), 3);
        assert_eq!(source_units[0].full_text(), "first");
        assert_eq!(source_units[1].full_text(), "second");
        assert_eq!(source_units[2].full_text(), "third");
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
        let sources = source_store(["func main"]);
        let result = parse_compilation_unit(&sources);
        let source_unit = &result.syntax_tree().root().source_units()[0];

        assert_eq!(
            token_kinds(source_unit.tokens()),
            [
                SyntaxKind::FuncKeyword,
                SyntaxKind::IdentifierToken,
                SyntaxKind::EndOfFileToken
            ]
        );

        assert_eq!(
            source_unit.tokens().last().map(SyntaxToken::kind),
            Some(SyntaxKind::EndOfFileToken)
        );
    }

    #[test]
    fn parser_preserves_exact_source_reconstruction_with_trivia() {
        let text = "  func // hi\r\n/** docs */\nmain\n// final\n";
        let sources = source_store([text]);
        let result = parse_compilation_unit(&sources);
        let source_unit = &result.syntax_tree().root().source_units()[0];

        assert_eq!(source_unit.full_text(), text);
        assert_eq!(result.syntax_tree().full_text(), text);
    }

    #[test]
    fn parser_returns_lexical_diagnostics_without_grammar_diagnostics() {
        let sources = source_store(["$"]);
        let result = parse_compilation_unit(&sources);

        assert_eq!(
            diagnostic_kinds(&result),
            [DiagnosticKind::LexicalInvalidCharacter]
        );
    }

    #[test]
    fn parser_returns_no_diagnostics_for_lexically_valid_sources() {
        let sources = source_store(["func main", "1 2 3"]);
        let result = parse_compilation_unit(&sources);

        assert!(result.diagnostics().is_empty());
    }

    fn token_kinds<'syntax>(
        tokens: impl IntoIterator<Item = &'syntax SyntaxToken>,
    ) -> Vec<SyntaxKind> {
        tokens.into_iter().map(SyntaxToken::kind).collect()
    }

    fn diagnostic_kinds(result: &ParseResult) -> Vec<DiagnosticKind> {
        result
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.kind())
            .collect()
    }
}
