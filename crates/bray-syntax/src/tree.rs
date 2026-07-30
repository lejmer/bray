use std::collections::HashMap;
use std::fmt::{self, Write};
use std::hash::{Hash, Hasher};
use std::sync::{Arc, OnceLock};

use bray_source::{SourceId, SourceSnapshot, TextRange, TextSize};

use crate::green::GreenNode;
use crate::{
    CompilationUnitSyntax, SourceUnitSyntax, SyntaxKind, SyntaxNodeView, SyntaxText,
    SyntaxWalkControl, SyntaxWalkEvent, walk_syntax_node,
};

/// Immutable syntax tree rooted at a compilation unit.
#[derive(Clone)]
pub struct SyntaxTree {
    root: CompilationUnitSyntax,
    nodes: OnceLock<Arc<HashMap<SyntaxNodeKey, IndexedSyntaxNode>>>,
}

impl SyntaxTree {
    /// Creates a syntax tree from a compilation-unit root node.
    pub fn new(root: CompilationUnitSyntax) -> Self {
        Self {
            root,
            nodes: OnceLock::new(),
        }
    }

    /// Creates a syntax tree from source-unit child nodes.
    pub fn compilation_unit(source_units: impl IntoIterator<Item = SourceUnitSyntax>) -> Self {
        let root = CompilationUnitSyntax::builder()
            .source_units(source_units)
            .build();

        Self::new(root)
    }

    /// Returns the root compilation-unit node.
    pub const fn root(&self) -> &CompilationUnitSyntax {
        &self.root
    }

    /// Returns the source-unit children in source ID order.
    pub fn source_units(&self) -> &[SourceUnitSyntax] {
        self.root.source_units()
    }

    /// Resolves one exact immutable node handle without traversing the tree.
    pub fn find_node(
        &self,
        source_id: SourceId,
        kind: SyntaxKind,
        full_range: TextRange,
        is_recovered: bool,
    ) -> Option<SyntaxNodeView<'_>> {
        self.nodes
            .get_or_init(|| Arc::new(indexed_nodes(&self.root)))
            .get(&SyntaxNodeKey {
                source_id,
                kind,
                full_range,
                is_recovered,
            })
            .map(IndexedSyntaxNode::view)
    }
}

impl fmt::Debug for SyntaxTree {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SyntaxTree")
            .field("root", &self.root)
            .finish()
    }
}

impl PartialEq for SyntaxTree {
    fn eq(&self, other: &Self) -> bool {
        self.root == other.root
    }
}

impl Eq for SyntaxTree {}

impl Hash for SyntaxTree {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.root.hash(state);
    }
}

impl SyntaxText for SyntaxTree {
    /// Appends the exact source text represented by the tree to `writer`.
    fn write_full_text(&self, writer: &mut dyn Write) -> fmt::Result {
        self.root.write_full_text(writer)
    }
}

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
struct SyntaxNodeKey {
    source_id: SourceId,
    kind: SyntaxKind,
    full_range: TextRange,
    is_recovered: bool,
}

#[derive(Clone)]
struct IndexedSyntaxNode {
    source: SourceSnapshot,
    node: GreenNode,
    start: TextSize,
}

impl IndexedSyntaxNode {
    fn view(&self) -> SyntaxNodeView<'_> {
        SyntaxNodeView::new(&self.source, &self.node, self.start)
    }
}

fn indexed_nodes(root: &CompilationUnitSyntax) -> HashMap<SyntaxNodeKey, IndexedSyntaxNode> {
    let mut nodes = HashMap::new();

    for source_unit in root.source_units() {
        walk_syntax_node(source_unit, |event| {
            let SyntaxWalkEvent::EnterNode(node) = event else {
                return SyntaxWalkControl::Continue;
            };

            let key = SyntaxNodeKey {
                source_id: node.source().source_id(),
                kind: node.kind(),
                full_range: node.full_range(),
                is_recovered: node.is_recovered(),
            };

            // Indexed handles share immutable source and green storage with the typed tree.
            nodes.entry(key).or_insert_with(|| IndexedSyntaxNode {
                source: node.source().clone(),
                node: node.green_node().clone(),
                start: node.start(),
            });

            SyntaxWalkControl::Continue
        });
    }

    nodes
}

#[cfg(test)]
mod tests {
    use bray_source::{SourceId, SourceIdentity, SourceOrigin, SourceSnapshot, SourceVersion};

    use super::SyntaxTree;
    use crate::{SourceSyntaxNode, SourceUnitSyntax, SyntaxKind, SyntaxToken};

    #[test]
    fn syntax_trees_store_a_compilation_unit_root() {
        let source_unit = SourceUnitSyntax::builder(snapshot(""))
            .tokens([SyntaxToken::end_of_file(bray_source::TextSize::ZERO)])
            .build();

        let tree = SyntaxTree::compilation_unit([source_unit]);

        assert_eq!(tree.root().kind(), SyntaxKind::CompilationUnit);
        assert_eq!(tree.source_units().len(), 1);
    }

    #[test]
    fn syntax_trees_are_send_and_sync() {
        assert_send_sync::<SyntaxTree>();
    }

    #[test]
    fn syntax_trees_resolve_exact_node_handles_repeatably() {
        let source_unit = SourceUnitSyntax::builder(snapshot(""))
            .tokens([SyntaxToken::end_of_file(bray_source::TextSize::ZERO)])
            .build();

        let source_id = source_unit.source().source_id();
        let full_range = source_unit.full_range();
        let is_recovered = source_unit.is_recovered();

        let tree = SyntaxTree::compilation_unit([source_unit]);

        let first = tree.find_node(source_id, SyntaxKind::SourceUnit, full_range, is_recovered);
        let second = tree.find_node(source_id, SyntaxKind::SourceUnit, full_range, is_recovered);

        assert_eq!(first, second);
        assert_eq!(first.map(|node| node.kind()), Some(SyntaxKind::SourceUnit));
    }

    fn assert_send_sync<T: Send + Sync>() {}

    fn snapshot(text: &str) -> SourceSnapshot {
        match SourceSnapshot::new(
            SourceId::new(0),
            SourceIdentity::new(0),
            SourceOrigin::virtual_source("syntax-tree-test"),
            SourceVersion::new(0),
            text,
        ) {
            Ok(snapshot) => snapshot,
            Err(error) => panic!("test source should fit in TextSize: {error:?}"),
        }
    }
}
