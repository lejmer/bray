use std::fmt;

use bray_source::{SourceSnapshot, TextRange, TextSize};

use super::body::ModuleBodySyntax;
use super::modifiers::ModuleModifiersSyntax;
use crate::builder::{GreenNodeBuilder, RequiredSyntaxSlot, require_token_kind};
use crate::green::GreenNode;
use crate::node::{GreenSourceSyntaxNode, GreenSyntaxNode};
use crate::node_support::{contains_recovery, required_child_node, required_token, skipped_syntax};
use crate::syntax::path::PathSyntax;
use crate::{SkippedSyntax, SyntaxKind, SyntaxNode, SyntaxToken};

/// Module declaration that owns loose source-unit module items.
#[derive(Clone, Eq, Hash, PartialEq)]
pub struct SourceUnitModuleDeclarationSyntax {
    source: SourceSnapshot,
    node: GreenNode,
    start: TextSize,
}

impl SourceUnitModuleDeclarationSyntax {
    /// Creates a builder for a source-unit module declaration at `start`.
    pub fn builder(
        source: SourceSnapshot,
        start: TextSize,
    ) -> SourceUnitModuleDeclarationSyntaxBuilder {
        SourceUnitModuleDeclarationSyntaxBuilder::new(source, start)
    }

    pub(crate) fn from_green(source: SourceSnapshot, node: GreenNode, start: TextSize) -> Self {
        assert_eq!(node.kind(), SyntaxKind::SourceUnitModuleDeclaration);

        Self {
            source,
            node,
            start,
        }
    }

    pub(crate) fn into_green(self) -> GreenNode {
        self.node
    }

    fn from_builder(builder: SourceUnitModuleDeclarationSyntaxBuilder) -> Self {
        Self {
            source: builder.source.into_value(),
            node: builder.node.build(SyntaxKind::SourceUnitModuleDeclaration),
            start: builder.start,
        }
    }

    /// Returns the full source text range covered by this declaration.
    pub fn full_range(&self) -> TextRange {
        SyntaxNode::full_range(self)
    }

    /// Returns whether this declaration contains parser recovery.
    pub fn is_recovered(&self) -> bool {
        contains_recovery(&self.node, self.start)
    }

    /// Returns the module-modifiers child.
    pub fn module_modifiers(&self) -> ModuleModifiersSyntax {
        required_child_node(
            &self.source,
            &self.node,
            self.start,
            SyntaxKind::ModuleModifiers,
            ModuleModifiersSyntax::from_green,
            "source-unit module declaration",
        )
    }

    /// Returns the required `module` keyword token.
    pub fn module_keyword(&self) -> SyntaxToken {
        required_token(
            &self.node,
            self.start,
            SyntaxKind::ModuleKeyword,
            "source-unit module declaration",
        )
    }

    /// Returns the required module path child.
    pub fn module_path(&self) -> PathSyntax {
        required_child_node(
            &self.source,
            &self.node,
            self.start,
            SyntaxKind::Path,
            PathSyntax::from_green,
            "source-unit module declaration",
        )
    }

    /// Returns the required semicolon token.
    pub fn semicolon_token(&self) -> SyntaxToken {
        required_token(
            &self.node,
            self.start,
            SyntaxKind::SemicolonToken,
            "source-unit module declaration",
        )
    }

    /// Returns descendant skipped-syntax recovery nodes in source order.
    pub fn skipped_syntax(&self) -> impl Iterator<Item = SkippedSyntax> + '_ {
        skipped_syntax(&self.source, &self.node, self.start)
    }

    /// Returns descendant syntax tokens in source order.
    pub fn tokens(&self) -> impl Iterator<Item = SyntaxToken> + '_ {
        self.node.syntax_tokens(self.start)
    }
}

impl fmt::Debug for SourceUnitModuleDeclarationSyntax {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SourceUnitModuleDeclarationSyntax")
            .field("source", &self.source)
            .field("kind", &SyntaxNode::kind(self))
            .field("full_range", &self.full_range())
            .field("is_recovered", &self.is_recovered())
            .finish()
    }
}

impl GreenSyntaxNode for SourceUnitModuleDeclarationSyntax {
    fn green_node(&self) -> &GreenNode {
        &self.node
    }

    fn start(&self) -> TextSize {
        self.start
    }

