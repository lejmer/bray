use std::fmt;

use bray_source::{SourceSnapshot, TextRange, TextSize};

use crate::builder::{GreenNodeBuilder, RequiredSyntaxSlot, require_token_kind};
use crate::green::GreenNode;
use crate::node::{GreenSourceSyntaxNode, GreenSyntaxNode};
use crate::node_support::{contains_recovery, first_token};
use crate::{SyntaxKind, SyntaxNode, SyntaxToken};

/// Optional module modifiers in source order.
#[derive(Clone, Eq, Hash, PartialEq)]
pub struct ModuleModifiersSyntax {
    source: SourceSnapshot,
    node: GreenNode,
    start: TextSize,
}

impl ModuleModifiersSyntax {
    /// Creates a builder for a module-modifiers node at `start`.
    pub fn builder(source: SourceSnapshot, start: TextSize) -> ModuleModifiersSyntaxBuilder {
        ModuleModifiersSyntaxBuilder::new(source, start)
    }

    pub(crate) fn from_green(source: SourceSnapshot, node: GreenNode, start: TextSize) -> Self {
        assert_eq!(node.kind(), SyntaxKind::ModuleModifiers);

        Self {
            source,
            node,
            start,
        }
    }

    pub(crate) fn into_green(self) -> GreenNode {
        self.node
    }

    fn from_builder(builder: ModuleModifiersSyntaxBuilder) -> Self {
        Self {
            source: builder.source.into_value(),
            node: builder.node.build(SyntaxKind::ModuleModifiers),
            start: builder.start,
        }
    }

    /// Returns the full source text range covered by this node.
    pub fn full_range(&self) -> TextRange {
        SyntaxNode::full_range(self)
    }

    /// Returns whether this node contains parser recovery.
    pub fn is_recovered(&self) -> bool {
        contains_recovery(&self.node, self.start)
    }

    /// Returns the optional `trusted` modifier token.
    pub fn trusted_token(&self) -> Option<SyntaxToken> {
        first_token(&self.node, self.start, SyntaxKind::TrustedKeyword)
    }

    /// Returns the optional visibility modifier token.
    pub fn visibility_token(&self) -> Option<SyntaxToken> {
        self.node
            .syntax_tokens(self.start)
            .find(|token| visibility_modifier_kinds().contains(&token.kind()))
    }

    /// Returns descendant syntax tokens in source order.
    pub fn tokens(&self) -> impl Iterator<Item = SyntaxToken> + '_ {
        self.node.syntax_tokens(self.start)
    }
}

impl fmt::Debug for ModuleModifiersSyntax {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ModuleModifiersSyntax")
            .field("source", &self.source)
            .field("kind", &SyntaxNode::kind(self))
            .field("full_range", &self.full_range())
            .field("is_recovered", &self.is_recovered())
            .finish()
    }
}

impl GreenSyntaxNode for ModuleModifiersSyntax {
    fn green_node(&self) -> &GreenNode {
        &self.node
    }

    fn start(&self) -> TextSize {
        self.start
    }

    fn range_description(&self) -> &'static str {
        "module-modifiers"
    }

    fn is_recovered(&self) -> bool {
        ModuleModifiersSyntax::is_recovered(self)
    }
}

impl GreenSourceSyntaxNode for ModuleModifiersSyntax {
    fn source(&self) -> &SourceSnapshot {
        &self.source
    }
}

/// Builder for a module-modifiers node.
pub struct ModuleModifiersSyntaxBuilder {
    source: RequiredSyntaxSlot<SourceSnapshot>,
    node: GreenNodeBuilder,
    start: TextSize,
}

impl ModuleModifiersSyntaxBuilder {
    /// Creates an empty module-modifiers builder at `start`.
    pub fn new(source: SourceSnapshot, start: TextSize) -> Self {
        let mut source_slot = RequiredSyntaxSlot::new("module_modifiers.source");

        source_slot.set(source);

        Self {
            source: source_slot,
            node: GreenNodeBuilder::new(),
            start,
        }
    }

    /// Appends the optional trusted modifier token.
    pub fn push_trusted_token(&mut self, token: SyntaxToken) {
        require_token_kind(
            token.kind(),
            SyntaxKind::TrustedKeyword,
            "module_modifiers.trusted_token",
        );

        self.node.push_token(token);
    }

    /// Appends the optional visibility modifier token.
    pub fn push_visibility_token(&mut self, token: SyntaxToken) {
        assert!(
            visibility_modifier_kinds().contains(&token.kind()),
            "module_modifiers.visibility_token expected a visibility modifier"
        );

        self.node.push_token(token);
    }

    /// Builds the module-modifiers node.
    pub fn build(self) -> ModuleModifiersSyntax {
        ModuleModifiersSyntax::from_builder(self)
    }
}

impl fmt::Debug for ModuleModifiersSyntaxBuilder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ModuleModifiersSyntaxBuilder")
            .field("start", &self.start)
            .finish_non_exhaustive()
    }
}

fn visibility_modifier_kinds() -> &'static [SyntaxKind] {
    &[SyntaxKind::PublicKeyword, SyntaxKind::InternalKeyword]
}
