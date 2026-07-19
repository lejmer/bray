use std::fmt::{self, Write};
use std::slice;
use std::sync::Arc;

use bray_base::shared_slice;
use bray_source::{TextRange, TextSize};

use crate::{SyntaxKind, SyntaxToken, SyntaxTokenPresence, SyntaxTrivia};

/// Canonical immutable syntax tree node.
///
/// Green nodes are parentless and own the source-order child sequence. Typed
/// syntax nodes wrap green nodes.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct GreenNode {
    data: Arc<GreenNodeData>,
}

#[derive(Debug, Eq, Hash, PartialEq)]
struct GreenNodeData {
    kind: SyntaxKind,
    full_width: TextSize,
    children: Arc<[GreenElement]>,
}

impl GreenNode {
    /// Creates a green node from source-order children.
    pub(crate) fn new(kind: SyntaxKind, children: impl IntoIterator<Item = GreenElement>) -> Self {
        assert!(kind.is_node(), "green node kind must be a node");

        let children = shared_slice(children);
        let full_width = full_width_for_children(&children);

        Self {
            data: Arc::new(GreenNodeData {
                kind,
                full_width,
                children,
            }),
        }
    }

    /// Returns this node's stable syntax kind.
    pub(crate) fn kind(&self) -> SyntaxKind {
        self.data.kind
    }

    /// Returns the full source width covered by this node and its children.
    pub(crate) fn full_width(&self) -> TextSize {
        self.data.full_width
    }

    /// Returns this node's source-order children.
    pub(crate) fn children(&self) -> &[GreenElement] {
        &self.data.children
    }

    /// Returns descendant syntax tokens in source order.
    pub(crate) fn syntax_tokens(&self, start: TextSize) -> GreenSyntaxTokenIter<'_> {
        GreenSyntaxTokenIter::new(self.children(), start)
    }

    /// Returns the first direct child token with `kind`.
    pub(crate) fn first_child_token(
        &self,
        start: TextSize,
        kind: SyntaxKind,
    ) -> Option<SyntaxToken> {
        self.first_child_token_matching(start, |token_kind| token_kind == kind)
    }

    /// Returns the first direct child token accepted by `predicate`.
    pub(crate) fn first_child_token_matching(
        &self,
        start: TextSize,
        predicate: impl Fn(SyntaxKind) -> bool,
    ) -> Option<SyntaxToken> {
        let mut offset = start;

        for child in self.children() {
            let child_start = offset;

            offset = checked_add(child_start, child.full_width(), "next child token start");

            let GreenElement::Token(token) = child else {
                continue;
            };

            if predicate(token.kind()) {
                return Some(token.syntax_token(child_start));
            }
        }

        None
    }

    /// Returns the last descendant token kind and presence in source order.
    pub(crate) fn last_token(&self) -> Option<(SyntaxKind, SyntaxTokenPresence)> {
        last_token(self.children())
    }

    /// Returns descendant skipped-syntax nodes in source order.
    pub(crate) fn skipped_syntax_nodes(&self, start: TextSize) -> GreenSkippedSyntaxIter<'_> {
        GreenSkippedSyntaxIter::new(self.children(), start)
    }

    /// Returns whether this node contains missing tokens or skipped syntax.
    pub(crate) fn contains_recovery(&self, start: TextSize) -> bool {
        self.syntax_tokens(start).any(|token| token.is_missing())
            || self.skipped_syntax_nodes(start).next().is_some()
    }

    /// Returns direct child nodes with `kind` and their source starts.
    pub(crate) fn child_nodes(&self, start: TextSize, kind: SyntaxKind) -> GreenChildNodeIter<'_> {
        GreenChildNodeIter::new(self.children(), start, kind)
    }

    /// Appends this node's exact source text.
    pub(crate) fn write_source_text(
        &self,
        source_text: &str,
        start: TextSize,
        writer: &mut dyn Write,
    ) -> fmt::Result {
        let mut offset = start;

        for child in self.children() {
            offset = child.write_source_text(source_text, offset, writer)?;
        }

        Ok(())
    }
}

