use std::fmt::{self, Write};
use std::slice;

use bray_source::{SourceSnapshot, TextRange, TextSize};

use crate::green::{GreenElement, GreenNode, checked_add};
use crate::node::{GreenSourceSyntaxNode, GreenSyntaxNode, full_range_from_width};
use crate::{SourceSyntaxNode, SourceUnitSyntax, SyntaxKind, SyntaxNode, SyntaxToken, SyntaxTree};

/// Opaque view of a source syntax node during traversal.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct SyntaxNodeView<'syntax> {
    source: &'syntax SourceSnapshot,
    node: &'syntax GreenNode,
    start: TextSize,
}

impl<'syntax> SyntaxNodeView<'syntax> {
    pub(crate) fn new(
        source: &'syntax SourceSnapshot,
        node: &'syntax GreenNode,
        start: TextSize,
    ) -> Self {
        Self {
            source,
            node,
            start,
        }
    }

    /// Returns this node's stable syntax kind.
    pub fn kind(&self) -> SyntaxKind {
        self.node.kind()
    }

    /// Returns the immutable source snapshot this node was parsed from.
    pub const fn source(&self) -> &'syntax SourceSnapshot {
        self.source
    }

    /// Returns the source offset where this node starts.
    pub const fn start(&self) -> TextSize {
        self.start
    }

    /// Returns the full source byte range covered by this node.
    pub fn full_range(&self) -> TextRange {
        full_range_from_width(self.start, self.node.full_width(), "syntax node view")
    }

    /// Returns whether this node contains explicit parser recovery.
    pub fn is_recovered(&self) -> bool {
        self.kind() == SyntaxKind::SkippedSyntax || self.node.contains_recovery(self.start)
    }

    /// Returns descendant syntax tokens in source order.
    pub fn tokens(&self) -> impl Iterator<Item = SyntaxToken> + 'syntax {
        self.node.syntax_tokens(self.start)
    }

    /// Casts this generic node view to a concrete typed syntax node.
    pub fn cast<T>(self) -> Option<T>
    where
        T: SyntaxCast,
    {
        T::cast_from(self)
    }

    pub(crate) const fn green_node(&self) -> &'syntax GreenNode {
        self.node
    }
}

impl fmt::Debug for SyntaxNodeView<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SyntaxNodeView")
            .field("source", self.source)
            .field("kind", &self.kind())
            .field("full_range", &self.full_range())
            .field("is_recovered", &self.is_recovered())
            .finish()
    }
}

impl SyntaxNode for SyntaxNodeView<'_> {
    fn kind(&self) -> SyntaxKind {
        SyntaxNodeView::kind(self)
    }

    fn full_range(&self) -> TextRange {
        SyntaxNodeView::full_range(self)
    }

    fn is_recovered(&self) -> bool {
        SyntaxNodeView::is_recovered(self)
    }
}

impl SourceSyntaxNode for SyntaxNodeView<'_> {
    fn source(&self) -> &SourceSnapshot {
        self.source
    }

    fn write_full_text_from(&self, source_text: &str, writer: &mut dyn Write) -> fmt::Result {
        self.node.write_source_text(source_text, self.start, writer)
    }
}

mod sealed {
    use crate::SyntaxNodeView;

    pub trait SyntaxWalkRoot {
        fn syntax_walk_root(&self) -> SyntaxNodeView<'_>;
    }
}

impl<T> sealed::SyntaxWalkRoot for T
where
    T: GreenSourceSyntaxNode,
{
    fn syntax_walk_root(&self) -> SyntaxNodeView<'_> {
        SyntaxNodeView::new(self.source(), self.green_node(), self.start())
    }
}

impl sealed::SyntaxWalkRoot for SyntaxNodeView<'_> {
    fn syntax_walk_root(&self) -> SyntaxNodeView<'_> {
        *self
    }
}

/// A Bray-owned typed syntax node or opaque node view that can be traversed.
///
/// This trait is sealed because traversal requires access to Bray's immutable
/// green syntax storage. It is exported only as the bound of
/// [`walk_syntax_node`].
pub trait SyntaxWalkRoot: sealed::SyntaxWalkRoot {}

impl<T> SyntaxWalkRoot for T where T: sealed::SyntaxWalkRoot {}

/// Concrete typed syntax node cast from a generic syntax node view.
pub trait SyntaxCast: SourceSyntaxNode + Sized {
    /// Stable syntax kind accepted by this cast.
    const KIND: SyntaxKind;

