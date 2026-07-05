use bray_diagnostics::DiagnosticBag;
use bray_source::{SourceId, SourceSnapshot, SourceStore};
use bray_syntax::{
    IdentifierListItemSyntax, IdentifierListSyntax, IdentifierListSyntaxBuilder, SourceUnitSyntax,
    SourceUnitSyntaxBuilder, SyntaxKind, SyntaxToken, SyntaxTree,
};

use crate::cursor::{ParserCursor, ParserCursorCheckpoint, RecoverySet};
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

    /// Runs a syntax-only lookahead decision on a forked parser.
    fn scan_ahead(&mut self, scan: impl FnOnce(&mut Parser) -> bool) -> bool {
        let checkpoint = self.cursor.checkpoint();

        let mut fork = self.fork_from_checkpoint(&checkpoint);

        let decision = scan(&mut fork);

        self.cursor.absorb_lexical_diagnostics_from(&fork.cursor);

        decision
    }

    fn fork_from_checkpoint(&self, checkpoint: &ParserCursorCheckpoint) -> Self {
        Self {
            // Forked parsers share immutable source text with the main parser.
            snapshot: self.snapshot.clone(),
            cursor: checkpoint.fork(),
        }
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

    fn at_any(&mut self, kinds: &[SyntaxKind]) -> bool {
        let kind = self.peek().kind();

        kinds.contains(&kind)
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

    fn recover_until(
        &mut self,
        builder: &mut impl RecoverySyntaxSink,
        stop_kinds: &[SyntaxKind],
    ) -> bool {
        let skipped_tokens = self.cursor.skip_until(RecoverySet::new(stop_kinds));
        let skipped_any = !skipped_tokens.is_empty();

        builder.push_skipped_tokens(skipped_tokens);

        skipped_any
    }

    fn recover_current_token(&mut self, builder: &mut impl RecoverySyntaxSink) {
        let Some(skipped_token) = self.cursor.skip_one() else {
            return;
        };

        builder.push_skipped_tokens(vec![skipped_token]);
    }

    fn at_list_end(&mut self, terminators: &[SyntaxKind]) -> bool {
        self.at_any(terminators) || self.at(SyntaxKind::EndOfFileToken)
    }

    fn at_list_boundary(&mut self, spec: SeparatedListSpec<'_>) -> bool {
        self.at(spec.separator_kind) || self.at_list_end(spec.terminators)
    }

    fn should_recover_invalid_token(&mut self) -> bool {
        self.scan_ahead(|scan| scan.at(SyntaxKind::InvalidToken))
    }

    fn should_parse_identifier_list(&mut self) -> bool {
        self.scan_ahead(|scan| {
            if !scan.at(SyntaxKind::IdentifierToken) {
                return false;
            }

            scan.consume();

            scan.at(SyntaxKind::CommaToken)
        })
    }

    fn parse_identifier_list(&mut self, terminators: &[SyntaxKind]) -> IdentifierListSyntax {
        let start = self.peek().full_range().start();
        let recovery_kinds = identifier_list_recovery_kinds(terminators);

        let spec = SeparatedListSpec {
            separator_kind: SyntaxKind::CommaToken,
            terminators,
            recovery_kinds: &recovery_kinds,
            allow_trailing_separator: true,
        };

        let mut builder = IdentifierListSyntax::builder(self.snapshot.clone(), start);

        self.parse_separated_list(&mut builder, spec, Parser::parse_identifier_list_item);

        builder.build()
    }

    fn parse_identifier_list_item(&mut self) -> IdentifierListItemSyntax {
        let mut builder = IdentifierListItemSyntax::builder(self.snapshot.clone());

        builder.push_identifier_token(self.expect(SyntaxKind::IdentifierToken));

        builder.build()
    }

    fn parse_separated_list<Item>(
        &mut self,
        builder: &mut impl SeparatedListSyntaxSink<Item>,
        spec: SeparatedListSpec<'_>,
        mut parse_item: impl FnMut(&mut Parser) -> Item,
    ) {
        let mut position = SeparatedListPosition::Item { allow_end: true };

        loop {
            match position {
                SeparatedListPosition::Item { allow_end }
                    if allow_end && self.at_list_end(spec.terminators) =>
                {
                    return;
                }
                SeparatedListPosition::Separator if self.at_list_end(spec.terminators) => {
                    return;
                }
                _ => {}
            }

            match position {
                SeparatedListPosition::Item { .. } => {
                    self.parse_separated_list_item(builder, spec, &mut parse_item);
                    position = SeparatedListPosition::Separator;
                }
                SeparatedListPosition::Separator => {
                    if let Some(separator) = self.consume_if(spec.separator_kind) {
                        builder.push_separator(separator);
                        position = SeparatedListPosition::Item {
                            allow_end: spec.allow_trailing_separator,
                        };
                        continue;
                    }

                    builder.push_separator(self.expect(spec.separator_kind));
                    position = SeparatedListPosition::Item { allow_end: false };
                }
            }
        }
    }

    fn parse_separated_list_item<Item>(
        &mut self,
        builder: &mut impl SeparatedListSyntaxSink<Item>,
        spec: SeparatedListSpec<'_>,
        parse_item: &mut impl FnMut(&mut Parser) -> Item,
    ) {
        let item_start = self.peek().start();
        let item = parse_item(self);

        builder.push_item(item);

        if self.peek().start() != item_start || self.at_list_boundary(spec) {
            return;
        }

        if self.recover_until(builder, spec.recovery_kinds) || self.at_list_boundary(spec) {
            return;
        }

        self.recover_current_token(builder);
    }

    fn parse_placeholder_source_unit_tokens(&mut self, builder: &mut SourceUnitSyntaxBuilder) {
        loop {
            if self.at(SyntaxKind::EndOfFileToken) {
                builder.push_token(self.expect(SyntaxKind::EndOfFileToken));
                return;
            }

            if self.should_recover_invalid_token() {
                self.recover_until(builder, &[SyntaxKind::EndOfFileToken]);
                continue;
            }

            if self.should_parse_identifier_list() {
                let list = self.parse_identifier_list(&[SyntaxKind::EndOfFileToken]);

                builder.push_identifier_list(list);
                continue;
            }

            builder.push_token(self.consume());
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SeparatedListSpec<'kinds> {
    separator_kind: SyntaxKind,
    terminators: &'kinds [SyntaxKind],
    recovery_kinds: &'kinds [SyntaxKind],
    allow_trailing_separator: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SeparatedListPosition {
    Item { allow_end: bool },
    Separator,
}

trait RecoverySyntaxSink {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>);
}

trait SeparatedListSyntaxSink<Item>: RecoverySyntaxSink {
    fn push_item(&mut self, item: Item);

    fn push_separator(&mut self, separator: SyntaxToken);
}

impl RecoverySyntaxSink for SourceUnitSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        SourceUnitSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for IdentifierListSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        IdentifierListSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl SeparatedListSyntaxSink<IdentifierListItemSyntax> for IdentifierListSyntaxBuilder {
    fn push_item(&mut self, item: IdentifierListItemSyntax) {
        IdentifierListSyntaxBuilder::push_item(self, item);
    }

    fn push_separator(&mut self, separator: SyntaxToken) {
        IdentifierListSyntaxBuilder::push_separator_token(self, separator);
    }
}

fn identifier_list_recovery_kinds(terminators: &[SyntaxKind]) -> Vec<SyntaxKind> {
    let mut recovery_kinds = Vec::with_capacity(terminators.len() + 2);

    recovery_kinds.push(SyntaxKind::IdentifierToken);
    recovery_kinds.push(SyntaxKind::CommaToken);
    recovery_kinds.extend_from_slice(terminators);

    recovery_kinds
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
    fn parser_separated_list_helper_parses_valid_identifier_lists() {
        let sources = source_store(["a,b,"]);
        let snapshot = source(&sources, 0);
        let mut parser = Parser::new(snapshot.clone());

        let list = parser.parse_identifier_list(&[SyntaxKind::EndOfFileToken]);
        let diagnostics = parser.finish();

        assert_eq!(list.full_text(), "a,b,");
        assert_eq!(list.items().count(), 2);

        assert_eq!(
            token_kinds(list.tokens()),
            [
                SyntaxKind::IdentifierToken,
                SyntaxKind::CommaToken,
                SyntaxKind::IdentifierToken,
                SyntaxKind::CommaToken
            ]
        );

        assert_eq!(list.separator_tokens().count(), 2);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn parser_separated_list_helper_represents_missing_separators() {
        let sources = source_store(["a b"]);
        let snapshot = source(&sources, 0);
        let mut parser = Parser::new(snapshot.clone());

        let list = parser.parse_identifier_list(&[SyntaxKind::EndOfFileToken]);
        let diagnostics = parser.finish();
        let separators = list.separator_tokens().collect::<Vec<_>>();

        let [separator] = separators.as_slice() else {
            panic!("expected one separator token: {separators:?}");
        };

        assert_eq!(list.full_text(), "a b");
        assert_eq!(list.items().count(), 2);
        assert_eq!(separator.kind(), SyntaxKind::CommaToken);
        assert!(separator.is_missing());

        assert_eq!(
            diagnostic_kinds(&diagnostics),
            [DiagnosticKind::SyntaxExpectedToken]
        );
    }

    #[test]
    fn parser_separated_list_helper_recovers_bad_tokens_without_losing_later_items() {
        let sources = source_store(["a,$,b"]);
        let snapshot = source(&sources, 0);
        let mut parser = Parser::new(snapshot.clone());

        let list = parser.parse_identifier_list(&[SyntaxKind::EndOfFileToken]);
        let diagnostics = parser.finish();
        let items = list.items().collect::<Vec<_>>();
        let skipped_syntax = list.skipped_syntax().collect::<Vec<_>>();

        let [first, recovered, last] = items.as_slice() else {
            panic!("expected three list items: {items:?}");
        };

        let [skipped] = skipped_syntax.as_slice() else {
            panic!("expected one skipped-syntax node: {skipped_syntax:?}");
        };

        assert_eq!(list.full_text(), "a,$,b");
        assert_eq!(first.identifier_token().text(snapshot.text()), Some("a"));
        assert!(recovered.identifier_token().is_missing());
        assert_eq!(last.identifier_token().text(snapshot.text()), Some("b"));
        assert_eq!(skipped.full_text(), "$");

        assert_eq!(
            diagnostic_kinds(&diagnostics),
            [
                DiagnosticKind::LexicalInvalidCharacter,
                DiagnosticKind::SyntaxExpectedToken,
                DiagnosticKind::SyntaxSkippedSyntax
            ]
        );
    }

    #[test]
    fn parser_source_units_attach_identifier_lists_for_comma_sequences() {
        let sources = source_store(["a,b,"]);
        let result = parse_compilation_unit(&sources);
        let source_unit = &result.syntax_tree().root().source_units()[0];
        let identifier_lists = source_unit.identifier_lists().collect::<Vec<_>>();

        let [identifier_list] = identifier_lists.as_slice() else {
            panic!("expected one identifier-list child: {identifier_lists:?}");
        };

        assert_eq!(source_unit.full_text(), "a,b,");
        assert_eq!(identifier_list.full_text(), "a,b,");
        assert_eq!(identifier_list.items().count(), 2);

        assert_eq!(
            token_kinds(source_unit.tokens()),
            [
                SyntaxKind::IdentifierToken,
                SyntaxKind::CommaToken,
                SyntaxKind::IdentifierToken,
                SyntaxKind::CommaToken,
                SyntaxKind::EndOfFileToken
            ]
        );

        assert!(result.diagnostics().is_empty());
    }

    #[test]
    fn parser_scan_ahead_leaves_main_cursor_position_unchanged() {
        let sources = source_store(["func main"]);

        let snapshot = match sources.get(bray_source::SourceId::new(0)) {
            Some(snapshot) => snapshot,
            None => panic!("source should exist"),
        };

        let mut parser = Parser::new(snapshot.clone());

        let saw_identifier_after_func = parser.scan_ahead(|scan| {
            assert_eq!(scan.consume().kind(), SyntaxKind::FuncKeyword);

            scan.at(SyntaxKind::IdentifierToken)
        });

        assert!(saw_identifier_after_func);
        assert_eq!(parser.peek().kind(), SyntaxKind::FuncKeyword);
    }

    #[test]
    fn parser_scan_ahead_discards_recovery_nodes_and_syntax_diagnostics() {
        let sources = source_store(["main;"]);

        let snapshot = match sources.get(bray_source::SourceId::new(0)) {
            Some(snapshot) => snapshot,
            None => panic!("source should exist"),
        };

        let mut parser = Parser::new(snapshot.clone());

        let built_skipped_syntax_in_scan = parser.scan_ahead(|scan| {
            let mut builder = SourceUnitSyntax::builder(snapshot.clone());

            scan.recover_until(&mut builder, &[SyntaxKind::SemicolonToken]);

            builder.push_token(scan.expect(SyntaxKind::SemicolonToken));
            builder.push_token(scan.expect(SyntaxKind::EndOfFileToken));

            let speculative_unit = builder.build();

            speculative_unit.skipped_syntax().next().is_some()
        });

        assert!(built_skipped_syntax_in_scan);

        let source_unit = parser.parse_source_unit();
        let diagnostics = parser.finish();

        assert_eq!(source_unit.full_text(), "main;");
        assert!(source_unit.skipped_syntax().next().is_none());
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn parser_scan_ahead_preserves_demanded_lexical_diagnostics() {
        let sources = source_store(["$"]);

        let snapshot = match sources.get(bray_source::SourceId::new(0)) {
            Some(snapshot) => snapshot,
            None => panic!("source should exist"),
        };

        let mut parser = Parser::new(snapshot.clone());

        let saw_invalid_token = parser.scan_ahead(|scan| scan.at(SyntaxKind::InvalidToken));

        assert!(saw_invalid_token);

        let diagnostics = parser.finish();

        assert_eq!(
            diagnostic_kinds(&diagnostics),
            [DiagnosticKind::LexicalInvalidCharacter]
        );
    }

    #[test]
    fn parser_scan_ahead_discards_parser_diagnostics_from_abandoned_scan() {
        let sources = source_store(["main"]);

        let snapshot = match sources.get(bray_source::SourceId::new(0)) {
            Some(snapshot) => snapshot,
            None => panic!("source should exist"),
        };

        let mut parser = Parser::new(snapshot.clone());

        let inserted_missing_token =
            parser.scan_ahead(|scan| scan.expect(SyntaxKind::FuncKeyword).is_missing());

        assert!(inserted_missing_token);
        assert_eq!(parser.peek().kind(), SyntaxKind::IdentifierToken);

        let diagnostics = parser.finish();

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn parser_returns_no_diagnostics_for_lexically_valid_sources() {
        let sources = source_store(["func main", "1 2 3"]);
        let result = parse_compilation_unit(&sources);

        assert!(result.diagnostics().is_empty());
    }

    fn source(sources: &bray_source::SourceStore, index: u32) -> bray_source::SourceSnapshot {
        match sources.get(bray_source::SourceId::new(index)) {
            Some(snapshot) => snapshot.clone(),
            None => panic!("source should exist"),
        }
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
