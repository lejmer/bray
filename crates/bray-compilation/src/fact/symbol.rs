use std::marker::PhantomData;
use std::sync::Arc;

use bray_symbols::{SymbolQueryContract, SymbolQueryRequest};

use super::{
    CancellationToken, CompilationFactKey, FactCellMap, FactQueryError, FactRuntime, SymbolQueryKey,
};

pub(crate) struct SymbolQueryCache<C>
where
    C: SymbolQueryContract,
{
    cells: FactCellMap<
        C::Owner,
        Arc<bray_diagnostics::DiagnosticResult<<C as bray_symbols::SymbolQueryContract>::Value>>,
    >,
    marker: PhantomData<fn() -> C>,
}

impl<C> SymbolQueryCache<C>
where
    C: SymbolQueryContract,
{
    pub(crate) fn new() -> Self {
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
                CompilationFactKey::from(SymbolQueryKey::new(C::erase_owner(*owner), C::KIND))
            }),
            marker: PhantomData,
        }
    }

    pub(crate) fn get_or_compute(
        &self,
        runtime: &FactRuntime,
        cancellation: &CancellationToken,
        request: SymbolQueryRequest<C>,
        compute: impl FnOnce() -> Result<
            bray_diagnostics::DiagnosticResult<<C as bray_symbols::SymbolQueryContract>::Value>,
            FactQueryError,
        > + Send,
    ) -> Result<
        Arc<bray_diagnostics::DiagnosticResult<<C as bray_symbols::SymbolQueryContract>::Value>>,
        FactQueryError,
    > {
        let key = CompilationFactKey::from(SymbolQueryKey::new(request.symbol(), request.kind()));

        let profile = runtime
            .profile()
            .map(|profile| (profile, crate::profile::ProfileQueryKind::from_key(&key)));

        let cell = self.cells.cell(request.owner())?;

        let published = cell.get_or_compute(runtime, key, cancellation, || {
            let result = compute()?;

            crate::profile::record_query_diagnostic_collection(profile, result.diagnostics().len());

            Ok(Arc::new(result))
        })?;

        crate::profile::record_query_result_reference::<bray_diagnostics::DiagnosticResult<C::Value>>(
            profile,
        );

        // The returned value must outlive the short-lived cache-cell borrow.
        Ok(Arc::clone(published))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use bray_diagnostics::DiagnosticResult;
    use bray_symbols::{
        CallableSignatureQuery, CallableSignatureTemplate, CallableSymbolId, FunctionSymbolId,
        SemanticValueStore, SymbolId, SymbolQueryRequest, TypeData, TypeExpressionTemplate,
    };

    use super::SymbolQueryCache;
    use crate::fact::{CancellationToken, FactRuntime};

    #[test]
    fn repeated_resolve_symbol_query_requests_publish_once() {
        let runtime = FactRuntime::default();
        let cancellation = CancellationToken::new();
        let cache = SymbolQueryCache::<CallableSignatureQuery>::new();
        let computations = AtomicUsize::new(0);

        let function = FunctionSymbolId::from_symbol_id(SymbolId::new(4));
        let request = SymbolQueryRequest::new(CallableSymbolId::from(function));
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
            panic!("symbol query results should publish");
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