    fn range_description(&self) -> &'static str {
        "source-unit module declaration"
    }

    fn is_recovered(&self) -> bool {
        SourceUnitModuleDeclarationSyntax::is_recovered(self)
    }
}

impl GreenSourceSyntaxNode for SourceUnitModuleDeclarationSyntax {
    fn source(&self) -> &SourceSnapshot {
        &self.source
    }
}

/// Builder for a source-unit module declaration.
pub struct SourceUnitModuleDeclarationSyntaxBuilder {
    source: RequiredSyntaxSlot<SourceSnapshot>,
    node: GreenNodeBuilder,
    start: TextSize,
}

impl SourceUnitModuleDeclarationSyntaxBuilder {
    /// Creates an empty source-unit module declaration builder at `start`.
    pub fn new(source: SourceSnapshot, start: TextSize) -> Self {
        let mut source_slot = RequiredSyntaxSlot::new("source_unit_module_declaration.source");

        source_slot.set(source);

        Self {
            source: source_slot,
            node: GreenNodeBuilder::new(),
            start,
        }
    }

    /// Appends present source tokens under a skipped-syntax recovery node.
    pub fn push_skipped_tokens(&mut self, tokens: impl IntoIterator<Item = SyntaxToken>) {
        self.node.push_skipped_tokens(tokens);
    }

    /// Appends the module-modifiers child.
    pub fn push_module_modifiers(&mut self, modifiers: ModuleModifiersSyntax) {
        self.node.push_node(modifiers.into_green());
    }

    /// Appends the module keyword token.
    pub fn push_module_keyword(&mut self, token: SyntaxToken) {
        require_token_kind(
            token.kind(),
            SyntaxKind::ModuleKeyword,
            "source_unit_module_declaration.module_keyword",
        );

        self.node.push_token(token);
    }

    /// Appends the module path child.
    pub fn push_module_path(&mut self, path: PathSyntax) {
        self.node.push_node(path.into_green());
    }

    /// Appends the semicolon token.
    pub fn push_semicolon_token(&mut self, token: SyntaxToken) {
        require_token_kind(
            token.kind(),
            SyntaxKind::SemicolonToken,
            "source_unit_module_declaration.semicolon_token",
        );

        self.node.push_token(token);
    }

    /// Builds the source-unit module declaration node.
    pub fn build(self) -> SourceUnitModuleDeclarationSyntax {
        SourceUnitModuleDeclarationSyntax::from_builder(self)
    }
}

impl fmt::Debug for SourceUnitModuleDeclarationSyntaxBuilder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SourceUnitModuleDeclarationSyntaxBuilder")
            .field("start", &self.start)
            .finish_non_exhaustive()
    }
}

/// Braced top-level module declaration.
#[derive(Clone, Eq, Hash, PartialEq)]
pub struct BlockModuleDeclarationSyntax {
    source: SourceSnapshot,
    node: GreenNode,
    start: TextSize,
}

impl BlockModuleDeclarationSyntax {
    /// Creates a builder for a block module declaration at `start`.
    pub fn builder(source: SourceSnapshot, start: TextSize) -> BlockModuleDeclarationSyntaxBuilder {
        BlockModuleDeclarationSyntaxBuilder::new(source, start)
    }

    pub(crate) fn from_green(source: SourceSnapshot, node: GreenNode, start: TextSize) -> Self {
        assert_eq!(node.kind(), SyntaxKind::BlockModuleDeclaration);

        Self {
            source,
            node,
            start,
        }
    }

    pub(crate) fn into_green(self) -> GreenNode {
        self.node
    }

    fn from_builder(builder: BlockModuleDeclarationSyntaxBuilder) -> Self {
        Self {
            source: builder.source.into_value(),
            node: builder.node.build(SyntaxKind::BlockModuleDeclaration),
            start: builder.start,
        }
    }

    /// Returns the full source text range covered by this declaration.
    pub fn full_range(&self) -> TextRange {
        SyntaxNode::full_range(self)
    }

    /// Returns whether this declaration contains parser recovery.
    pub fn is_recovered(&self) -> bool {
        contains_recovery(&self.node, self.start)
    }

    /// Returns the module-modifiers child.
    pub fn module_modifiers(&self) -> ModuleModifiersSyntax {
        required_child_node(
            &self.source,
            &self.node,
            self.start,
            SyntaxKind::ModuleModifiers,
            ModuleModifiersSyntax::from_green,
            "block module declaration",
        )
    }

