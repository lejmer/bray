use std::marker::PhantomData;

use bray_compiler_known::{
    CatalogDeclarationSurfaceSyntax, CompilerKnownDeclarationDescriptor, CompilerKnownDeclarationId,
};

use crate::{ExactSymbolId, SymbolKind};

/// A category-typed route to one compiler-known declaration's semantic facts.
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

/// Semantic facts supplied by one compiler-known declaration descriptor.
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
    fn declaration_facts_are_lazy_static_generated_surfaces() {
        let provider = build_provider();
        let key = declaration_key("MemoryCopy");

        let Some(fact_key) = provider.fact_key::<FunctionSymbolId>(&key) else {
            panic!("MemoryCopy fact key must be typed as a function");
        };

        let Some(fact) = provider.declaration_fact(fact_key) else {
            panic!("generated MemoryCopy facts must resolve");
        };

        assert_eq!(fact.descriptor().key(), &key);

        assert_eq!(
            fact.descriptor().implementation_hook(),
            Some(ImplementationHook::MemoryCopy)
        );

        assert_eq!(fact.surface().kind(), fact.descriptor().kind());
        assert!(!fact.surface().elements().is_empty());
    }
}
