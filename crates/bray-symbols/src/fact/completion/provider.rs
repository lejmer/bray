use bray_diagnostics::DiagnosticBag;

use super::SymbolFactCompletionRequest;

/// Read-only cancellation contract used while planning symbol completion.
pub trait SymbolCompletionCancellation: Sync {
    /// Returns whether the current completion request should stop without publication.
    fn is_cancelled(&self) -> bool;
}

/// A cancellation source that never cancels completion planning.
#[derive(Clone, Copy, Debug, Default)]
pub struct NeverCancelSymbolCompletion;

impl SymbolCompletionCancellation for NeverCancelSymbolCompletion {
    fn is_cancelled(&self) -> bool {
        false
    }
}

/// Computes or retrieves one symbol fact and returns its owned diagnostics.
///
/// Legal semantic recursion should resolve through the provider's ordinary typed facts. An
/// invalid dependency cycle is an outer provider error unless that fact category deliberately
/// recovers by publishing an error-aware value and diagnostics.
pub trait SymbolFactForcer: Sync {
    /// The provider-specific outer query failure.
    type Error: Send;

    /// Forces one exact fact without pre-rendering or globally publishing diagnostics.
    fn force(&self, request: SymbolFactCompletionRequest) -> Result<DiagnosticBag, Self::Error>;
}

impl<F, E> SymbolFactForcer for F
where
    F: Fn(SymbolFactCompletionRequest) -> Result<DiagnosticBag, E> + Sync,
    E: Send,
{
    type Error = E;

    fn force(&self, request: SymbolFactCompletionRequest) -> Result<DiagnosticBag, Self::Error> {
        self(request)
    }
}