    /// Returns the required `module` keyword token.
    pub fn module_keyword(&self) -> SyntaxToken {
        required_token(
            &self.node,
            self.start,
            SyntaxKind::ModuleKeyword,
            "block module declaration",
        )
    }

    /// Returns the required module path child.
    pub fn module_path(&self) -> PathSyntax {
        required_child_node(
            &self.source,
            &self.node,
            self.start,
            SyntaxKind::Path,
            PathSyntax::from_green,
            "block module declaration",
        )
    }

    /// Returns the required module-body child.
    pub fn module_body(&self) -> ModuleBodySyntax {
        required_child_node(
            &self.source,
            &self.node,
            self.start,
            SyntaxKind::ModuleBody,
            ModuleBodySyntax::from_green,
            "block module declaration",
        )
    }

    /// Returns descendant skipped-syntax recovery nodes in source order.
    pub fn skipped_syntax(&self) -> impl Iterator<Item = SkippedSyntax> + '_ {
        skipped_syntax(&self.source, &self.node, self.start)
    }

    /// Returns descendant syntax tokens in source order.
    pub fn tokens(&self) -> impl Iterator<Item = SyntaxToken> + '_ {
        self.node.syntax_tokens(self.start)
    }
}

impl fmt::Debug for BlockModuleDeclarationSyntax {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BlockModuleDeclarationSyntax")
            .field("source", &self.source)
            .field("kind", &SyntaxNode::kind(self))
            .field("full_range", &self.full_range())
            .field("is_recovered", &self.is_recovered())
            .finish()
    }
}

impl GreenSyntaxNode for BlockModuleDeclarationSyntax {
    fn green_node(&self) -> &GreenNode {
        &self.node
    }

    fn start(&self) -> TextSize {
        self.start
    }

    fn range_description(&self) -> &'static str {
        "block module declaration"
    }

    fn is_recovered(&self) -> bool {
        BlockModuleDeclarationSyntax::is_recovered(self)
    }
}

impl GreenSourceSyntaxNode for BlockModuleDeclarationSyntax {
    fn source(&self) -> &SourceSnapshot {
        &self.source
    }
}

/// Builder for a block module declaration.
pub struct BlockModuleDeclarationSyntaxBuilder {
    source: RequiredSyntaxSlot<SourceSnapshot>,
    node: GreenNodeBuilder,
    start: TextSize,
}

impl BlockModuleDeclarationSyntaxBuilder {
    /// Creates an empty block module declaration builder at `start`.
    pub fn new(source: SourceSnapshot, start: TextSize) -> Self {
        let mut source_slot = RequiredSyntaxSlot::new("block_module_declaration.source");

        source_slot.set(source);

        Self {
            source: source_slot,
            node: GreenNodeBuilder::new(),
            start,
        }
    }

    /// Appends present source tokens under a skipped-syntax recovery node.
    pub fn push_skipped_tokens(&mut self, tokens: impl IntoIterator<Item = SyntaxToken>) {
        self.node.push_skipped_tokens(tokens);
    }

    /// Appends the module-modifiers child.
    pub fn push_module_modifiers(&mut self, modifiers: ModuleModifiersSyntax) {
        self.node.push_node(modifiers.into_green());
    }

    /// Appends the module keyword token.
    pub fn push_module_keyword(&mut self, token: SyntaxToken) {
        require_token_kind(
            token.kind(),
            SyntaxKind::ModuleKeyword,
            "block_module_declaration.module_keyword",
        );

        self.node.push_token(token);
    }

    /// Appends the module path child.
    pub fn push_module_path(&mut self, path: PathSyntax) {
        self.node.push_node(path.into_green());
    }

    /// Appends the module-body child.
    pub fn push_module_body(&mut self, body: ModuleBodySyntax) {
        self.node.push_node(body.into_green());
    }

    /// Builds the block module declaration node.
    pub fn build(self) -> BlockModuleDeclarationSyntax {
        BlockModuleDeclarationSyntax::from_builder(self)
    }
}

impl fmt::Debug for BlockModuleDeclarationSyntaxBuilder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BlockModuleDeclarationSyntaxBuilder")
            .field("start", &self.start)
            .finish_non_exhaustive()
    }
}