/// Green syntax tree child element.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum GreenElement {
    /// A token child.
    Token(GreenToken),
    /// A node child.
    Node(GreenNode),
}

impl GreenElement {
    /// Returns this element's full source width.
    pub(crate) fn full_width(&self) -> TextSize {
        match self {
            Self::Token(token) => token.full_width(),
            Self::Node(node) => node.full_width(),
        }
    }

    /// Appends this element's exact source text.
    pub(crate) fn write_source_text(
        &self,
        source_text: &str,
        start: TextSize,
        writer: &mut dyn Write,
    ) -> Result<TextSize, fmt::Error> {
        match self {
            Self::Token(token) => token.write_source_text(source_text, start, writer),
            Self::Node(node) => {
                node.write_source_text(source_text, start, writer)?;
                Ok(checked_add(
                    start,
                    node.full_width(),
                    "green node source text end",
                ))
            }
        }
    }
}

impl From<SyntaxToken> for GreenElement {
    fn from(token: SyntaxToken) -> Self {
        Self::Token(GreenToken::from_syntax_token(&token))
    }
}

impl From<GreenNode> for GreenElement {
    fn from(node: GreenNode) -> Self {
        Self::Node(node)
    }
}

/// Width-only token stored by green syntax nodes.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct GreenToken {
    kind: SyntaxKind,
    width: TextSize,
    presence: SyntaxTokenPresence,
    full_width: TextSize,
    leading_trivia: Arc<[GreenTrivia]>,
    trailing_trivia: Arc<[GreenTrivia]>,
}

impl GreenToken {
    fn from_syntax_token(token: &SyntaxToken) -> Self {
        let leading_trivia = shared_slice(
            token
                .leading_trivia()
                .iter()
                .map(GreenTrivia::from_syntax_trivia),
        );

        let trailing_trivia = shared_slice(
            token
                .trailing_trivia()
                .iter()
                .map(GreenTrivia::from_syntax_trivia),
        );

        let full_width =
            full_width_for_token(token.range().len(), &leading_trivia, &trailing_trivia);

        Self {
            kind: token.kind(),
            width: token.range().len(),
            presence: token.presence(),
            full_width,
            leading_trivia,
            trailing_trivia,
        }
    }

    pub(crate) fn kind(&self) -> SyntaxKind {
        self.kind
    }

    pub(crate) fn presence(&self) -> SyntaxTokenPresence {
        self.presence
    }

    fn full_width(&self) -> TextSize {
        self.full_width
    }

    pub(crate) fn syntax_token(&self, start: TextSize) -> SyntaxToken {
        let (leading_trivia, token_start) = syntax_trivia_list(&self.leading_trivia, start);
        let token_end = checked_add(token_start, self.width, "green token end");
        let (trailing_trivia, _) = syntax_trivia_list(&self.trailing_trivia, token_end);

        SyntaxToken::with_trivia_and_presence(
            self.kind,
            TextRange::new(token_start, token_end),
            self.presence,
            leading_trivia,
            trailing_trivia,
        )
    }

    fn write_source_text(
        &self,
        source_text: &str,
        start: TextSize,
        writer: &mut dyn Write,
    ) -> Result<TextSize, fmt::Error> {
        let token = self.syntax_token(start);

        token.write_full_text(source_text, writer)?;

        Ok(checked_add(
            start,
            self.full_width,
            "green token source text end",
        ))
    }
}

/// Width-only trivia stored by green syntax tokens.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct GreenTrivia {
    kind: SyntaxKind,
    width: TextSize,
}

impl GreenTrivia {
    fn from_syntax_trivia(trivia: &SyntaxTrivia) -> Self {
        Self {
            kind: trivia.kind(),
            width: trivia.range().len(),
        }
    }

