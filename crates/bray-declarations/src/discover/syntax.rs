use bray_syntax::{SourceSyntaxNode, SyntaxCast, SyntaxNodeView};

use crate::chunk::DiscoveredDeclaration;
use crate::name::DeclarationName;
use crate::record::DeclarationKind;
use crate::surface::{DeclarationSurface, SyntaxAnchor};

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
    discovered_declaration_with_surface(kind, name, node, DeclarationSurface::empty())
}

pub(super) fn discovered_declaration_with_surface<N>(
    kind: DeclarationKind,
    name: Option<DeclarationName>,
    node: &N,
    surface: DeclarationSurface,
) -> DiscoveredDeclaration
where
    N: SourceSyntaxNode,
{
    DiscoveredDeclaration::new(
        kind,
        name,
        SyntaxAnchor::from_node(node),
        surface,
        Box::new([]),
    )
}
