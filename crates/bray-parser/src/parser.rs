use bray_diagnostics::DiagnosticBag;
use bray_source::{SourceId, SourceSnapshot, SourceStore};
use bray_syntax::{SourceUnitSyntax, SourceUnitSyntaxBuilder, SyntaxKind, SyntaxToken, SyntaxTree};

use crate::cursor::{ParserCursor, RecoverySet};
use crate::lexer::LexerTokenSource;

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

struct Parser {
    snapshot: SourceSnapshot,
    cursor: ParserCursor,
}

impl Parser {
    fn new(snapshot: SourceSnapshot) -> Self {
        // LexerTokenSource owns a snapshot handle; cloning shares immutable source text.
        let token_source = LexerTokenSource::new(snapshot.clone());

        Self {
            snapshot,
            cursor: ParserCursor::new(token_source),
        }
    }

    fn parse_source_unit(&mut self) -> SourceUnitSyntax {
        // Source syntax keeps a cheap handle to immutable source text.
        let mut builder = SourceUnitSyntax::builder(self.snapshot.clone());

        self.parse_placeholder_source_unit_tokens(&mut builder);

        builder.build()
    }

    fn finish(self) -> DiagnosticBag {
        self.cursor.finish()
    }

    fn lookahead(&mut self, distance: usize) -> SyntaxToken {
        self.cursor.lookahead(distance)
    }

    fn peek(&mut self) -> SyntaxToken {
        self.lookahead(0)
    }

    fn at(&mut self, kind: SyntaxKind) -> bool {
        self.peek().kind() == kind
    }

    fn consume_if(&mut self, kind: SyntaxKind) -> Option<SyntaxToken> {
        if !self.at(kind) {
            return None;
        }

        Some(self.cursor.consume())
    }

    fn consume(&mut self) -> SyntaxToken {
        let kind = self.peek().kind();

        match self.consume_if(kind) {
            Some(token) => token,
            None => panic!("parser token changed between peek and consume"),
        }
    }

    fn expect(&mut self, kind: SyntaxKind) -> SyntaxToken {
        self.cursor.expect(kind)
    }

    fn recover_until(&mut self, builder: &mut impl RecoverySyntaxSink, stop_kinds: &[SyntaxKind]) {
        let skipped_tokens = self.cursor.skip_until(RecoverySet::new(stop_kinds));

        builder.push_skipped_tokens(skipped_tokens);
    }

    fn parse_placeholder_source_unit_tokens(&mut self, builder: &mut SourceUnitSyntaxBuilder) {
        loop {
            if self.at(SyntaxKind::EndOfFileToken) {
                builder.push_token(self.expect(SyntaxKind::EndOfFileToken));
                return;
            }

            if self.at(SyntaxKind::InvalidToken) {
                self.recover_until(builder, &[SyntaxKind::EndOfFileToken]);
                continue;
            }

            builder.push_token(self.consume());
        }
    }
}

trait RecoverySyntaxSink {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>);
}

impl RecoverySyntaxSink for SourceUnitSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        SourceUnitSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{DiagnosticBag, DiagnosticKind};
    use bray_source::{TextRange, TextSize};
    use bray_syntax::{SourceUnitSyntax, SyntaxKind, SyntaxText, SyntaxToken};
    use bray_testing::test_source_store as source_store;

    use super::{Parser, SyntaxTreeResult, parse_compilation_unit, parse_source_unit};

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
    fn parser_exposes_source_unit_syntax_results_as_the_parse_primitive() {
        let sources = source_store(["first", "$"]);

        let source = match sources.get(bray_source::SourceId::new(1)) {
            Some(source) => source,
            None => panic!("second source should exist"),
        };

        let result = parse_source_unit(source);

        assert_eq!(result.source_id(), bray_source::SourceId::new(1));
        assert_eq!(result.source_unit().full_text(), "$");

        assert_eq!(
            diagnostic_kinds(result.diagnostics()),
            [
                DiagnosticKind::LexicalInvalidCharacter,
                DiagnosticKind::SyntaxSkippedSyntax
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
            source_unit.tokens().last().map(|token| token.kind()),
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
    fn parser_recovers_invalid_tokens_as_skipped_syntax() {
        let sources = source_store(["$"]);
        let result = parse_compilation_unit(&sources);
        let source_unit = &result.syntax_tree().root().source_units()[0];
        let skipped_syntax = source_unit.skipped_syntax().collect::<Vec<_>>();

        assert_eq!(
            parse_diagnostic_kinds(&result),
            [
                DiagnosticKind::LexicalInvalidCharacter,
                DiagnosticKind::SyntaxSkippedSyntax
            ]
        );

        let [skipped] = skipped_syntax.as_slice() else {
            panic!("expected one skipped-syntax node: {skipped_syntax:?}");
        };

        assert_eq!(skipped.full_text(), "$");
    }

    #[test]
    fn parser_recovery_helper_attaches_skipped_syntax_before_stop_token() {
        let sources = source_store(["main;"]);

        let snapshot = match sources.get(bray_source::SourceId::new(0)) {
            Some(snapshot) => snapshot,
            None => panic!("source should exist"),
        };

        let mut parser = Parser::new(snapshot.clone());
        let mut builder = SourceUnitSyntax::builder(snapshot.clone());

        parser.recover_until(&mut builder, &[SyntaxKind::SemicolonToken]);

        builder.push_token(parser.expect(SyntaxKind::SemicolonToken));
        builder.push_token(parser.expect(SyntaxKind::EndOfFileToken));

        let source_unit = builder.build();
        let diagnostics = parser.finish();
        let skipped_syntax = source_unit.skipped_syntax().collect::<Vec<_>>();

        let [skipped] = skipped_syntax.as_slice() else {
            panic!("expected one skipped-syntax node: {skipped_syntax:?}");
        };

        assert_eq!(skipped.full_text(), "main");
        assert_eq!(source_unit.full_text(), "main;");

        assert_eq!(
            diagnostic_kinds(&diagnostics),
            [DiagnosticKind::SyntaxSkippedSyntax]
        );
    }

    #[test]
    fn parser_returns_no_diagnostics_for_lexically_valid_sources() {
        let sources = source_store(["func main", "1 2 3"]);
        let result = parse_compilation_unit(&sources);

        assert!(result.diagnostics().is_empty());
    }

    fn token_kinds(tokens: impl IntoIterator<Item = SyntaxToken>) -> Vec<SyntaxKind> {
        tokens.into_iter().map(|token| token.kind()).collect()
    }

    fn parse_diagnostic_kinds(result: &SyntaxTreeResult) -> Vec<DiagnosticKind> {
        diagnostic_kinds(result.diagnostics())
    }

    fn diagnostic_kinds(diagnostics: &DiagnosticBag) -> Vec<DiagnosticKind> {
        diagnostics
            .iter()
            .map(|diagnostic| diagnostic.kind())
            .collect()
    }
}
