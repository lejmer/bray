use std::fmt::{self, Write};
use std::sync::Arc;

use bray_base::shared_slice;
use bray_source::{SourceSnapshot, TextRange, TextSize};

use super::recovery::{SkippedSyntax, skipped_syntax_nodes};
use super::{
    BlockModuleDeclarationSyntax, IdentifierListSyntax, SourceUnitModuleDeclarationSyntax,
};
use crate::builder::{GreenNodeBuilder, RequiredSyntaxSlot, SyntaxListSlot, require_token_kind};
use crate::green::GreenNode;
use crate::node::{GreenSourceSyntaxNode, GreenSyntaxNode};
use crate::{SyntaxKind, SyntaxNode, SyntaxText, SyntaxToken, SyntaxTokenPresence};

/// Root syntax node for one compiler compilation unit.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CompilationUnitSyntax {
    source_units: Arc<[SourceUnitSyntax]>,
}

impl CompilationUnitSyntax {
    /// Creates a builder for a compilation-unit node.
    pub const fn builder() -> CompilationUnitSyntaxBuilder {
        CompilationUnitSyntaxBuilder::new()
    }

    fn from_builder(builder: CompilationUnitSyntaxBuilder) -> Self {
        Self {
            source_units: shared_slice(builder.source_units.into_vec()),
        }
    }

    /// Returns this node's stable syntax kind.
    pub const fn kind(&self) -> SyntaxKind {
        SyntaxKind::CompilationUnit
    }

    /// Returns this node's full range.
    ///
    /// Compilation units contain source units from distinct source coordinate
    /// spaces, so the root range is intentionally empty.
    pub const fn full_range(&self) -> TextRange {
        TextRange::EMPTY
    }

    /// Returns the source-unit children in source ID order.
    pub fn source_units(&self) -> &[SourceUnitSyntax] {
        &self.source_units
    }

    /// Returns whether this compilation unit contains no source units.
    pub fn is_empty(&self) -> bool {
        self.source_units.is_empty()
    }
}

/// Builder for a compilation-unit syntax node.
#[derive(Debug, Default)]
pub struct CompilationUnitSyntaxBuilder {
    source_units: SyntaxListSlot<SourceUnitSyntax>,
}

impl CompilationUnitSyntaxBuilder {
    /// Creates an empty compilation-unit builder.
    pub const fn new() -> Self {
        Self {
            source_units: SyntaxListSlot::new(),
        }
    }

    /// Appends a source-unit child to the compilation-unit child list.
    pub fn push_source_unit(&mut self, source_unit: SourceUnitSyntax) {
        self.source_units.push(source_unit);
    }

    /// Appends source-unit children to the compilation-unit child list.
    pub fn source_units(
        mut self,
        source_units: impl IntoIterator<Item = SourceUnitSyntax>,
    ) -> Self {
        self.source_units.extend(source_units);

        self
    }

    /// Builds the compilation-unit node.
    pub fn build(self) -> CompilationUnitSyntax {
        CompilationUnitSyntax::from_builder(self)
    }
}

impl SyntaxText for CompilationUnitSyntax {
    /// Appends the exact source text for every source unit in order.
    fn write_full_text(&self, writer: &mut dyn Write) -> fmt::Result {
        for source_unit in self.source_units() {
            source_unit.write_full_text(writer)?;
        }

        Ok(())
    }
}

impl SyntaxNode for CompilationUnitSyntax {
    fn kind(&self) -> SyntaxKind {
        self.kind()
    }

    fn full_range(&self) -> TextRange {
        self.full_range()
    }
}

/// Root syntax node for one parsed source snapshot.
#[derive(Clone, Eq, Hash, PartialEq)]
pub struct SourceUnitSyntax {
    source: SourceSnapshot,
    node: GreenNode,
}

impl SourceUnitSyntax {
    /// Creates a builder for a source-unit node.
    pub fn builder(source: SourceSnapshot) -> SourceUnitSyntaxBuilder {
        SourceUnitSyntaxBuilder::new(source)
    }

    fn from_builder(builder: SourceUnitSyntaxBuilder) -> Self {
        let source = builder.source.into_value();

        match builder.node.last_token() {
            Some((kind, SyntaxTokenPresence::Present)) if kind == SyntaxKind::EndOfFileToken => {
                require_token_kind(kind, SyntaxKind::EndOfFileToken, "source_unit.eof_token");
            }
            Some((SyntaxKind::EndOfFileToken, SyntaxTokenPresence::Missing)) => {
                panic!("source unit token stream must end with present EOF");
            }
            _ => panic!("source unit token stream must end with EOF"),
        }

        let node = builder.node.build(SyntaxKind::SourceUnit);

        Self { source, node }
    }

