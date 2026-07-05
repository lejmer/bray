use std::fmt::{self, Write};
use std::slice;
use std::sync::Arc;

use bray_base::shared_slice;
use bray_source::{TextRange, TextSize};

use crate::{SyntaxKind, SyntaxToken, SyntaxTrivia};

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

    /// Returns the last descendant token kind in source order.
    pub(crate) fn last_token_kind(&self) -> Option<SyntaxKind> {
        last_token_kind(self.children())
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
            full_width,
            leading_trivia,
            trailing_trivia,
        }
    }

    pub(crate) fn kind(&self) -> SyntaxKind {
        self.kind
    }

    fn full_width(&self) -> TextSize {
        self.full_width
    }

    fn syntax_token(&self, start: TextSize) -> SyntaxToken {
        let (leading_trivia, token_start) = syntax_trivia_list(&self.leading_trivia, start);
        let token_end = checked_add(token_start, self.width, "green token end");
        let (trailing_trivia, _) = syntax_trivia_list(&self.trailing_trivia, token_end);

        SyntaxToken::with_trivia(
            self.kind,
            TextRange::new(token_start, token_end),
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

fn full_width_for_children(children: &[GreenElement]) -> TextSize {
    let mut width = TextSize::ZERO;

    for child in children {
        width = checked_add(width, child.full_width(), "green node width");
    }

    width
}

fn last_token_kind(children: &[GreenElement]) -> Option<SyntaxKind> {
    match children.last()? {
        GreenElement::Token(token) => Some(token.kind()),
        GreenElement::Node(node) => node.last_token_kind(),
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

fn checked_add(lhs: TextSize, rhs: TextSize, message: &'static str) -> TextSize {
    match lhs.checked_add(rhs) {
        Some(value) => value,
        None => panic!("{message} overflow"),
    }
}

#[cfg(test)]
mod tests {
    use bray_source::{TextRange, TextSize};

    use super::{GreenElement, GreenNode};
    use crate::{SyntaxKind, SyntaxToken, SyntaxTrivia};

    #[test]
    fn green_nodes_store_tokens_and_nodes_in_source_order() {
        let source_text = "func main";

        let func = SyntaxToken::new(
            SyntaxKind::FuncKeyword,
            TextRange::new(TextSize::ZERO, TextSize::new(4)),
        )
        .with_trailing_trivia([SyntaxTrivia::whitespace(TextRange::new(
            TextSize::new(4),
            TextSize::new(5),
        ))]);

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
    fn green_nodes_are_send_and_sync() {
        assert_send_sync::<GreenElement>();
        assert_send_sync::<GreenNode>();
    }

    fn assert_send_sync<T: Send + Sync>() {}
}
