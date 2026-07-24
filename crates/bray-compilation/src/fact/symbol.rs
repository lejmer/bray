use std::marker::PhantomData;
use std::sync::Arc;

use bray_symbols::{SymbolFactContract, SymbolFactRequest, SymbolFactResult};

use super::{
    CancellationToken, CompilationFactKey, FactCellMap, FactQueryError, FactRuntime, SymbolFactKey,
};

pub(crate) struct SymbolFactCache<C>
where
    C: SymbolFactContract,
{
    cells: FactCellMap<C::Owner, Arc<SymbolFactResult<C>>>,
    marker: PhantomData<fn() -> C>,
}

impl<C> SymbolFactCache<C>
where
    C: SymbolFactContract,
{
    pub(crate) const fn new() -> Self {
        Self {
            cells: FactCellMap::new(),
            marker: PhantomData,
        }
    }

    pub(crate) fn updated(
        &self,
        reusable: &std::collections::BTreeSet<CompilationFactKey>,
    ) -> Self {
        Self {
            cells: self.cells.updated(reusable, |owner| {
                CompilationFactKey::from(SymbolFactKey::new(C::erase_owner(*owner), C::KIND))
            }),
            marker: PhantomData,
        }
    }

    pub(crate) fn get_or_compute(
        &self,
        runtime: &FactRuntime,
        cancellation: &CancellationToken,
        request: SymbolFactRequest<C>,
        compute: impl FnOnce() -> Result<SymbolFactResult<C>, FactQueryError> + Send,
    ) -> Result<Arc<SymbolFactResult<C>>, FactQueryError> {
        let key = CompilationFactKey::from(SymbolFactKey::new(request.symbol(), request.kind()));

        let cell = self.cells.cell(request.owner())?;
        let published =
            cell.get_or_compute(runtime, key, cancellation, || compute().map(Arc::new))?;

        // The returned fact must outlive the short-lived cache-cell borrow.
        Ok(Arc::clone(published))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use bray_diagnostics::DiagnosticResult;
    use bray_symbols::{
        CallableSignatureFact, CallableSignatureTemplate, CallableSymbolId, FunctionSymbolId,
        SemanticValueStore, SymbolFactRequest, SymbolId, TypeData, TypeExpressionTemplate,
    };

    use super::SymbolFactCache;
    use crate::fact::{CancellationToken, FactRuntime};

    #[test]
    fn repeated_symbol_fact_requests_publish_once() {
        let runtime = FactRuntime::default();
        let cancellation = CancellationToken::new();
        let cache = SymbolFactCache::<CallableSignatureFact>::new();
        let computations = AtomicUsize::new(0);

        let function = FunctionSymbolId::from_symbol_id(SymbolId::new(4));
        let request = SymbolFactRequest::new(CallableSymbolId::from(function));
        let signature = signature();

        let first = cache.get_or_compute(&runtime, &cancellation, request, || {
            computations.fetch_add(1, Ordering::SeqCst);

            Ok(DiagnosticResult::without_diagnostics(signature.clone()))
        });

        let second = cache.get_or_compute(&runtime, &cancellation, request, || {
            computations.fetch_add(1, Ordering::SeqCst);

            Ok(DiagnosticResult::without_diagnostics(signature))
        });

        let (Ok(first), Ok(second)) = (first, second) else {
            panic!("symbol facts should publish");
        };

        assert!(std::sync::Arc::ptr_eq(&first, &second));
        assert_eq!(computations.load(Ordering::SeqCst), 1);
    }

    fn signature() -> CallableSignatureTemplate {
        let Ok(store) = SemanticValueStore::try_new() else {
            panic!("semantic store should build");
        };

        let Ok(ty) = store.intern_type(TypeData::Error) else {
            panic!("error type should intern");
        };

        CallableSignatureTemplate::new(
            TypeExpressionTemplate::Resolved(ty),
            None,
            [],
            TypeExpressionTemplate::Resolved(ty),
        )
    }
}