    fn syntax_trivia(&self, start: TextSize) -> SyntaxTrivia {
        let end = checked_add(start, self.width, "green trivia end");

        SyntaxTrivia::new(self.kind, TextRange::new(start, end))
    }
}

/// Source-order syntax-token iterator over a green tree.
pub(crate) struct GreenSyntaxTokenIter<'green> {
    stack: Vec<GreenChildCursor<'green>>,
}

struct GreenChildCursor<'green> {
    children: slice::Iter<'green, GreenElement>,
    offset: TextSize,
}

impl<'green> GreenSyntaxTokenIter<'green> {
    fn new(children: &'green [GreenElement], start: TextSize) -> Self {
        Self {
            stack: vec![GreenChildCursor {
                children: children.iter(),
                offset: start,
            }],
        }
    }
}

impl Iterator for GreenSyntaxTokenIter<'_> {
    type Item = SyntaxToken;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let current = self.stack.last_mut()?;

            match current.children.next() {
                Some(GreenElement::Token(token)) => {
                    let start = current.offset;

                    current.offset = checked_add(start, token.full_width(), "next token start");

                    return Some(token.syntax_token(start));
                }
                Some(GreenElement::Node(node)) => {
                    let start = current.offset;

                    current.offset = checked_add(start, node.full_width(), "next node start");

                    self.stack.push(GreenChildCursor {
                        children: node.children().iter(),
                        offset: start,
                    });
                }
                None => {
                    self.stack.pop();
                }
            }
        }
    }
}

/// Source-order skipped-syntax iterator over a green tree.
pub(crate) struct GreenSkippedSyntaxIter<'green> {
    stack: Vec<GreenChildCursor<'green>>,
}

impl<'green> GreenSkippedSyntaxIter<'green> {
    fn new(children: &'green [GreenElement], start: TextSize) -> Self {
        Self {
            stack: vec![GreenChildCursor {
                children: children.iter(),
                offset: start,
            }],
        }
    }
}

impl Iterator for GreenSkippedSyntaxIter<'_> {
    type Item = (GreenNode, TextSize);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let current = self.stack.last_mut()?;

            match current.children.next() {
                Some(GreenElement::Token(token)) => {
                    current.offset =
                        checked_add(current.offset, token.full_width(), "next token start");
                }
                Some(GreenElement::Node(node)) => {
                    let start = current.offset;

                    current.offset = checked_add(start, node.full_width(), "next node start");

                    if node.kind() == SyntaxKind::SkippedSyntax {
                        return Some((share_node(node), start));
                    }

                    self.stack.push(GreenChildCursor {
                        children: node.children().iter(),
                        offset: start,
                    });
                }
                None => {
                    self.stack.pop();
                }
            }
        }
    }
}

/// Source-order direct child-node iterator over a green node.
pub(crate) struct GreenChildNodeIter<'green> {
    children: slice::Iter<'green, GreenElement>,
    offset: TextSize,
    kind: SyntaxKind,
}

impl<'green> GreenChildNodeIter<'green> {
    fn new(children: &'green [GreenElement], start: TextSize, kind: SyntaxKind) -> Self {
        Self {
            children: children.iter(),
            offset: start,
            kind,
        }
    }
}

impl Iterator for GreenChildNodeIter<'_> {
    type Item = (GreenNode, TextSize);

    fn next(&mut self) -> Option<Self::Item> {
        for child in self.children.by_ref() {
            let start = self.offset;

            self.offset = checked_add(start, child.full_width(), "next child node start");

            let GreenElement::Node(node) = child else {
                continue;
            };

            if node.kind() == self.kind {
                return Some((share_node(node), start));
            }
        }

        None
    }
}

fn full_width_for_children(children: &[GreenElement]) -> TextSize {
    let mut width = TextSize::ZERO;

    for child in children {
        width = checked_add(width, child.full_width(), "green node width");
    }

    width
}

