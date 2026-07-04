use std::fmt::{self, Write};

use super::{CompilationUnitSyntax, SourceUnitSyntax, SyntaxText};

/// Immutable syntax tree rooted at a compilation unit.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SyntaxTree {
    root: CompilationUnitSyntax,
}

impl SyntaxTree {
    /// Creates a syntax tree from a compilation-unit root node.
    pub const fn new(root: CompilationUnitSyntax) -> Self {
        Self { root }
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
}

impl SyntaxText for SyntaxTree {
    /// Appends the exact source text represented by the tree to `writer`.
    fn write_full_text(&self, writer: &mut dyn Write) -> fmt::Result {
        self.root.write_full_text(writer)
    }
}

#[cfg(test)]
mod tests {
    use bray_source::{SourceId, SourceIdentity, SourceOrigin, SourceSnapshot, SourceVersion};

    use super::SyntaxTree;
    use crate::{SourceUnitSyntax, SyntaxKind, SyntaxToken};

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