    /// Returns the full source text range.
    pub fn full_range(&self) -> TextRange {
        SyntaxNode::full_range(self)
    }

    /// Returns this source unit's syntax tokens in source order, including EOF.
    ///
    /// Tokens are synthesized from immutable green storage and this source
    /// unit's coordinate space. Grammar-aware code should prefer named slots
    /// and child lists on concrete syntax nodes as those nodes are added.
    pub fn tokens(&self) -> impl Iterator<Item = SyntaxToken> + '_ {
        self.node.syntax_tokens(TextSize::ZERO)
    }

    /// Returns skipped-syntax recovery nodes in source order.
    pub fn skipped_syntax(&self) -> impl Iterator<Item = SkippedSyntax> + '_ {
        skipped_syntax_nodes(&self.source, &self.node, TextSize::ZERO)
    }

    /// Returns direct identifier-list child nodes in source order.
    pub fn identifier_lists(&self) -> impl Iterator<Item = IdentifierListSyntax> + '_ {
        self.node
            .child_nodes(TextSize::ZERO, SyntaxKind::IdentifierList)
            .map(|(node, start)| {
                // SourceSnapshot clones share immutable source text with typed child nodes.
                IdentifierListSyntax::from_green(self.source.clone(), node, start)
            })
    }

    /// Returns the source-unit module declaration child when present.
    pub fn source_unit_module_declaration(&self) -> Option<SourceUnitModuleDeclarationSyntax> {
        self.node
            .child_nodes(TextSize::ZERO, SyntaxKind::SourceUnitModuleDeclaration)
            .next()
            .map(|(node, start)| {
                // SourceSnapshot clones share immutable source text with typed child nodes.
                SourceUnitModuleDeclarationSyntax::from_green(self.source.clone(), node, start)
            })
    }

    /// Returns direct block module declaration children in source order.
    pub fn block_module_declarations(
        &self,
    ) -> impl Iterator<Item = BlockModuleDeclarationSyntax> + '_ {
        self.node
            .child_nodes(TextSize::ZERO, SyntaxKind::BlockModuleDeclaration)
            .map(|(node, start)| {
                // SourceSnapshot clones share immutable source text with typed child nodes.
                BlockModuleDeclarationSyntax::from_green(self.source.clone(), node, start)
            })
    }

    /// Returns the required EOF token for this source unit.
    pub fn eof_token(&self) -> SyntaxToken {
        match self.node.syntax_tokens(TextSize::ZERO).last() {
            Some(token) => token,
            None => panic!("source unit token stream must end with EOF"),
        }
    }
}

impl fmt::Debug for SourceUnitSyntax {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SourceUnitSyntax")
            .field("source", &self.source)
            .field("kind", &SyntaxNode::kind(self))
            .field("full_range", &self.full_range())
            .finish()
    }
}

/// Builder for a source-unit syntax node.
pub struct SourceUnitSyntaxBuilder {
    source: RequiredSyntaxSlot<SourceSnapshot>,
    node: GreenNodeBuilder,
}

impl SourceUnitSyntaxBuilder {
    /// Creates an empty source-unit builder for `source`.
    pub fn new(source: SourceSnapshot) -> Self {
        let mut source_slot = RequiredSyntaxSlot::new("source_unit.source");

        source_slot.set(source);

        Self {
            source: source_slot,
            node: GreenNodeBuilder::new(),
        }
    }

    /// Appends a token to the source-unit token list.
    pub fn push_token(&mut self, token: SyntaxToken) {
        self.node.push_token(token);
    }

    /// Appends present source tokens under a skipped-syntax recovery node.
    ///
    /// Empty token lists do not add a recovery node.
    ///
    /// Panics when any skipped token is missing.
    pub fn push_skipped_tokens(&mut self, tokens: impl IntoIterator<Item = SyntaxToken>) {
        self.node.push_skipped_tokens(tokens);
    }

    /// Appends an identifier-list child in source order.
    pub fn push_identifier_list(&mut self, list: IdentifierListSyntax) {
        self.node.push_node(list.into_green());
    }

    /// Appends a source-unit module declaration child in source order.
    pub fn push_source_unit_module_declaration(
        &mut self,
        declaration: SourceUnitModuleDeclarationSyntax,
    ) {
        self.node.push_node(declaration.into_green());
    }