    /// Casts `view` to this typed syntax node when its kind matches.
    fn cast_from(view: SyntaxNodeView<'_>) -> Option<Self>;
}

/// Source-order syntax traversal event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SyntaxWalkEvent<'syntax> {
    /// Traversal is entering a syntax node.
    EnterNode(SyntaxNodeView<'syntax>),
    /// Traversal reached a syntax token.
    Token(SyntaxToken),
    /// Traversal is exiting a syntax node.
    ExitNode(SyntaxNodeView<'syntax>),
}

/// Control decision returned by syntax walk visitors.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyntaxWalkControl {
    /// Continue walking normally.
    Continue,
    /// Skip the current node's children.
    ///
    /// This only affects `EnterNode` events. The walker still emits the
    /// matching `ExitNode` event.
    SkipChildren,
    /// Stop traversal immediately.
    Stop,
}

/// Walks every source unit in a syntax tree in source-unit order.
///
/// This is serial convenience for callers that do not own scheduling policy.
/// Parallel compiler phases should usually schedule independent source-unit
/// tasks and call [`walk_source_unit`] inside each task.
pub fn walk_syntax_tree(
    tree: &SyntaxTree,
    mut visitor: impl for<'syntax> FnMut(SyntaxWalkEvent<'syntax>) -> SyntaxWalkControl,
) {
    for source_unit in tree.source_units() {
        if walk_source_unit_inner(source_unit, &mut visitor) {
            return;
        }
    }
}

/// Walks one source unit in source order.
///
/// This is the preferred syntax traversal primitive for scheduler-owned
/// source-unit tasks. Keep visitor state task-local and merge task output
/// deterministically in the owning compiler phase.
pub fn walk_source_unit(
    source_unit: &SourceUnitSyntax,
    mut visitor: impl for<'syntax> FnMut(SyntaxWalkEvent<'syntax>) -> SyntaxWalkControl,
) {
    walk_source_unit_inner(source_unit, &mut visitor);
}

/// Walks one typed source syntax node in source order.
pub fn walk_syntax_node(
    node: &impl SyntaxWalkRoot,
    mut visitor: impl for<'syntax> FnMut(SyntaxWalkEvent<'syntax>) -> SyntaxWalkControl,
) {
    walk_node(node.syntax_walk_root(), &mut visitor);
}

pub(crate) fn cast_source_node<T>(
    view: SyntaxNodeView<'_>,
    kind: SyntaxKind,
    from_green: impl FnOnce(SourceSnapshot, GreenNode, TextSize) -> T,
) -> Option<T> {
    if view.kind() != kind {
        return None;
    }

    // SourceSnapshot and GreenNode clones share immutable source and tree storage.
    Some(from_green(
        view.source().clone(),
        view.green_node().clone(),
        view.start(),
    ))
}

fn walk_source_unit_inner<'syntax>(
    source_unit: &'syntax SourceUnitSyntax,
    visitor: &mut impl FnMut(SyntaxWalkEvent<'syntax>) -> SyntaxWalkControl,
) -> bool {
    let root = SyntaxNodeView::new(
        GreenSourceSyntaxNode::source(source_unit),
        source_unit.green_node(),
        source_unit.start(),
    );

    walk_node(root, visitor)
}

fn walk_node<'syntax>(
    root: SyntaxNodeView<'syntax>,
    visitor: &mut impl FnMut(SyntaxWalkEvent<'syntax>) -> SyntaxWalkControl,
) -> bool {
    match visitor(SyntaxWalkEvent::EnterNode(root)) {
        SyntaxWalkControl::Continue => {}
        SyntaxWalkControl::SkipChildren => {
            return emit_exit(root, visitor);
        }
        SyntaxWalkControl::Stop => return true,
    }

    let mut stack = vec![WalkCursor::new(root)];

    while let Some(current) = stack.last_mut() {
        match current.children.next() {
            Some(child) => {
                let child_start = current.offset;

                current.offset = checked_add(child_start, child.full_width(), "next walk child");

                if walk_child(
                    child,
                    child_start,
                    current.node.source(),
                    visitor,
                    &mut stack,
                ) {
                    return true;
                }
            }
            None => {
                let node = current.node;

                stack.pop();

                if emit_exit(node, visitor) {
                    return true;
                }
            }
        }
    }

    false
}

