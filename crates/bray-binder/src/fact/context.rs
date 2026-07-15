use bray_declarations::DeclarationTable;
use bray_symbols::{SemanticValueStore, SymbolGraph};
use bray_syntax::SyntaxTree;

use crate::{BinderCancellation, TargetFactProvider};

/// Injected read-only facts available to one binding computation.
pub trait BinderFactContext: Send + Sync {
    /// The provider for target-profile-dependent constant facts.
    type TargetFacts: TargetFactProvider + ?Sized;
    /// The origin-neutral provider for symbol-facing semantic facts.
    type SymbolFacts: Send + Sync + ?Sized;
    /// The compilation-owned cancellation observer.
    type Cancellation: BinderCancellation + ?Sized;

    /// Returns the immutable syntax input for this compilation snapshot.
    fn syntax(&self) -> &SyntaxTree;

    /// Returns the immutable declaration-discovery input.
    fn declarations(&self) -> &DeclarationTable;

    /// Returns the immutable compilation-wide symbol identity graph.
    fn symbols(&self) -> &SymbolGraph;

    /// Returns the canonical semantic value store associated with the symbol graph.
    fn semantic_values(&self) -> &SemanticValueStore;

    /// Returns the selected target's fact provider.
    fn target_facts(&self) -> &Self::TargetFacts;

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
        assert!(!context.is_cancelled());
    }

    #[test]
    fn contexts_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<crate::fact::test_support::TestContext<'static>>();
    }
}