    /// Appends a block module declaration child in source order.
    pub fn push_block_module_declaration(&mut self, declaration: BlockModuleDeclarationSyntax) {
        self.node.push_node(declaration.into_green());
    }

    /// Appends tokens to the source-unit token list.
    pub fn tokens(mut self, tokens: impl IntoIterator<Item = SyntaxToken>) -> Self {
        self.node.push_tokens(tokens);

        self
    }

    /// Appends present source tokens under a skipped-syntax recovery node.
    ///
    /// Empty token lists do not add a recovery node.
    ///
    /// Panics when any skipped token is missing.
    pub fn skipped_tokens(mut self, tokens: impl IntoIterator<Item = SyntaxToken>) -> Self {
        self.push_skipped_tokens(tokens);

        self
    }

    /// Appends an identifier-list child in source order.
    pub fn identifier_list(mut self, list: IdentifierListSyntax) -> Self {
        self.push_identifier_list(list);

        self
    }

    /// Appends a source-unit module declaration child in source order.
    pub fn source_unit_module_declaration(
        mut self,
        declaration: SourceUnitModuleDeclarationSyntax,
    ) -> Self {
        self.push_source_unit_module_declaration(declaration);

        self
    }

    /// Appends a block module declaration child in source order.
    pub fn block_module_declaration(mut self, declaration: BlockModuleDeclarationSyntax) -> Self {
        self.push_block_module_declaration(declaration);

        self
    }

    /// Builds the source-unit node.
    ///
    /// Panics when the token list does not end in EOF.
    pub fn build(self) -> SourceUnitSyntax {
        SourceUnitSyntax::from_builder(self)
    }
}

impl fmt::Debug for SourceUnitSyntaxBuilder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SourceUnitSyntaxBuilder")
            .finish_non_exhaustive()
    }
}

impl GreenSyntaxNode for SourceUnitSyntax {
    fn green_node(&self) -> &GreenNode {
        &self.node
    }

    fn start(&self) -> TextSize {
        TextSize::ZERO
    }

    fn range_description(&self) -> &'static str {
        "source-unit"
    }
}

impl GreenSourceSyntaxNode for SourceUnitSyntax {
    fn source(&self) -> &SourceSnapshot {
        &self.source
    }
}

#[cfg(test)]
mod tests {
    use bray_source::{
        SourceId, SourceIdentity, SourceOrigin, SourceSnapshot, SourceVersion, TextRange, TextSize,
    };

    use super::{CompilationUnitSyntax, SourceUnitSyntax};
    use crate::{
        BlockModuleDeclarationSyntax, IdentifierListItemSyntax, IdentifierListSyntax,
        ModuleBodySyntax, ModuleDirectivesSyntax, ModuleModifiersSyntax, PathSyntax,
        SourceSyntaxNode, SourceUnitModuleDeclarationSyntax, SyntaxKind, SyntaxNode, SyntaxText,
        SyntaxToken, SyntaxTrivia,
    };

    #[test]
    fn source_units_store_source_snapshot_named_tokens_and_full_range() {
        let snapshot = snapshot("func");

        let first = SyntaxToken::new(
            SyntaxKind::FuncKeyword,
            TextRange::new(TextSize::ZERO, TextSize::new(4)),
        );

        let eof = SyntaxToken::end_of_file(TextSize::new(4));

        let source_unit = SourceUnitSyntax::builder(snapshot)
            .tokens([first.clone(), eof.clone()])
            .build();

        assert_eq!(source_unit.kind(), SyntaxKind::SourceUnit);

        assert_eq!(
            source_unit.full_range(),
            TextRange::new(TextSize::ZERO, TextSize::new(4))
        );

        assert_eq!(
            source_unit.tokens().collect::<Vec<_>>(),
            [first, eof.clone()]
        );

        assert_eq!(source_unit.eof_token(), eof);
    }

    #[test]
    fn compilation_units_store_named_source_unit_children() {
        let source_unit = SourceUnitSyntax::builder(snapshot(""))
            .tokens([SyntaxToken::end_of_file(TextSize::ZERO)])
            .build();

        let compilation_unit = CompilationUnitSyntax::builder()
            .source_units([source_unit.clone()])
            .build();

        assert_eq!(compilation_unit.kind(), SyntaxKind::CompilationUnit);
        assert_eq!(compilation_unit.source_units(), &[source_unit]);
        assert_eq!(compilation_unit.full_range(), TextRange::EMPTY);
    }

