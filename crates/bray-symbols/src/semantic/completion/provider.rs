use bray_base::Cancellation;
use bray_diagnostics::DiagnosticBag;

use super::SymbolCompletionQuery;

/// A cancellation source that never cancels completion planning.
#[derive(Clone, Copy, Debug, Default)]
pub struct NeverCancelSymbolCompletion;

impl Cancellation for NeverCancelSymbolCompletion {
    fn is_cancelled(&self) -> bool {
        false
    }
}

/// Evaluates one symbol query and returns its owned diagnostics.
///
/// Legal semantic recursion should resolve through ordinary typed queries. An invalid dependency
/// cycle is an outer evaluator error unless that query category deliberately
/// recovers by publishing an error-aware value and diagnostics.
pub trait SymbolCompletionEvaluator: Sync {
    /// The evaluator-specific outer query failure.
    type Error: Send;

    /// Evaluates one exact query without pre-rendering or globally publishing diagnostics.
    fn evaluate(&self, request: SymbolCompletionQuery) -> Result<DiagnosticBag, Self::Error>;
}

impl<F, E> SymbolCompletionEvaluator for F
where
    F: Fn(SymbolCompletionQuery) -> Result<DiagnosticBag, E> + Sync,
    E: Send,
{
    type Error = E;

    fn evaluate(&self, request: SymbolCompletionQuery) -> Result<DiagnosticBag, Self::Error> {
        self(request)
    }
}

#[cfg(test)]
mod tests {
    use bray_base::Cancellation;
    use bray_diagnostics::DiagnosticBag;

    use super::{NeverCancelSymbolCompletion, SymbolCompletionEvaluator};
    use crate::{AnySymbolId, FunctionSymbolId, SymbolCompletionQuery, SymbolId, SymbolQueryKind};

    #[test]
    fn never_cancel_source_remains_active() {
        assert!(!NeverCancelSymbolCompletion.is_cancelled());
    }

    #[test]
    fn closures_adapt_typed_query_evaluation() {
        let symbol = AnySymbolId::from(FunctionSymbolId::from_symbol_id(SymbolId::new(1)));
        let request = SymbolCompletionQuery::new(symbol, SymbolQueryKind::CallableSignature);

        let evaluator = |requested| -> Result<DiagnosticBag, ()> {
            assert_eq!(requested, request);

            Ok(DiagnosticBag::new())
        };

        let result = evaluator.evaluate(request);

        assert!(matches!(result, Ok(diagnostics) if diagnostics.is_empty()));
    }

    #[test]
    fn completion_helpers_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<NeverCancelSymbolCompletion>();
    }
}