fn last_token(children: &[GreenElement]) -> Option<(SyntaxKind, SyntaxTokenPresence)> {
    match children.last()? {
        GreenElement::Token(token) => Some((token.kind(), token.presence())),
        GreenElement::Node(node) => node.last_token(),
    }
}

fn full_width_for_token(
    width: TextSize,
    leading_trivia: &[GreenTrivia],
    trailing_trivia: &[GreenTrivia],
) -> TextSize {
    let mut full_width = width;

    for trivia in leading_trivia.iter().chain(trailing_trivia.iter()) {
        full_width = checked_add(full_width, trivia.width, "green token width");
    }

    full_width
}

fn syntax_trivia_list(trivia: &[GreenTrivia], start: TextSize) -> (Vec<SyntaxTrivia>, TextSize) {
    let mut values = Vec::with_capacity(trivia.len());
    let mut offset = start;

    for green_trivia in trivia {
        let syntax_trivia = green_trivia.syntax_trivia(offset);

        offset = syntax_trivia.end();

        values.push(syntax_trivia);
    }

    (values, offset)
}

pub(crate) fn checked_add(lhs: TextSize, rhs: TextSize, message: &'static str) -> TextSize {
    match lhs.checked_add(rhs) {
        Some(value) => value,
        None => panic!("{message} overflow"),
    }
}

fn share_node(node: &GreenNode) -> GreenNode {
    // GreenNode clones share immutable Arc-backed tree storage.
    node.clone()
}

#[cfg(test)]
mod tests {
    use bray_source::{TextRange, TextSize};

    use super::{GreenElement, GreenNode};
    use crate::test_support::func_keyword_with_trailing_space;
    use crate::{SyntaxKind, SyntaxToken, SyntaxTrivia};

    #[test]
    fn green_nodes_store_tokens_and_nodes_in_source_order() {
        let source_text = "func main";

        let func = func_keyword_with_trailing_space();

        let main = SyntaxToken::new(
            SyntaxKind::IdentifierToken,
            TextRange::new(TextSize::new(5), TextSize::new(9)),
        );

        let child = GreenNode::new(SyntaxKind::SourceUnit, [GreenElement::from(func.clone())]);

        let parent = GreenNode::new(
            SyntaxKind::SourceUnit,
            [
                GreenElement::from(child.clone()),
                GreenElement::from(main.clone()),
            ],
        );

        let tokens = parent.syntax_tokens(TextSize::ZERO).collect::<Vec<_>>();

        let mut written_text = String::new();

        match parent.write_source_text(source_text, TextSize::ZERO, &mut written_text) {
            Ok(()) => {}
            Err(error) => panic!("writing green text should succeed: {error:?}"),
        }

        assert_eq!(child.kind(), SyntaxKind::SourceUnit);
        assert_eq!(child.full_width(), TextSize::new(5));
        assert_eq!(parent.full_width(), TextSize::new(9));
        assert_eq!(tokens, [func, main]);
        assert_eq!(written_text, source_text);
    }

    #[test]
    fn green_nodes_synthesize_shifted_syntax_tokens_from_widths() {
        let token = SyntaxToken::new(
            SyntaxKind::FuncKeyword,
            TextRange::new(TextSize::new(12), TextSize::new(16)),
        )
        .with_leading_trivia([SyntaxTrivia::whitespace(TextRange::new(
            TextSize::new(10),
            TextSize::new(12),
        ))])
        .with_trailing_trivia([SyntaxTrivia::whitespace(TextRange::new(
            TextSize::new(16),
            TextSize::new(17),
        ))]);

        let node = GreenNode::new(SyntaxKind::SourceUnit, [GreenElement::from(token)]);
        let shifted = node.syntax_tokens(TextSize::new(20)).collect::<Vec<_>>();

        assert_eq!(shifted.len(), 1);

        assert_eq!(
            shifted[0].full_range(),
            TextRange::new(TextSize::new(20), TextSize::new(27))
        );

        assert_eq!(
            shifted[0].range(),
            TextRange::new(TextSize::new(22), TextSize::new(26))
        );
    }

