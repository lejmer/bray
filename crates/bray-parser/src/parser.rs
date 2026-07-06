use bray_diagnostics::DiagnosticBag;
use bray_source::{SourceId, SourceSnapshot, SourceStore};
use bray_syntax::{
    BlockModuleDeclarationSyntax, BlockModuleDeclarationSyntaxBuilder, IdentifierListItemSyntax,
    IdentifierListSyntax, IdentifierListSyntaxBuilder, ModuleBodySyntax, ModuleBodySyntaxBuilder,
    ModuleModifiersSyntax, PathSyntax, SourceUnitModuleDeclarationSyntax,
    SourceUnitModuleDeclarationSyntaxBuilder, SourceUnitSyntax, SourceUnitSyntaxBuilder,
    SyntaxKind, SyntaxToken, SyntaxTree,
};

use crate::cursor::{ParserCursor, ParserCursorCheckpoint, RecoverySet};
use crate::lexer::LexerTokenSource;

const MODULE_HEADER_START_KINDS: [SyntaxKind; 4] = [
    SyntaxKind::TrustedKeyword,
    SyntaxKind::PublicKeyword,
    SyntaxKind::InternalKeyword,
    SyntaxKind::ModuleKeyword,
];

const MODULE_DECLARATION_START_KINDS: [SyntaxKind; 5] = [
    SyntaxKind::AtToken,
    SyntaxKind::TrustedKeyword,
    SyntaxKind::PublicKeyword,
    SyntaxKind::InternalKeyword,
    SyntaxKind::ModuleKeyword,
];

