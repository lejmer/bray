use std::marker::PhantomData;

use bray_compiler_known::{
    CatalogDeclarationSurfaceSyntax, CompilerKnownDeclarationDescriptor, CompilerKnownDeclarationId,
};

use crate::{ExactSymbolId, SymbolKind};

/// A category-typed route to one compiler-known declaration's ordinary lazy facts.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CompilerKnownSymbolFactKey<I: ExactSymbolId> {
    declaration: CompilerKnownDeclarationId,
    marker: PhantomData<fn() -> I>,
}

impl<I: ExactSymbolId> CompilerKnownSymbolFactKey<I> {
    pub(crate) const fn new(declaration: CompilerKnownDeclarationId) -> Self {
        Self {
            declaration,
            marker: PhantomData,
        }
    }

    /// Returns the catalog-local declaration routed by this key.
    pub const fn declaration(self) -> CompilerKnownDeclarationId {
        self.declaration
    }

    /// Returns the exact ordinary symbol category expected by the request.
    pub const fn kind(self) -> SymbolKind {
        I::KIND
    }
}

/// Descriptor-backed declaration facts resolved only when requested.
#[derive(Clone, Copy, Debug)]
pub struct CompilerKnownDeclarationFact<'catalog> {
    descriptor: &'catalog CompilerKnownDeclarationDescriptor,
    surface: &'catalog CatalogDeclarationSurfaceSyntax,
}

impl<'catalog> CompilerKnownDeclarationFact<'catalog> {
    pub(crate) const fn new(
        descriptor: &'catalog CompilerKnownDeclarationDescriptor,
        surface: &'catalog CatalogDeclarationSurfaceSyntax,
    ) -> Self {
        Self {
            descriptor,
            surface,
        }
    }

    /// Returns the immutable catalog descriptor supplying metadata for this symbol.
    pub const fn descriptor(&self) -> &'catalog CompilerKnownDeclarationDescriptor {
        self.descriptor
    }

    /// Returns generated pre-parsed declaration syntax without reading `.braydef` input.
    pub const fn surface(&self) -> &'catalog CatalogDeclarationSurfaceSyntax {
        self.surface
    }
}
