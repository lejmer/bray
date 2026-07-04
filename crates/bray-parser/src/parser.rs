use bray_diagnostics::DiagnosticBag;
use bray_source::{SourceSnapshot, SourceStore};
use bray_syntax::{SourceUnitSyntax, SyntaxToken, SyntaxTree};

use crate::lexer::LexerTokenSource;

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
    ///
    /// The initial parser does not emit grammar diagnostics. This bag contains
    /// lexical diagnostics collected from the token sources consumed by the
    /// parser.
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
    // LexerTokenSource owns its snapshot; cloning shares immutable source text.
    let mut token_source = LexerTokenSource::new(snapshot.clone());

    let tokens = drain_tokens(&mut token_source);

    ParsedSourceUnit {
        source_unit: SourceUnitSyntax::new(snapshot.clone(), tokens),
        diagnostics: token_source.into_diagnostics(),
    }
}

fn drain_tokens(token_source: &mut LexerTokenSource) -> Vec<SyntaxToken> {
    let mut tokens = Vec::new();

    loop {
        let token = token_source.consume();
        let reached_end = token.is_end_of_file();

        tokens.push(token);

        if reached_end {
            return tokens;
        }
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticKind;
    use bray_source::{
        SourceIdentity, SourceOrigin, SourceStore, SourceVersion, TextRange, TextSize,
    };
    use bray_syntax::{SyntaxKind, SyntaxText, SyntaxToken};

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

    fn source_store<const N: usize>(texts: [&str; N]) -> SourceStore {
        let mut store = SourceStore::with_capacity(texts.len());

        for (index, text) in texts.into_iter().enumerate() {
            insert_source(&mut store, index, text);
        }

        store
    }

    fn insert_source(store: &mut SourceStore, index: usize, text: &str) {
        let identity = SourceIdentity::new(raw_source_identity(index));
        let origin = SourceOrigin::virtual_source(format!("parser-test-{index}"));

        match store.insert(identity, origin, SourceVersion::new(0), text) {
            Ok(_) => {}
            Err(error) => panic!("test source should insert successfully: {error:?}"),
        }
    }

    fn raw_source_identity(index: usize) -> u32 {
        match u32::try_from(index) {
            Ok(raw) => raw,
            Err(error) => panic!("test source index should fit in u32: {error:?}"),
        }
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
