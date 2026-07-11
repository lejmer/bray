use std::fmt;

use bray_source::{SourceSnapshot, TextRange};
use bray_syntax::{SourceSyntaxNode, SyntaxKind, SyntaxNode};

struct ExternalSourceNode;

impl SyntaxNode for ExternalSourceNode {
    fn kind(&self) -> SyntaxKind {
        SyntaxKind::IdentifierToken
    }

    fn full_range(&self) -> TextRange {
        TextRange::default()
    }
}

impl SourceSyntaxNode for ExternalSourceNode {
    fn source(&self) -> &SourceSnapshot {
        panic!("compile-time contract test does not access a source")
    }

    fn write_full_text_from(
        &self,
        _source_text: &str,
        _writer: &mut dyn fmt::Write,
    ) -> fmt::Result {
        Ok(())
    }
}

#[test]
fn source_syntax_node_remains_implementable_without_green_storage() {
    fn require_source_syntax_node<T: SourceSyntaxNode>() {}

    require_source_syntax_node::<ExternalSourceNode>();
}