fn walk_child<'syntax>(
    child: &'syntax GreenElement,
    start: TextSize,
    source: &'syntax SourceSnapshot,
    visitor: &mut impl FnMut(SyntaxWalkEvent<'syntax>) -> SyntaxWalkControl,
    stack: &mut Vec<WalkCursor<'syntax>>,
) -> bool {
    match child {
        GreenElement::Token(token) => {
            matches!(
                visitor(SyntaxWalkEvent::Token(token.syntax_token(start))),
                SyntaxWalkControl::Stop
            )
        }
        GreenElement::Node(node) => {
            let view = SyntaxNodeView::new(source, node, start);

            match visitor(SyntaxWalkEvent::EnterNode(view)) {
                SyntaxWalkControl::Continue => {
                    stack.push(WalkCursor::new(view));

                    false
                }
                SyntaxWalkControl::SkipChildren => emit_exit(view, visitor),
                SyntaxWalkControl::Stop => true,
            }
        }
    }
}

fn emit_exit<'syntax>(
    node: SyntaxNodeView<'syntax>,
    visitor: &mut impl FnMut(SyntaxWalkEvent<'syntax>) -> SyntaxWalkControl,
) -> bool {
    matches!(
        visitor(SyntaxWalkEvent::ExitNode(node)),
        SyntaxWalkControl::Stop
    )
}

struct WalkCursor<'syntax> {
    node: SyntaxNodeView<'syntax>,
    children: slice::Iter<'syntax, GreenElement>,
    offset: TextSize,
}

impl<'syntax> WalkCursor<'syntax> {
    fn new(node: SyntaxNodeView<'syntax>) -> Self {
        Self {
            node,
            children: node.green_node().children().iter(),
            offset: node.start(),
        }
    }
}

#[cfg(test)]
mod tests {
    use bray_source::{SourceOrigin, TextRange, TextSize};

    use super::{
        SyntaxNodeView, SyntaxWalkControl, SyntaxWalkEvent, walk_source_unit, walk_syntax_node,
        walk_syntax_tree,
    };
    use crate::test_support::{snapshot, token};
    use crate::{
        IdentifierListItemSyntax, IdentifierListSyntax, SourceUnitSyntax, SyntaxKind, SyntaxText,
        SyntaxToken, SyntaxTree,
    };

    #[test]
    fn source_unit_walks_nodes_and_tokens_in_source_order() {
        let source_unit = identifier_list_source_unit();
        let mut events = Vec::new();

        walk_source_unit(&source_unit, |event| {
            events.push(event_name(event));

            SyntaxWalkControl::Continue
        });

        assert_eq!(
            events,
            [
                "enter source_unit",
                "enter identifier_list",
                "enter identifier_list_item",
                "token identifier_token",
                "exit identifier_list_item",
                "token comma_token",
                "enter identifier_list_item",
                "token identifier_token",
                "exit identifier_list_item",
                "exit identifier_list",
                "token end_of_file_token",
                "exit source_unit",
            ]
        );
    }

    #[test]
    fn source_unit_walk_can_skip_node_children() {
        let source_unit = identifier_list_source_unit();
        let mut events = Vec::new();

        walk_source_unit(&source_unit, |event| {
            let name = event_name(event.clone());
            let control = match event {
                SyntaxWalkEvent::EnterNode(node) if node.kind() == SyntaxKind::IdentifierList => {
                    SyntaxWalkControl::SkipChildren
                }
                _ => SyntaxWalkControl::Continue,
            };

            events.push(name);

            control
        });

        assert_eq!(
            events,
            [
                "enter source_unit",
                "enter identifier_list",
                "exit identifier_list",
                "token end_of_file_token",
                "exit source_unit",
            ]
        );
    }

    #[test]
    fn source_unit_walk_can_stop_early() {
        let source_unit = identifier_list_source_unit();
        let mut events = Vec::new();

        walk_source_unit(&source_unit, |event| {
            let name = event_name(event.clone());

            let control = match event {
                SyntaxWalkEvent::Token(token) if token.kind() == SyntaxKind::CommaToken => {
                    SyntaxWalkControl::Stop
                }
                _ => SyntaxWalkControl::Continue,
            };

            events.push(name);

            control
        });

        assert_eq!(
            events,
            [
                "enter source_unit",
                "enter identifier_list",
                "enter identifier_list_item",
                "token identifier_token",
                "exit identifier_list_item",
                "token comma_token",
            ]
        );
    }

    #[test]
    fn typed_node_walk_preserves_its_nested_hierarchy() {
        let source_unit = identifier_list_source_unit();
        let mut lists = source_unit.identifier_lists();

        let Some(list) = lists.next() else {
            panic!("test source unit should contain an identifier list");
        };

        let mut events = Vec::new();

        walk_syntax_node(&list, |event| {
            events.push(event_name(event));

            SyntaxWalkControl::Continue
        });

        assert_eq!(
            events.first().map(String::as_str),
            Some("enter identifier_list")
        );
        assert!(events.contains(&"enter identifier_list_item".to_owned()));
        assert_eq!(
            events.last().map(String::as_str),
            Some("exit identifier_list")
        );
    }

