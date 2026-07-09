use bray_syntax::{SourceSyntaxNode, SyntaxCast, SyntaxNodeView};

use crate::chunk::DiscoveredDeclaration;
use crate::name::DeclarationName;
use crate::record::DeclarationKind;

pub(super) fn cast_node<T>(view: SyntaxNodeView<'_>, description: &'static str) -> T
where
    T: SyntaxCast,
{
    match view.cast::<T>() {
        Some(node) => node,
        None => panic!("{description} view should cast"),
    }
}

pub(super) fn discovered_declaration<N>(
    kind: DeclarationKind,
    name: Option<DeclarationName>,
    node: &N,
) -> DiscoveredDeclaration
where
    N: SourceSyntaxNode,
{
    DiscoveredDeclaration::new(
        kind,
        name,
        node.source().source_id(),
        node.kind(),
        node.full_range(),
        node.is_recovered(),
        Box::new([]),
    )
}
