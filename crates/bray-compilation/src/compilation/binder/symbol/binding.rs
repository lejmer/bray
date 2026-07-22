use bray_binder::{BinderFactError, BinderFactResult};
use bray_symbols::{SymbolFactContract, SymbolFactRequest, SymbolFactResult};

use crate::compilation::binder::CompilationBinderFacts;
use crate::fact::{FactQueryError, SymbolFactCache};

pub(super) trait CompilationSymbolFactBinding<C>
where
    C: SymbolFactContract,
{
    fn cache(&self) -> &SymbolFactCache<C>;

    fn bind(
        &self,
        context: &CompilationBinderFacts<'_>,
        request: SymbolFactRequest<C>,
    ) -> BinderFactResult<SymbolFactResult<C>>;
}

pub(super) fn binder_error(error: FactQueryError) -> BinderFactError {
    match error {
        FactQueryError::Cancelled => BinderFactError::Cancelled,
        FactQueryError::Cycle(_)
        | FactQueryError::InfrastructureFailure
        | FactQueryError::SemanticUnitContext(_)
        | FactQueryError::CheckerInfrastructure(_) => BinderFactError::DependencyUnavailable,
    }
}