    #[test]
    fn typed_nodes_implement_syntax_node_contract() {
        let source_unit = SourceUnitSyntax::builder(snapshot(""))
            .tokens([SyntaxToken::end_of_file(TextSize::ZERO)])
            .build();

        let compilation_unit = CompilationUnitSyntax::builder()
            .source_units([source_unit.clone()])
            .build();

        assert_node_kind(&source_unit, SyntaxKind::SourceUnit);
        assert_node_kind(&compilation_unit, SyntaxKind::CompilationUnit);
    }

    #[test]
    fn source_units_reconstruct_exact_source_text_from_tokens() {
        let snapshot = snapshot("  func// tail");
        let leading = SyntaxTrivia::whitespace(TextRange::new(TextSize::ZERO, TextSize::new(2)));

        let trailing =
            SyntaxTrivia::line_comment(TextRange::new(TextSize::new(6), TextSize::new(13)));

        let token = SyntaxToken::new(
            SyntaxKind::FuncKeyword,
            TextRange::new(TextSize::new(2), TextSize::new(6)),
        )
        .with_leading_trivia([leading])
        .with_trailing_trivia([trailing]);

        let source_unit = SourceUnitSyntax::builder(snapshot)
            .tokens([token, SyntaxToken::end_of_file(TextSize::new(13))])
            .build();

        assert_eq!(
            source_unit.full_text_from(source_unit.source().text()),
            "  func// tail"
        );

        let compilation_unit = CompilationUnitSyntax::builder()
            .source_units([source_unit])
            .build();

        assert_eq!(compilation_unit.full_text(), "  func// tail");
    }

    #[test]
    fn source_units_preserve_missing_tokens_in_named_slots() {
        let snapshot = snapshot("func");
        let token = SyntaxToken::new(
            SyntaxKind::FuncKeyword,
            TextRange::new(TextSize::ZERO, TextSize::new(4)),
        );

        let missing = SyntaxToken::missing(SyntaxKind::SemicolonToken, TextSize::new(4));
        let eof = SyntaxToken::end_of_file(TextSize::new(4));

        let source_unit = SourceUnitSyntax::builder(snapshot)
            .tokens([token.clone(), missing.clone(), eof.clone()])
            .build();

        let tokens = source_unit.tokens().collect::<Vec<_>>();

        assert_eq!(tokens, [token, missing.clone(), eof]);
        assert!(tokens[1].is_missing());
        assert_eq!(tokens[1].kind(), SyntaxKind::SemicolonToken);
        assert_eq!(source_unit.full_text(), "func");
    }

    #[test]
    fn source_units_attach_skipped_syntax_without_losing_source_text() {
        let snapshot = snapshot("func @ main");

        let func = SyntaxToken::new(
            SyntaxKind::FuncKeyword,
            TextRange::new(TextSize::ZERO, TextSize::new(4)),
        )
        .with_trailing_trivia([SyntaxTrivia::whitespace(TextRange::new(
            TextSize::new(4),
            TextSize::new(5),
        ))]);

        let skipped_token =
            SyntaxToken::invalid(TextRange::new(TextSize::new(5), TextSize::new(6)))
                .with_trailing_trivia([SyntaxTrivia::whitespace(TextRange::new(
                    TextSize::new(6),
                    TextSize::new(7),
                ))]);

        let main = SyntaxToken::new(
            SyntaxKind::IdentifierToken,
            TextRange::new(TextSize::new(7), TextSize::new(11)),
        );

        let eof = SyntaxToken::end_of_file(TextSize::new(11));
        let mut builder = SourceUnitSyntax::builder(snapshot);

        builder.push_token(func.clone());
        builder.push_skipped_tokens([skipped_token.clone()]);
        builder.push_token(main.clone());
        builder.push_token(eof.clone());

        let source_unit = builder.build();
        let skipped_syntax = source_unit.skipped_syntax().collect::<Vec<_>>();

        assert_eq!(source_unit.full_text(), "func @ main");

        assert_eq!(
            source_unit.tokens().collect::<Vec<_>>(),
            [func, skipped_token.clone(), main, eof]
        );

        let [skipped] = skipped_syntax.as_slice() else {
            panic!("expected one skipped-syntax node: {skipped_syntax:?}");
        };

        assert_eq!(skipped.kind(), SyntaxKind::SkippedSyntax);
        assert!(skipped.is_recovered());

        assert_eq!(
            skipped.full_range(),
            TextRange::new(TextSize::new(5), TextSize::new(7))
        );

        assert_eq!(skipped.tokens().collect::<Vec<_>>(), [skipped_token]);
        assert_eq!(skipped.full_text(), "@ ");
    }