const SOURCE_UNIT_ITEM_TERMINATORS: [SyntaxKind; 1] = [SyntaxKind::EndOfFileToken];
const TOP_LEVEL_BLOCK_MODULE_RECOVERY_KINDS: [SyntaxKind; 6] = [
    SyntaxKind::AtToken,
    SyntaxKind::TrustedKeyword,
    SyntaxKind::PublicKeyword,
    SyntaxKind::InternalKeyword,
    SyntaxKind::ModuleKeyword,
    SyntaxKind::EndOfFileToken,
];

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
        let mut builder = SourceUnitSyntax::builder(self.syntax_source());

        if self.should_parse_block_module_declaration() {
            self.parse_block_module_declarations(&mut builder);
        } else {
            let declaration = self.parse_source_unit_module_declaration();

            builder.push_source_unit_module_declaration(declaration);
            self.parse_module_items(&mut builder, &SOURCE_UNIT_ITEM_TERMINATORS);
        }

        builder.push_token(self.expect(SyntaxKind::EndOfFileToken));

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

    fn syntax_source(&self) -> SourceSnapshot {
        // Syntax nodes share immutable source text by cloning the snapshot handle.
        self.snapshot.clone()
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

    fn parse_source_unit_module_declaration(&mut self) -> SourceUnitModuleDeclarationSyntax {
        let start = self.peek().full_range().start();
        let mut builder = SourceUnitModuleDeclarationSyntax::builder(self.syntax_source(), start);

        self.parse_module_declaration_header(&mut builder);
        self.recover_until(
            &mut builder,
            &[SyntaxKind::SemicolonToken, SyntaxKind::EndOfFileToken],
        );
        builder.push_semicolon_token(self.expect(SyntaxKind::SemicolonToken));

        builder.build()
    }

    fn parse_block_module_declarations(&mut self, builder: &mut SourceUnitSyntaxBuilder) {
        loop {
            if self.at(SyntaxKind::EndOfFileToken) {
                return;
            }

            if self.should_parse_block_module_declaration() {
                let declaration = self.parse_block_module_declaration();

                builder.push_block_module_declaration(declaration);
                continue;
            }

            if self.recover_until(builder, &TOP_LEVEL_BLOCK_MODULE_RECOVERY_KINDS) {
                continue;
            }

            self.recover_current_token(builder);
        }
    }

    fn parse_block_module_declaration(&mut self) -> BlockModuleDeclarationSyntax {
        let start = self.peek().full_range().start();
        let mut builder = BlockModuleDeclarationSyntax::builder(self.syntax_source(), start);

        self.parse_module_declaration_header(&mut builder);
        builder.push_module_body(self.parse_module_body());

        builder.build()
    }

    fn parse_module_declaration_header(&mut self, builder: &mut impl ModuleDeclarationSyntaxSink) {
        self.recover_until(builder, &MODULE_HEADER_START_KINDS);

        builder.push_module_modifiers(self.parse_module_modifiers());
        builder.push_module_keyword(self.expect(SyntaxKind::ModuleKeyword));
        builder.push_module_path(self.parse_module_path());
    }

    fn parse_module_modifiers(&mut self) -> ModuleModifiersSyntax {
        let start = self.peek().full_range().start();
        let mut builder = ModuleModifiersSyntax::builder(self.syntax_source(), start);

        if self.at(SyntaxKind::TrustedKeyword) {
            builder.push_trusted_token(self.parse_trusted_modifier());
        }

        if self.at(SyntaxKind::PublicKeyword) || self.at(SyntaxKind::InternalKeyword) {
            builder.push_visibility_token(self.parse_visibility_modifier());
        }

        builder.build()
    }

    fn parse_trusted_modifier(&mut self) -> SyntaxToken {
        self.expect(SyntaxKind::TrustedKeyword)
    }

    fn parse_visibility_modifier(&mut self) -> SyntaxToken {
        if self.at(SyntaxKind::PublicKeyword) {
            return self.expect(SyntaxKind::PublicKeyword);
        }

        self.expect(SyntaxKind::InternalKeyword)
    }

    fn parse_module_path(&mut self) -> PathSyntax {
        self.parse_path()
    }

    fn parse_module_body(&mut self) -> ModuleBodySyntax {
        let start = self.peek().full_range().start();
        let mut builder = ModuleBodySyntax::builder(self.syntax_source(), start);

        builder.push_open_brace_token(self.expect(SyntaxKind::OpenBraceToken));
        self.recover_until_balanced_close_brace(&mut builder);
        builder.push_close_brace_token(self.expect(SyntaxKind::CloseBraceToken));

        builder.build()
    }

    fn parse_module_items(
        &mut self,
        builder: &mut impl RecoverySyntaxSink,
        terminators: &[SyntaxKind],
    ) {
        while !self.at_any(terminators) && !self.at(SyntaxKind::EndOfFileToken) {
            self.parse_module_item(builder, terminators);
        }
    }

    fn parse_module_item(
        &mut self,
        builder: &mut impl RecoverySyntaxSink,
        terminators: &[SyntaxKind],
    ) {
        let start = self.peek().start();

        if self.recover_until(builder, terminators) || self.peek().start() != start {
            return;
        }

        self.recover_current_token(builder);
    }

    fn parse_path(&mut self) -> PathSyntax {
        let mut builder = PathSyntax::builder(self.syntax_source());

        builder.push_identifier_token(self.parse_identifier());

        while self.at(SyntaxKind::DotToken) {
            builder.push_dot_token(self.expect(SyntaxKind::DotToken));
            builder.push_identifier_token(self.parse_identifier());
        }

        builder.build()
    }

    fn parse_identifier(&mut self) -> SyntaxToken {
        self.expect(SyntaxKind::IdentifierToken)
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

    fn recover_until_balanced_close_brace(&mut self, builder: &mut impl RecoverySyntaxSink) {
        let skipped_tokens = self.cursor.skip_until_balanced_close_brace();

        builder.push_skipped_tokens(skipped_tokens);
    }

    fn at_list_end(&mut self, terminators: &[SyntaxKind]) -> bool {
        self.at_any(terminators) || self.at(SyntaxKind::EndOfFileToken)
    }

    fn at_list_boundary(&mut self, spec: SeparatedListSpec<'_>) -> bool {
        self.at(spec.separator_kind) || self.at_list_end(spec.terminators)
    }

    // TODO(parser): Remove this expectation once production grammar uses separated lists.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "TODO(parser): separated-list parser support is waiting for grammar use"
        )
    )]
    fn parse_identifier_list(&mut self, terminators: &[SyntaxKind]) -> IdentifierListSyntax {
        let start = self.peek().full_range().start();
        let recovery_kinds = identifier_list_recovery_kinds(terminators);

        let spec = SeparatedListSpec {
            separator_kind: SyntaxKind::CommaToken,
            terminators,
            recovery_kinds: &recovery_kinds,
            allow_trailing_separator: true,
        };

        let mut builder = IdentifierListSyntax::builder(self.syntax_source(), start);

        self.parse_separated_list(&mut builder, spec, Parser::parse_identifier_list_item);

        builder.build()
    }

    fn parse_identifier_list_item(&mut self) -> IdentifierListItemSyntax {
        let mut builder = IdentifierListItemSyntax::builder(self.syntax_source());

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

    fn should_parse_block_module_declaration(&mut self) -> bool {
        if !self.at_any(&MODULE_DECLARATION_START_KINDS) {
            return false;
        }

        self.scan_ahead(|scan| {
            scan.skip_module_directives_for_scan();
            scan.consume_module_modifiers_for_scan();

            if !scan.at(SyntaxKind::ModuleKeyword) {
                return false;
            }

            scan.consume();

            if !scan.consume_path_for_scan() {
                return false;
            }

            scan.at(SyntaxKind::OpenBraceToken)
        })
    }

    fn skip_module_directives_for_scan(&mut self) {
        while !self.at_any(&MODULE_HEADER_START_KINDS) && !self.at(SyntaxKind::EndOfFileToken) {
            self.consume();
        }
    }

    fn consume_module_modifiers_for_scan(&mut self) {
        if self.at(SyntaxKind::TrustedKeyword) {
            self.consume();
        }

        if self.at(SyntaxKind::PublicKeyword) || self.at(SyntaxKind::InternalKeyword) {
            self.consume();
        }
    }

    fn consume_path_for_scan(&mut self) -> bool {
        if !self.at(SyntaxKind::IdentifierToken) {
            return false;
        }

        self.consume();

        while self.at(SyntaxKind::DotToken) {
            self.consume();

            if !self.at(SyntaxKind::IdentifierToken) {
                break;
            }

            self.consume();
        }

        true
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

trait ModuleDeclarationSyntaxSink: RecoverySyntaxSink {
    fn push_module_modifiers(&mut self, modifiers: ModuleModifiersSyntax);

    fn push_module_keyword(&mut self, token: SyntaxToken);

    fn push_module_path(&mut self, path: PathSyntax);
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

impl RecoverySyntaxSink for SourceUnitModuleDeclarationSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        SourceUnitModuleDeclarationSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for BlockModuleDeclarationSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        BlockModuleDeclarationSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for ModuleBodySyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        ModuleBodySyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for IdentifierListSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        IdentifierListSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl ModuleDeclarationSyntaxSink for SourceUnitModuleDeclarationSyntaxBuilder {
    fn push_module_modifiers(&mut self, modifiers: ModuleModifiersSyntax) {
        SourceUnitModuleDeclarationSyntaxBuilder::push_module_modifiers(self, modifiers);
    }

    fn push_module_keyword(&mut self, token: SyntaxToken) {
        SourceUnitModuleDeclarationSyntaxBuilder::push_module_keyword(self, token);
    }

    fn push_module_path(&mut self, path: PathSyntax) {
        SourceUnitModuleDeclarationSyntaxBuilder::push_module_path(self, path);
    }
}

impl ModuleDeclarationSyntaxSink for BlockModuleDeclarationSyntaxBuilder {
    fn push_module_modifiers(&mut self, modifiers: ModuleModifiersSyntax) {
        BlockModuleDeclarationSyntaxBuilder::push_module_modifiers(self, modifiers);
    }

    fn push_module_keyword(&mut self, token: SyntaxToken) {
        BlockModuleDeclarationSyntaxBuilder::push_module_keyword(self, token);
    }

    fn push_module_path(&mut self, path: PathSyntax) {
        BlockModuleDeclarationSyntaxBuilder::push_module_path(self, path);
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
                DiagnosticKind::SyntaxSkippedSyntax,
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
    fn parser_recovers_invalid_tokens_as_skipped_syntax() {
        let sources = source_store(["$"]);
        let result = parse_compilation_unit(&sources);
        let source_unit = &result.syntax_tree().root().source_units()[0];
        let skipped_syntax = source_unit.skipped_syntax().collect::<Vec<_>>();

        assert_eq!(
            parse_diagnostic_kinds(&result),
            [
                DiagnosticKind::LexicalInvalidCharacter,
                DiagnosticKind::SyntaxSkippedSyntax,
                DiagnosticKind::SyntaxUnexpectedEof,
                DiagnosticKind::SyntaxUnexpectedEof,
                DiagnosticKind::SyntaxUnexpectedEof
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
    fn parser_parses_source_unit_module_declarations() {
        let sources = source_store(["trusted public module main.core; func main() {}"]);
        let result = parse_compilation_unit(&sources);
        let source_unit = &result.syntax_tree().root().source_units()[0];

        let declaration = match source_unit.source_unit_module_declaration() {
            Some(declaration) => declaration,
            None => panic!("expected source-unit module declaration"),
        };

        let modifiers = declaration.module_modifiers();
        let path = declaration.module_path();
        let skipped_syntax = source_unit.skipped_syntax().collect::<Vec<_>>();

        assert_eq!(
            source_unit.full_text(),
            "trusted public module main.core; func main() {}"
        );

        assert_eq!(declaration.full_text(), "trusted public module main.core; ");

        assert_eq!(
            modifiers.trusted_token().map(|token| token.kind()),
            Some(SyntaxKind::TrustedKeyword)
        );

        assert_eq!(
            modifiers.visibility_token().map(|token| token.kind()),
            Some(SyntaxKind::PublicKeyword)
        );

        assert_eq!(path.full_text(), "main.core");
        assert_eq!(path.identifier_tokens().count(), 2);
        assert_eq!(path.dot_tokens().count(), 1);

        let [skipped] = skipped_syntax.as_slice() else {
            panic!("expected one skipped-syntax node: {skipped_syntax:?}");
        };

        assert_eq!(skipped.full_text(), "func main() {}");

        assert_eq!(
            parse_diagnostic_kinds(&result),
            [DiagnosticKind::SyntaxSkippedSyntax]
        );
    }

    #[test]
    fn parser_parses_block_module_declarations_and_skips_body_items() {
        let sources = source_store(["internal module main { func run() {} } module extra {}"]);
        let result = parse_compilation_unit(&sources);
        let source_unit = &result.syntax_tree().root().source_units()[0];
        let declarations = source_unit.block_module_declarations().collect::<Vec<_>>();

        let [first, second] = declarations.as_slice() else {
            panic!("expected two block module declarations: {declarations:?}");
        };

        assert_eq!(
            source_unit.full_text(),
            "internal module main { func run() {} } module extra {}"
        );

        assert!(source_unit.source_unit_module_declaration().is_none());

        assert_eq!(
            first
                .module_modifiers()
                .visibility_token()
                .map(|token| token.kind()),
            Some(SyntaxKind::InternalKeyword)
        );

        assert_eq!(first.module_path().full_text(), "main ");
        assert_eq!(first.module_body().full_text(), "{ func run() {} } ");
        assert_eq!(first.module_body().skipped_syntax().count(), 1);
        assert_eq!(second.module_path().full_text(), "extra ");
        assert!(second.module_body().skipped_syntax().next().is_none());

        assert_eq!(
            parse_diagnostic_kinds(&result),
            [DiagnosticKind::SyntaxSkippedSyntax]
        );
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
        let sources = source_store(["module main;"]);

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

        assert_eq!(source_unit.full_text(), "module main;");
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
        let sources = source_store(["module main;", "module extra {}"]);
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
