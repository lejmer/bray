use bray_declarations::DeclarationTable;
use bray_symbols::{SemanticValueStore, SymbolGraph};
use bray_syntax::SyntaxTree;

use crate::{BinderCancellation, TargetFactProvider};

/// Injected read-only facts available to one binding computation.
///
/// Implementations are supplied by the compilation query layer. This contract deliberately
/// exposes semantic inputs and provider boundaries rather than compilation caches, dependency
/// stacks, workers, or publication policy.
pub trait BinderFactContext: Send + Sync {
    /// The provider for target-profile-dependent constant facts.
    type TargetFacts: TargetFactProvider + ?Sized;
    /// The provider for facts decoded from imported package interfaces.
    type ImportedSymbolFacts: Send + Sync + ?Sized;
    /// The provider for ordinary symbol-facing semantic facts.
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

    /// Returns the imported symbol-fact provider.
    fn imported_symbol_facts(&self) -> &Self::ImportedSymbolFacts;

    /// Returns the ordinary symbol-fact provider.
    fn symbol_facts(&self) -> &Self::SymbolFacts;

    /// Returns the cancellation observer for this binding request.
    fn cancellation(&self) -> &Self::Cancellation;

    /// Returns whether cancellation has been requested.
    fn is_cancelled(&self) -> bool {
        self.cancellation().is_cancelled()
    }
}
