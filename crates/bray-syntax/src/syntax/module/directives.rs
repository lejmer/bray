use std::fmt;

use bray_source::{SourceSnapshot, TextSize};

use crate::builder::RequiredSyntaxSlot;
use crate::green::GreenNode;
use crate::list::{SyntaxList, SyntaxListBuilder};
use crate::node::{GreenSourceSyntaxNode, GreenSyntaxNode};
use crate::syntax::directive::{LinkDirectiveSyntax, TargetDirectiveSyntax, TestDirectiveSyntax};
use crate::{SkippedSyntax, SyntaxKind, SyntaxNode, SyntaxToken};

/// Module directives in source order.
#[derive(Clone, Eq, Hash, PartialEq)]
pub struct ModuleDirectivesSyntax {
    list: SyntaxList,
}

impl ModuleDirectivesSyntax {
    /// Creates a builder for module directives at `start`.
    pub fn builder(source: SourceSnapshot, start: TextSize) -> ModuleDirectivesSyntaxBuilder {
        ModuleDirectivesSyntaxBuilder::new(source, start)
    }

    pub(crate) fn from_green(source: SourceSnapshot, node: GreenNode, start: TextSize) -> Self {
        Self {
            list: SyntaxList::from_green(source, node, start, SyntaxKind::ModuleDirectives),
        }
    }

    pub(crate) fn into_green(self) -> GreenNode {
        self.list.into_green()
    }

    fn from_builder(builder: ModuleDirectivesSyntaxBuilder) -> Self {
        Self {
            list: SyntaxList::from_green(
                builder.source.into_value(),
                builder.list.build(),
                builder.start,
                SyntaxKind::ModuleDirectives,
            ),
        }
    }

    /// Returns the full source text range covered by this directive group.
    pub fn full_range(&self) -> bray_source::TextRange {
        SyntaxNode::full_range(self)
    }

    /// Returns whether this directive group contains parser recovery.
    pub fn is_recovered(&self) -> bool {
        self.list.is_recovered()
    }

    /// Returns `@target(...)` module directives in source order.
    pub fn target_directives(&self) -> impl Iterator<Item = TargetDirectiveSyntax> + '_ {
        self.list
            .child_nodes(SyntaxKind::TargetDirective)
            .map(|(node, start)| {
                TargetDirectiveSyntax::from_green(self.child_source(), node, start)
            })
    }

    /// Returns `@test` module directives in source order.
    pub fn test_directives(&self) -> impl Iterator<Item = TestDirectiveSyntax> + '_ {
        self.list
            .child_nodes(SyntaxKind::TestDirective)
            .map(|(node, start)| TestDirectiveSyntax::from_green(self.child_source(), node, start))
    }

    /// Returns `@link(...)` module directives in source order.
    pub fn link_directives(&self) -> impl Iterator<Item = LinkDirectiveSyntax> + '_ {
        self.list
            .child_nodes(SyntaxKind::LinkDirective)
            .map(|(node, start)| LinkDirectiveSyntax::from_green(self.child_source(), node, start))
    }

    /// Returns descendant syntax tokens in source order.
    pub fn tokens(&self) -> impl Iterator<Item = SyntaxToken> + '_ {
        self.list.tokens()
    }

    /// Returns skipped-syntax recovery nodes in source order.
    pub fn skipped_syntax(&self) -> impl Iterator<Item = SkippedSyntax> + '_ {
        self.list.skipped_syntax()
    }

    fn child_source(&self) -> SourceSnapshot {
        // SourceSnapshot clones share immutable source text with typed child nodes.
        self.list.source().clone()
    }
}

impl fmt::Debug for ModuleDirectivesSyntax {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ModuleDirectivesSyntax")
            .field("source", &GreenSourceSyntaxNode::source(self))
            .field("kind", &SyntaxNode::kind(self))
            .field("full_range", &self.full_range())
            .field("is_recovered", &self.is_recovered())
            .finish()
    }
}

impl GreenSyntaxNode for ModuleDirectivesSyntax {
    fn green_node(&self) -> &GreenNode {
        self.list.green_node()
    }

    fn start(&self) -> TextSize {
        self.list.start()
    }

    fn range_description(&self) -> &'static str {
        "module-directives"
    }

    fn is_recovered(&self) -> bool {
        ModuleDirectivesSyntax::is_recovered(self)
    }
}

impl GreenSourceSyntaxNode for ModuleDirectivesSyntax {
    fn source(&self) -> &SourceSnapshot {
        self.list.source()
    }
}

/// Builder for module directives.
pub struct ModuleDirectivesSyntaxBuilder {
    source: RequiredSyntaxSlot<SourceSnapshot>,
    start: TextSize,
    list: SyntaxListBuilder,
}

impl ModuleDirectivesSyntaxBuilder {
    /// Creates an empty module-directives builder at `start`.
    pub fn new(source: SourceSnapshot, start: TextSize) -> Self {
        let mut source_slot = RequiredSyntaxSlot::new("module_directives.source");

        source_slot.set(source);

        Self {
            source: source_slot,
            start,
            list: SyntaxListBuilder::new(SyntaxKind::ModuleDirectives),
        }
    }

    /// Appends a `@target(...)` directive in source order.
    pub fn push_target_directive(&mut self, directive: TargetDirectiveSyntax) {
        self.list.push_item_node(directive.into_green());
    }

