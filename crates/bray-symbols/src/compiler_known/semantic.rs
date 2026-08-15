use std::marker::PhantomData;

use bray_compiler_known::{
    CatalogDeclarationSurfaceSyntax, CompilerKnownDeclarationDescriptor, CompilerKnownDeclarationId,
};

use crate::{ExactSymbolId, SymbolKind};

/// A category-typed route to one compiler-known declaration's semantics.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CompilerKnownSemanticKey<I: ExactSymbolId> {
    declaration: CompilerKnownDeclarationId,
    marker: PhantomData<fn() -> I>,
}

impl<I: ExactSymbolId> CompilerKnownSemanticKey<I> {
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

/// Semantics supplied by one compiler-known declaration descriptor.
#[derive(Clone, Copy, Debug)]
pub struct CompilerKnownDeclarationSemantics<'catalog> {
    descriptor: &'catalog CompilerKnownDeclarationDescriptor,
    surface: &'catalog CatalogDeclarationSurfaceSyntax,
}

impl<'catalog> CompilerKnownDeclarationSemantics<'catalog> {
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

    /// Returns the declaration's parsed syntax surface.
    pub const fn surface(&self) -> &'catalog CatalogDeclarationSurfaceSyntax {
        self.surface
    }
}

#[cfg(test)]
mod tests {
    use bray_compiler_known::ImplementationHook;

    use crate::FunctionSymbolId;
    use crate::compiler_known::test_support::{build_provider, declaration_key};

    #[test]
    fn declaration_semantics_are_lazy_static_generated_surfaces() {
        let provider = build_provider();
        let key = declaration_key("MemoryCopy");

        let Some(semantic_key) = provider.semantic_key::<FunctionSymbolId>(&key) else {
            panic!("MemoryCopy semantic key must be typed as a function");
        };

        let Some(semantics) = provider.declaration_semantics(semantic_key) else {
            panic!("generated MemoryCopy semantics must resolve");
        };

        assert_eq!(semantics.descriptor().key(), &key);

        assert_eq!(
            semantics.descriptor().implementation_hook(),
            Some(ImplementationHook::MemoryCopy)
        );

        assert_eq!(semantics.surface().kind(), semantics.descriptor().kind());
        assert!(!semantics.surface().elements().is_empty());
    }
}
