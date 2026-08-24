use bray_binder::{BindingQueryError, BindingQueryResult};
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

pub(super) fn binder_error(error: FactQueryError) -> BindingQueryError {
    match error {
        FactQueryError::Cancelled => BindingQueryError::Cancelled,
        FactQueryError::Cycle(_)
        | FactQueryError::InfrastructureFailure
        | FactQueryError::AtomicInitializerArgumentUnavailable
        | FactQueryError::AtomicInitializerResultUnavailable
        | FactQueryError::ImportedExecutableTemplateMismatch
        | FactQueryError::SemanticUnitContext(_)
        | FactQueryError::CheckerInfrastructure(_) => BindingQueryError::DependencyUnavailable,
    }
}