    #[test]
    fn source_units_expose_identifier_list_children() {
        let snapshot = snapshot("a,b");

        let first = IdentifierListItemSyntax::builder(snapshot.clone())
            .identifier_token(SyntaxToken::new(
                SyntaxKind::IdentifierToken,
                TextRange::new(TextSize::ZERO, TextSize::new(1)),
            ))
            .build();

        let comma = SyntaxToken::new(
            SyntaxKind::CommaToken,
            TextRange::new(TextSize::new(1), TextSize::new(2)),
        );

        let second = IdentifierListItemSyntax::builder(snapshot.clone())
            .identifier_token(SyntaxToken::new(
                SyntaxKind::IdentifierToken,
                TextRange::new(TextSize::new(2), TextSize::new(3)),
            ))
            .build();

        let list = IdentifierListSyntax::builder(snapshot.clone(), TextSize::ZERO)
            .item(first)
            .separator_token(comma)
            .item(second)
            .build();

        let source_unit = SourceUnitSyntax::builder(snapshot)
            .identifier_list(list)
            .tokens([SyntaxToken::end_of_file(TextSize::new(3))])
            .build();

        let identifier_lists = source_unit.identifier_lists().collect::<Vec<_>>();

        let [identifier_list] = identifier_lists.as_slice() else {
            panic!("expected one identifier-list child: {identifier_lists:?}");
        };

        assert_eq!(source_unit.full_text(), "a,b");
        assert_eq!(identifier_list.full_text(), "a,b");
        assert_eq!(identifier_list.items().count(), 2);
    }

    #[test]
    fn source_units_expose_source_unit_module_declaration_children() {
        let snapshot = snapshot("module main;");
        let declaration = source_unit_module_declaration(snapshot.clone());
        let eof = SyntaxToken::end_of_file(TextSize::new(12));

        let source_unit = SourceUnitSyntax::builder(snapshot)
            .source_unit_module_declaration(declaration)
            .tokens([eof])
            .build();

        let declaration = match source_unit.source_unit_module_declaration() {
            Some(declaration) => declaration,
            None => panic!("expected source-unit module declaration"),
        };

        assert_eq!(source_unit.full_text(), "module main;");
        assert_eq!(declaration.full_text(), "module main;");

        assert_eq!(
            declaration.module_keyword().kind(),
            SyntaxKind::ModuleKeyword
        );

        assert_eq!(
            declaration.semicolon_token().kind(),
            SyntaxKind::SemicolonToken
        );

        assert_eq!(declaration.module_path().full_text(), "main");
        assert!(source_unit.block_module_declarations().next().is_none());
    }

    #[test]
    fn source_units_expose_block_module_declaration_children() {
        let snapshot = snapshot("module main {}");
        let declaration = block_module_declaration(snapshot.clone());
        let eof = SyntaxToken::end_of_file(TextSize::new(14));

        let source_unit = SourceUnitSyntax::builder(snapshot)
            .block_module_declaration(declaration)
            .tokens([eof])
            .build();

        let block_declarations = source_unit.block_module_declarations().collect::<Vec<_>>();

        let [declaration] = block_declarations.as_slice() else {
            panic!("expected one block module declaration: {block_declarations:?}");
        };

        assert_eq!(source_unit.full_text(), "module main {}");
        assert!(source_unit.source_unit_module_declaration().is_none());

        assert_eq!(
            declaration.module_keyword().kind(),
            SyntaxKind::ModuleKeyword
        );

        assert_eq!(declaration.module_path().full_text(), "main ");
        assert_eq!(declaration.module_body().full_text(), "{}");
    }

    #[test]
    fn source_unit_debug_does_not_expose_green_storage() {
        let source_unit = SourceUnitSyntax::builder(snapshot(""))
            .tokens([SyntaxToken::end_of_file(TextSize::ZERO)])
            .build();

        let debug_text = format!("{source_unit:?}");

        assert!(debug_text.contains("SourceUnitSyntax"));
        assert!(debug_text.contains("kind"));
        assert!(!debug_text.contains("GreenNode"));
    }

