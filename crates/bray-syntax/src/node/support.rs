use bray_source::{SourceSnapshot, TextSize};

use crate::green::GreenNode;
use crate::syntax::skipped_syntax_nodes;
use crate::{SkippedSyntax, SyntaxKind, SyntaxToken};

pub(crate) fn first_token(
    node: &GreenNode,
    start: TextSize,
    kind: SyntaxKind,
) -> Option<SyntaxToken> {
    node.first_child_token(start, kind)
}

pub(crate) fn required_token(
    node: &GreenNode,
    start: TextSize,
    kind: SyntaxKind,
    node_name: &'static str,
) -> SyntaxToken {
    match first_token(node, start, kind) {
        Some(token) => token,
        None => panic!("{node_name} must contain {}", kind.as_str()),
    }
}

pub(crate) fn required_child_node<T>(
    source: &SourceSnapshot,
    node: &GreenNode,
    start: TextSize,
    kind: SyntaxKind,
    wrap: impl FnOnce(SourceSnapshot, GreenNode, TextSize) -> T,
    node_name: &'static str,
) -> T {
    match node.child_nodes(start, kind).next() {
        Some((child, child_start)) => {
            // SourceSnapshot clones share immutable source text with typed child nodes.
            wrap(source.clone(), child, child_start)
        }
        None => panic!("{node_name} must contain {}", kind.as_str()),
    }
}

pub(crate) fn child_nodes<'syntax, T>(
    source: &'syntax SourceSnapshot,
    node: &'syntax GreenNode,
    start: TextSize,
    kind: SyntaxKind,
    wrap: impl Fn(SourceSnapshot, GreenNode, TextSize) -> T + 'syntax,
) -> impl Iterator<Item = T> + 'syntax {
    node.child_nodes(start, kind).map(move |(node, start)| {
        // SourceSnapshot clones share immutable source text with typed child nodes.
        wrap(source.clone(), node, start)
    })
}

pub(crate) fn skipped_syntax<'syntax>(
    source: &'syntax SourceSnapshot,
    node: &'syntax GreenNode,
    start: TextSize,
) -> impl Iterator<Item = SkippedSyntax> + 'syntax {
    skipped_syntax_nodes(source, node, start)
}

pub(crate) fn contains_recovery(node: &GreenNode, start: TextSize) -> bool {
    node.contains_recovery(start)
}
