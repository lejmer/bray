use bray_source::{
    SourceId, SourceIdentity, SourceOrigin, SourceSnapshot, SourceVersion, TextRange, TextSize,
};

use crate::green::{GreenElement, GreenNode};
use crate::{SyntaxCast, SyntaxKind, SyntaxNodeView, SyntaxToken};

/// One validated source-order event in a preparsed Bray syntax fragment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreparsedSyntaxEvent<'text> {
    /// Starts one syntax node.
    EnterNode(SyntaxKind),
    /// Appends one present syntax token with its exact spelling.
    Token {
        /// The token's stable syntax kind.
        kind: SyntaxKind,
        /// The exact token text.
        text: &'text str,
    },
    /// Ends one syntax node.
    ExitNode(SyntaxKind),
}

/// An immutable typed-syntax fragment reconstructed from validated parser output.
///
/// This is intended for checked-in generated syntax data and does not parse text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparsedSyntaxFragment {
    source: SourceSnapshot,
    root: GreenNode,
}

impl PreparsedSyntaxFragment {
    /// Reconstructs one complete syntax fragment from balanced source-order events.
    pub fn try_new<'text>(
        source_id: SourceId,
        identity: SourceIdentity,
        generated_name: impl Into<String>,
        events: impl IntoIterator<Item = PreparsedSyntaxEvent<'text>>,
    ) -> Result<Self, PreparsedSyntaxFragmentError> {
        let mut text = String::new();
        let mut stack = Vec::<PendingNode>::new();
        let mut root = None;

        for event in events {
            match event {
                PreparsedSyntaxEvent::EnterNode(kind) => {
                    if !kind.is_node() {
                        return Err(PreparsedSyntaxFragmentError::ExpectedNodeKind(kind));
                    }

                    stack.push(PendingNode::new(kind));
                }
                PreparsedSyntaxEvent::Token {
                    kind,
                    text: token_text,
                } => {
                    if !kind.is_token() {
                        return Err(PreparsedSyntaxFragmentError::ExpectedTokenKind(kind));
                    }

                    let Some(node) = stack.last_mut() else {
                        return Err(PreparsedSyntaxFragmentError::TokenOutsideNode(kind));
                    };

                    let start = text_size(text.len())?;

                    text.push_str(token_text);

                    let end = text_size(text.len())?;
                    let token = SyntaxToken::new(kind, TextRange::new(start, end));

                    node.children.push(token.into());
                }
                PreparsedSyntaxEvent::ExitNode(kind) => {
                    let Some(node) = stack.pop() else {
                        return Err(PreparsedSyntaxFragmentError::UnexpectedExit(kind));
                    };

                    if node.kind != kind {
                        return Err(PreparsedSyntaxFragmentError::MismatchedExit {
                            expected: node.kind,
                            actual: kind,
                        });
                    }

                    let node = GreenNode::new(node.kind, node.children);

                    match stack.last_mut() {
                        Some(parent) => parent.children.push(node.into()),
                        None if root.is_none() => root = Some(node),
                        None => return Err(PreparsedSyntaxFragmentError::MultipleRoots),
                    }
                }
            }
        }

        if let Some(node) = stack.last() {
            return Err(PreparsedSyntaxFragmentError::UnclosedNode(node.kind));
        }

        let Some(root) = root else {
            return Err(PreparsedSyntaxFragmentError::MissingRoot);
        };

        let source = SourceSnapshot::new(
            source_id,
            identity,
            SourceOrigin::generated(generated_name),
            SourceVersion::new(0),
            text,
        )
        .map_err(|_| PreparsedSyntaxFragmentError::TextTooLarge)?;

        Ok(Self { source, root })
    }

    /// Returns the immutable synthetic source snapshot backing this fragment.
    pub const fn source(&self) -> &SourceSnapshot {
        &self.source
    }

    /// Returns an opaque view of the fragment root.
    pub fn root(&self) -> SyntaxNodeView<'_> {
        SyntaxNodeView::new(&self.source, &self.root, TextSize::ZERO)
    }

    /// Casts the fragment root to one concrete typed Bray syntax node.
    pub fn cast<T>(&self) -> Option<T>
    where
        T: SyntaxCast,
    {
        self.root().cast()
    }
}