    #[test]
    fn typed_syntax_nodes_are_send_and_sync() {
        assert_send_sync::<CompilationUnitSyntax>();
        assert_send_sync::<SourceUnitSyntax>();
        assert_send_sync::<SourceUnitModuleDeclarationSyntax>();
        assert_send_sync::<BlockModuleDeclarationSyntax>();
        assert_send_sync::<ModuleDirectivesSyntax>();
        assert_send_sync::<ModuleModifiersSyntax>();
        assert_send_sync::<ModuleBodySyntax>();
        assert_send_sync::<PathSyntax>();
    }

    #[test]
    #[should_panic]
    fn source_units_reject_token_streams_without_eof() {
        let _ = SourceUnitSyntax::builder(snapshot("func"))
            .tokens(Vec::new())
            .build();
    }

    fn assert_node_kind(node: &impl SyntaxNode, kind: SyntaxKind) {
        assert_eq!(node.kind(), kind);
    }

    fn assert_send_sync<T: Send + Sync>() {}

    fn source_unit_module_declaration(
        snapshot: SourceSnapshot,
    ) -> SourceUnitModuleDeclarationSyntax {
        let mut builder =
            SourceUnitModuleDeclarationSyntax::builder(snapshot.clone(), TextSize::ZERO);

        builder.push_module_directives(
            ModuleDirectivesSyntax::builder(snapshot.clone(), TextSize::ZERO).build(),
        );
        builder.push_module_modifiers(
            ModuleModifiersSyntax::builder(snapshot.clone(), TextSize::ZERO).build(),
        );
        builder.push_module_keyword(module_keyword());
        builder.push_module_path(path(snapshot.clone(), false));
        builder.push_semicolon_token(SyntaxToken::new(
            SyntaxKind::SemicolonToken,
            TextRange::new(TextSize::new(11), TextSize::new(12)),
        ));

        builder.build()
    }

    fn block_module_declaration(snapshot: SourceSnapshot) -> BlockModuleDeclarationSyntax {
        let mut builder = BlockModuleDeclarationSyntax::builder(snapshot.clone(), TextSize::ZERO);

        builder.push_module_directives(
            ModuleDirectivesSyntax::builder(snapshot.clone(), TextSize::ZERO).build(),
        );
        builder.push_module_modifiers(
            ModuleModifiersSyntax::builder(snapshot.clone(), TextSize::ZERO).build(),
        );
        builder.push_module_keyword(module_keyword());
        builder.push_module_path(path(snapshot.clone(), true));
        builder.push_module_body(module_body(snapshot));

        builder.build()
    }

    fn module_keyword() -> SyntaxToken {
        SyntaxToken::new(
            SyntaxKind::ModuleKeyword,
            TextRange::new(TextSize::ZERO, TextSize::new(6)),
        )
        .with_trailing_trivia([SyntaxTrivia::whitespace(TextRange::new(
            TextSize::new(6),
            TextSize::new(7),
        ))])
    }

    fn path(snapshot: SourceSnapshot, has_trailing_space: bool) -> PathSyntax {
        let mut builder = PathSyntax::builder(snapshot);
        let mut token = SyntaxToken::new(
            SyntaxKind::IdentifierToken,
            TextRange::new(TextSize::new(7), TextSize::new(11)),
        );

        if has_trailing_space {
            token = token.with_trailing_trivia([SyntaxTrivia::whitespace(TextRange::new(
                TextSize::new(11),
                TextSize::new(12),
            ))]);
        }

        builder.push_identifier_token(token);

        builder.build()
    }

    fn module_body(snapshot: SourceSnapshot) -> ModuleBodySyntax {
        let mut builder = ModuleBodySyntax::builder(snapshot, TextSize::new(12));

        builder.push_open_brace_token(SyntaxToken::new(
            SyntaxKind::OpenBraceToken,
            TextRange::new(TextSize::new(12), TextSize::new(13)),
        ));
        builder.push_close_brace_token(SyntaxToken::new(
            SyntaxKind::CloseBraceToken,
            TextRange::new(TextSize::new(13), TextSize::new(14)),
        ));

        builder.build()
    }

    fn snapshot(text: &str) -> SourceSnapshot {
        match SourceSnapshot::new(
            SourceId::new(0),
            SourceIdentity::new(0),
            SourceOrigin::virtual_source("syntax-test"),
            SourceVersion::new(0),
            text,
        ) {
            Ok(snapshot) => snapshot,
            Err(error) => panic!("test source should fit in TextSize: {error:?}"),
        }
    }
}