    /// Appends a `@test` directive in source order.
    pub fn push_test_directive(&mut self, directive: TestDirectiveSyntax) {
        self.list.push_item_node(directive.into_green());
    }

    /// Appends a `@link(...)` directive in source order.
    pub fn push_link_directive(&mut self, directive: LinkDirectiveSyntax) {
        self.list.push_item_node(directive.into_green());
    }

    /// Appends present source tokens under a skipped-syntax recovery node.
    ///
    /// Empty token lists do not add a recovery node.
    ///
    /// Panics when any skipped token is missing.
    pub fn push_skipped_tokens(&mut self, tokens: impl IntoIterator<Item = SyntaxToken>) {
        self.list.push_skipped_tokens(tokens);
    }

    /// Builds the module-directives node.
    pub fn build(self) -> ModuleDirectivesSyntax {
        ModuleDirectivesSyntax::from_builder(self)
    }
}

impl fmt::Debug for ModuleDirectivesSyntaxBuilder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ModuleDirectivesSyntaxBuilder")
            .field("start", &self.start)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use bray_source::{SourceSnapshot, TextRange, TextSize};

    use super::{ModuleDirectivesSyntax, ModuleDirectivesSyntaxBuilder};
    use crate::test_support::{snapshot as test_snapshot, token};
    use crate::{
        DirectiveArgumentListSyntax, LinkDirectiveSyntax, SyntaxKind, SyntaxText, SyntaxTrivia,
        TargetDirectiveSyntax, TestDirectiveSyntax,
    };

    // TODO(syntax): Update this when directive arguments are typed syntax.
    #[test]
    fn module_directives_preserve_mixed_directives_in_source_order() {
        let snapshot = test_snapshot(
            "syntax-module-directives-test",
            "@test @target(x) @link(\"m\")",
        );
        let test = test_directive(&snapshot);
        let target = target_directive(&snapshot);
        let link = link_directive(&snapshot);

        let directives = ModuleDirectivesSyntax::builder(snapshot, TextSize::ZERO)
            .test_directive(test)
            .target_directive(target)
            .link_directive(link)
            .build();

        assert_eq!(directives.full_text(), "@test @target(x) @link(\"m\")");
        assert_eq!(directives.test_directives().count(), 1);
        assert_eq!(directives.target_directives().count(), 1);
        assert_eq!(directives.link_directives().count(), 1);
        assert!(directives.is_recovered());
        assert_eq!(directives.skipped_syntax().count(), 2);
    }

    impl ModuleDirectivesSyntaxBuilder {
        fn test_directive(mut self, directive: TestDirectiveSyntax) -> Self {
            self.push_test_directive(directive);

            self
        }

        fn target_directive(mut self, directive: TargetDirectiveSyntax) -> Self {
            self.push_target_directive(directive);

            self
        }

        fn link_directive(mut self, directive: LinkDirectiveSyntax) -> Self {
            self.push_link_directive(directive);

            self
        }
    }

    fn test_directive(snapshot: &SourceSnapshot) -> TestDirectiveSyntax {
        let mut builder = TestDirectiveSyntax::builder(snapshot.clone(), TextSize::ZERO);

        builder.push_directive_marker_token(token(SyntaxKind::AtToken, 0, 1));
        builder.push_name_token(
            token(SyntaxKind::IdentifierToken, 1, 5).with_trailing_trivia([
                SyntaxTrivia::whitespace(TextRange::new(TextSize::new(5), TextSize::new(6))),
            ]),
        );

        builder.build()
    }

    fn target_directive(snapshot: &SourceSnapshot) -> TargetDirectiveSyntax {
        let mut builder = TargetDirectiveSyntax::builder(snapshot.clone(), TextSize::new(6));

        builder.push_directive_marker_token(token(SyntaxKind::AtToken, 6, 7));
        builder.push_name_token(token(SyntaxKind::IdentifierToken, 7, 13));
        builder.push_open_paren_token(token(SyntaxKind::OpenParenToken, 13, 14));
        builder.push_skipped_tokens([token(SyntaxKind::IdentifierToken, 14, 15)]);
        builder.push_close_paren_token(
            token(SyntaxKind::CloseParenToken, 15, 16).with_trailing_trivia([
                SyntaxTrivia::whitespace(TextRange::new(TextSize::new(16), TextSize::new(17))),
            ]),
        );

        builder.build()
    }

    fn link_directive(snapshot: &SourceSnapshot) -> LinkDirectiveSyntax {
        let mut builder = LinkDirectiveSyntax::builder(snapshot.clone(), TextSize::new(17));

        builder.push_directive_marker_token(token(SyntaxKind::AtToken, 17, 18));
        builder.push_name_token(token(SyntaxKind::IdentifierToken, 18, 22));
        builder.push_directive_argument_list(link_argument_list(snapshot));

        builder.build()
    }

    fn link_argument_list(snapshot: &SourceSnapshot) -> DirectiveArgumentListSyntax {
        let mut builder = DirectiveArgumentListSyntax::builder(snapshot.clone(), TextSize::new(22));

        builder.push_open_paren_token(token(SyntaxKind::OpenParenToken, 22, 23));
        builder.push_skipped_tokens([token(SyntaxKind::StringLiteralToken, 23, 26)]);
        builder.push_close_paren_token(token(SyntaxKind::CloseParenToken, 26, 27));

        builder.build()
    }
}
