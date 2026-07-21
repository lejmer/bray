use bray_base::Cancellation;
use bray_declarations::DeclarationTable;
use bray_symbols::{
    AnySymbolId, ImportedSymbolSkeleton, PackageSymbolId, SemanticValueStore, SymbolGraph,
    SymbolKey,
};
use bray_syntax::SyntaxTree;
use bray_target::TargetProfile;

use crate::BinderFactResult;

/// One selected imported package root for a qualified source path.
#[derive(Clone, Copy, Debug)]
pub struct ImportedPathRoot<'symbols> {
    symbols: &'symbols ImportedSymbolSkeleton,
    package: PackageSymbolId,
    consumed_components: usize,
}

impl<'symbols> ImportedPathRoot<'symbols> {
    /// Creates a root when the package identity is an exact prefix of the source path.
    pub fn for_path(
        symbols: &'symbols ImportedSymbolSkeleton,
        package: PackageSymbolId,
        components: &[&str],
    ) -> Option<Self> {
        let package_identity = symbols.package(package)?.identity();
        let consumed_components = package_identity.as_str().split('.').count();

        if consumed_components > components.len()
            || !package_identity
                .as_str()
                .split('.')
                .eq(components[..consumed_components].iter().copied())
        {
            return None;
        }

        Some(Self {
            symbols,
            package,
            consumed_components,
        })
    }

    /// Returns the imported identity provider for the selected package.
    pub const fn symbols(self) -> &'symbols ImportedSymbolSkeleton {
        self.symbols
    }

    /// Returns the exact imported package identity selected by the path prefix.
    pub const fn package(self) -> PackageSymbolId {
        self.package
    }

    /// Returns the number of source path components occupied by the package identity.
    pub const fn consumed_components(self) -> usize {
        self.consumed_components
    }
}

/// Injected read-only facts available to one binding computation.
pub trait BinderFactContext: Send + Sync {
    /// The origin-neutral provider for symbol-facing semantic facts.
    type SymbolFacts: Send + Sync + ?Sized;
    /// The compilation-owned cancellation observer.
    type Cancellation: Cancellation + ?Sized;

    /// Returns the immutable syntax input for this compilation snapshot.
    fn syntax(&self) -> &SyntaxTree;

    /// Returns the immutable declaration-discovery input.
    fn declarations(&self) -> &DeclarationTable;

    /// Returns the immutable compilation-wide symbol identity graph.
    fn symbols(&self) -> &SymbolGraph;

    /// Returns one exact symbol's stable semantic key across supported origins.
    fn symbol_key(&self, symbol: AnySymbolId) -> BinderFactResult<Option<&SymbolKey>> {
        Ok(self.symbols().symbol_key(symbol))
    }

    /// Returns whether recovery contributed to one exact symbol's surface.
    fn symbol_is_recovered(&self, symbol: AnySymbolId) -> BinderFactResult<Option<bool>> {
        Ok(self.symbols().symbol_is_recovered(symbol))
    }

    /// Resolves the longest selected dependency package prefix of a qualified source path.
    fn imported_path_root(
        &self,
        components: &[&str],
    ) -> BinderFactResult<Option<ImportedPathRoot<'_>>>;

    /// Returns the canonical semantic value store associated with the symbol graph.
    fn semantic_values(&self) -> &SemanticValueStore;

    /// Returns the selected language-level target profile.
    fn selected_target(&self) -> &TargetProfile;

    /// Returns the origin-neutral symbol-fact provider.
    fn symbol_facts(&self) -> &Self::SymbolFacts;

    /// Returns the cancellation observer for this binding request.
    fn cancellation(&self) -> &Self::Cancellation;

    /// Returns whether cancellation has been requested.
    fn is_cancelled(&self) -> bool {
        self.cancellation().is_cancelled()
    }
}

#[cfg(test)]
mod tests {
    use super::BinderFactContext;
    use crate::fact::test_support::TestFixture;

    #[test]
    fn contexts_expose_exact_immutable_fact_inputs() {
        let fixture = TestFixture::new();
        let context = fixture.context();

        assert_eq!(context.syntax().source_units().len(), 1);
        assert_eq!(context.declarations().declarations().len(), 2);
        assert_eq!(context.symbols().constants().len(), 1);
        assert_eq!(context.semantic_values().id(), fixture.semantic_values.id());
        assert_eq!(
            context.selected_target().identity().as_str(),
            "x86_64-unknown-linux-gnu"
        );
        assert!(!context.is_cancelled());
    }

    #[test]
    fn contexts_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<crate::fact::test_support::TestContext<'static>>();
    }
}