    #[test]
    fn node_views_cast_to_typed_syntax_nodes() {
        let source_unit = identifier_list_source_unit();
        let mut list_text = None;

        walk_source_unit(&source_unit, |event| {
            if let SyntaxWalkEvent::EnterNode(node) = event
                && let Some(list) = node.cast::<IdentifierListSyntax>()
            {
                list_text = Some(list.full_text());
            }

            SyntaxWalkControl::Continue
        });

        assert_eq!(list_text.as_deref(), Some("a,b"));
    }

    #[test]
    fn source_tree_walks_source_units_in_order() {
        let first = eof_source_unit("first");
        let second = eof_source_unit("second");
        let tree = SyntaxTree::compilation_unit([first, second]);

        let mut source_names = Vec::new();

        walk_syntax_tree(&tree, |event| {
            if let SyntaxWalkEvent::EnterNode(node) = event
                && node.kind() == SyntaxKind::SourceUnit
            {
                match node.source().origin() {
                    SourceOrigin::Virtual { name } => source_names.push(name.clone()),
                    origin => panic!("expected virtual test source: {origin:?}"),
                }
            }

            SyntaxWalkControl::SkipChildren
        });

        assert_eq!(source_names, ["first", "second"]);
    }

    #[test]
    fn skipped_syntax_walks_as_recovered_node() {
        let source = snapshot("syntax-walk-test", "$");
        let skipped = token(SyntaxKind::InvalidToken, 0, 1);

        let source_unit = SourceUnitSyntax::builder(source)
            .skipped_tokens([skipped])
            .tokens([SyntaxToken::end_of_file(TextSize::new(1))])
            .build();

        let mut recovered_ranges = Vec::new();

        walk_source_unit(&source_unit, |event| {
            if let SyntaxWalkEvent::EnterNode(node) = event
                && node.is_recovered()
            {
                recovered_ranges.push((node.kind(), node.full_range()));
            }

            SyntaxWalkControl::Continue
        });

        assert_eq!(
            recovered_ranges,
            [
                (
                    SyntaxKind::SourceUnit,
                    TextRange::new(TextSize::ZERO, TextSize::new(1))
                ),
                (
                    SyntaxKind::SkippedSyntax,
                    TextRange::new(TextSize::ZERO, TextSize::new(1)),
                ),
            ]
        );
    }

    #[test]
    fn syntax_walk_public_values_are_send_and_sync() {
        assert_send_sync::<SyntaxNodeView<'static>>();
        assert_send_sync::<SyntaxWalkEvent<'static>>();
        assert_send_sync::<SyntaxWalkControl>();
    }

    fn identifier_list_source_unit() -> SourceUnitSyntax {
        let source = snapshot("syntax-walk-test", "a,b");

        let first = IdentifierListItemSyntax::builder(source.clone())
            .identifier_token(token(SyntaxKind::IdentifierToken, 0, 1))
            .build();

        let second = IdentifierListItemSyntax::builder(source.clone())
            .identifier_token(token(SyntaxKind::IdentifierToken, 2, 3))
            .build();

        let mut list = IdentifierListSyntax::builder(source.clone(), TextSize::ZERO);

        list.push_item(first);
        list.push_separator_token(token(SyntaxKind::CommaToken, 1, 2));
        list.push_item(second);

        SourceUnitSyntax::builder(source)
            .identifier_list(list.build())
            .tokens([SyntaxToken::end_of_file(TextSize::new(3))])
            .build()
    }

    fn eof_source_unit(source_name: &'static str) -> SourceUnitSyntax {
        let source = snapshot(source_name, "");

        SourceUnitSyntax::builder(source)
            .tokens([SyntaxToken::end_of_file(TextSize::ZERO)])
            .build()
    }

    fn event_name(event: SyntaxWalkEvent<'_>) -> String {
        match event {
            SyntaxWalkEvent::EnterNode(node) => format!("enter {}", node.kind().as_str()),
            SyntaxWalkEvent::Token(token) => format!("token {}", token.kind().as_str()),
            SyntaxWalkEvent::ExitNode(node) => format!("exit {}", node.kind().as_str()),
        }
    }

    fn assert_send_sync<T: Send + Sync>() {}
}