    #[test]
    fn green_nodes_preserve_missing_token_presence() {
        let token = SyntaxToken::missing(SyntaxKind::SemicolonToken, TextSize::new(4));
        let node = GreenNode::new(SyntaxKind::SourceUnit, [GreenElement::from(token.clone())]);
        let tokens = node.syntax_tokens(TextSize::new(4)).collect::<Vec<_>>();

        assert_eq!(tokens, [token]);
    }

    #[test]
    fn green_nodes_find_skipped_syntax_nodes() {
        let skipped = GreenNode::new(
            SyntaxKind::SkippedSyntax,
            [GreenElement::from(SyntaxToken::invalid(TextRange::new(
                TextSize::new(1),
                TextSize::new(2),
            )))],
        );

        let parent = GreenNode::new(
            SyntaxKind::SourceUnit,
            [
                GreenElement::from(SyntaxToken::new(
                    SyntaxKind::FuncKeyword,
                    TextRange::new(TextSize::ZERO, TextSize::new(1)),
                )),
                GreenElement::from(skipped.clone()),
            ],
        );

        let skipped_nodes = parent
            .skipped_syntax_nodes(TextSize::ZERO)
            .collect::<Vec<_>>();

        assert_eq!(skipped_nodes, [(skipped, TextSize::new(1))]);
    }

    #[test]
    fn green_nodes_find_direct_child_nodes_by_kind() {
        let first = GreenNode::new(
            SyntaxKind::IdentifierListItem,
            [GreenElement::from(SyntaxToken::new(
                SyntaxKind::IdentifierToken,
                TextRange::new(TextSize::ZERO, TextSize::new(1)),
            ))],
        );

        let skipped = GreenNode::new(
            SyntaxKind::SkippedSyntax,
            [GreenElement::from(SyntaxToken::invalid(TextRange::new(
                TextSize::new(1),
                TextSize::new(2),
            )))],
        );

        let second = GreenNode::new(
            SyntaxKind::IdentifierListItem,
            [GreenElement::from(SyntaxToken::new(
                SyntaxKind::IdentifierToken,
                TextRange::new(TextSize::new(2), TextSize::new(3)),
            ))],
        );

        let parent = GreenNode::new(
            SyntaxKind::IdentifierList,
            [
                GreenElement::from(first.clone()),
                GreenElement::from(skipped),
                GreenElement::from(second.clone()),
            ],
        );

        let children = parent
            .child_nodes(TextSize::ZERO, SyntaxKind::IdentifierListItem)
            .collect::<Vec<_>>();

        assert_eq!(
            children,
            [(first, TextSize::ZERO), (second, TextSize::new(2))]
        );
    }

    #[test]
    fn green_nodes_find_direct_child_tokens_without_descending_into_child_nodes() {
        let child = GreenNode::new(
            SyntaxKind::SourceUnit,
            [GreenElement::from(SyntaxToken::new(
                SyntaxKind::IdentifierToken,
                TextRange::new(TextSize::ZERO, TextSize::new(3)),
            ))],
        );

        let direct = SyntaxToken::new(
            SyntaxKind::IdentifierToken,
            TextRange::new(TextSize::new(3), TextSize::new(7)),
        );

        let parent = GreenNode::new(
            SyntaxKind::SourceUnit,
            [
                GreenElement::from(child),
                GreenElement::from(direct.clone()),
            ],
        );

        assert_eq!(
            parent.first_child_token(TextSize::ZERO, SyntaxKind::IdentifierToken),
            Some(direct)
        );
    }

    #[test]
    fn green_nodes_are_send_and_sync() {
        assert_send_sync::<GreenElement>();
        assert_send_sync::<GreenNode>();
    }

    fn assert_send_sync<T: Send + Sync>() {}
}
