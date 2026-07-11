use std::sync::Arc;

use bray_symbols::{SymbolFactContract, SymbolFactRequest, SymbolFactResult};

use crate::{BinderFactContext, BinderFactResult};

/// Provides shared immutable access to one category of symbol-facing semantic fact.
///
/// Implement this trait separately for each supported [`SymbolFactContract`]. The coordinating
/// query layer can route source, compiler-known, synthesized, or imported symbols internally,
/// but repeated completed requests must expose the same published result.
pub trait SymbolFactProvider<Contract>: Send + Sync
where
    Contract: SymbolFactContract,
{
    /// Returns the published fact for the typed request.
    fn symbol_fact(
        &self,
        request: SymbolFactRequest<Contract>,
    ) -> BinderFactResult<Arc<SymbolFactResult<Contract>>>;
}

/// Provides shared immutable symbol facts decoded from compiled package interfaces.
///
/// Imported storage, decoding, validation, and local identity mapping remain hidden behind this
/// provider. Invalid external input must be represented by the fact's structured diagnostics and
/// category-specific recovery value, not by a panic.
pub trait ImportedSymbolFactProvider<Contract>: Send + Sync
where
    Contract: SymbolFactContract,
{
    /// Returns the published imported fact for the typed compilation-local owner.
    fn imported_symbol_fact(
        &self,
        request: SymbolFactRequest<Contract>,
    ) -> BinderFactResult<Arc<SymbolFactResult<Contract>>>;
}

/// Computes one binding-dependent symbol fact without coordinating its publication.
///
/// Binder implementations return one complete immutable value with its owned diagnostics.
/// Compilation wraps this operation in exact keys, dependency tracking, caching, cycle handling,
/// cancellation, and atomic publication.
pub trait BindingSymbolFactProvider<Contract, Context>: Send + Sync
where
    Contract: SymbolFactContract,
    Context: BinderFactContext + ?Sized,
{
    /// Computes the complete fact selected by `request` from injected read-only dependencies.
    fn compute_symbol_fact(
        &self,
        context: &Context,
        request: SymbolFactRequest<Contract>,
    ) -> BinderFactResult<SymbolFactResult<Contract>>;
}
