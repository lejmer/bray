use crate::compilation::binder::BindingQueryResult;
use bray_binder::BindingQueryError;
use bray_symbols::{SymbolQueryContract, SymbolQueryRequest};

use crate::compilation::binder::CompilationBindingContext;
use crate::fact::{FactQueryError, SymbolQueryCache};

pub(super) trait CompilationSymbolQueryEvaluator<C>
where
    C: SymbolQueryContract,
{
    fn cache(&self) -> &SymbolQueryCache<C>;

    fn bind(
        &self,
        context: &CompilationBindingContext<'_>,
        request: SymbolQueryRequest<C>,
    ) -> BindingQueryResult<
        bray_diagnostics::DiagnosticResult<<C as bray_symbols::SymbolQueryContract>::Value>,
    >;
}

pub(in crate::compilation) fn binder_error(
    error: FactQueryError,
) -> BindingQueryError<FactQueryError> {
    match error {
        FactQueryError::Cancelled => BindingQueryError::Cancelled,
        FactQueryError::SemanticValueStore(error) => BindingQueryError::SemanticValue(error),
        error => BindingQueryError::Upstream(error),
    }
}