#[derive(Debug)]
struct PendingNode {
    kind: SyntaxKind,
    children: Vec<GreenElement>,
}

impl PendingNode {
    fn new(kind: SyntaxKind) -> Self {
        Self {
            kind,
            children: Vec::new(),
        }
    }
}

/// A violated invariant in generated preparsed syntax events.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PreparsedSyntaxFragmentError {
    /// An enter event used a token kind.
    ExpectedNodeKind(SyntaxKind),
    /// A token event used a node kind.
    ExpectedTokenKind(SyntaxKind),
    /// A token appeared before a root node was opened.
    TokenOutsideNode(SyntaxKind),
    /// An exit event appeared without a matching open node.
    UnexpectedExit(SyntaxKind),
    /// An exit event did not match the active node.
    MismatchedExit {
        /// The active node kind.
        expected: SyntaxKind,
        /// The supplied exit kind.
        actual: SyntaxKind,
    },
    /// More than one complete root was supplied.
    MultipleRoots,
    /// The event stream ended with an open node.
    UnclosedNode(SyntaxKind),
    /// The event stream did not contain a root node.
    MissingRoot,
    /// The reconstructed source text exceeded Bray's source-size limit.
    TextTooLarge,
    /// A generated source ordinal could not fit its reserved identity domain.
    GeneratedSourceIdOutOfRange,
}

fn text_size(bytes: usize) -> Result<TextSize, PreparsedSyntaxFragmentError> {
    TextSize::try_from(bytes).map_err(|_| PreparsedSyntaxFragmentError::TextTooLarge)
}

#[cfg(test)]
mod tests {
    use bray_source::{SourceId, SourceIdentity, SourceOriginKind};

    use super::{PreparsedSyntaxEvent, PreparsedSyntaxFragment, PreparsedSyntaxFragmentError};
    use crate::{SourceSyntaxNode, SyntaxKind, SyntaxText, TypeExpressionSyntax};

    #[test]
    fn preparsed_events_reconstruct_typed_syntax_without_exposing_green_storage() {
        let fragment = fragment([
            PreparsedSyntaxEvent::EnterNode(SyntaxKind::TypeExpression),
            PreparsedSyntaxEvent::EnterNode(SyntaxKind::Path),
            PreparsedSyntaxEvent::Token {
                kind: SyntaxKind::IdentifierToken,
                text: "bool",
            },
            PreparsedSyntaxEvent::ExitNode(SyntaxKind::Path),
            PreparsedSyntaxEvent::ExitNode(SyntaxKind::TypeExpression),
        ]);

        let Some(ty) = fragment.cast::<TypeExpressionSyntax>() else {
            panic!("fragment root should retain its typed syntax kind");
        };

        assert_eq!(fragment.source().text(), "bool");

        assert_eq!(
            fragment.source().origin().kind(),
            SourceOriginKind::Generated
        );

        assert_eq!(ty.full_text(), "bool");
        assert_eq!(ty.source(), fragment.source());
    }

    #[test]
    fn preparsed_events_reject_unbalanced_nodes() {
        let result = PreparsedSyntaxFragment::try_new(
            SourceId::new(0),
            SourceIdentity::new(0),
            "preparsed-syntax-test",
            [
                PreparsedSyntaxEvent::EnterNode(SyntaxKind::Path),
                PreparsedSyntaxEvent::ExitNode(SyntaxKind::TypeExpression),
            ],
        );

        assert_eq!(
            result,
            Err(PreparsedSyntaxFragmentError::MismatchedExit {
                expected: SyntaxKind::Path,
                actual: SyntaxKind::TypeExpression,
            })
        );
    }

    fn fragment<'text>(
        events: impl IntoIterator<Item = PreparsedSyntaxEvent<'text>>,
    ) -> PreparsedSyntaxFragment {
        match PreparsedSyntaxFragment::try_new(
            SourceId::new(0),
            SourceIdentity::new(0),
            "preparsed-syntax-test",
            events,
        ) {
            Ok(fragment) => fragment,
            Err(error) => panic!("valid preparsed events should reconstruct: {error:?}"),
        }
    }
}
