use bray_binder::BindingQueryError;

use crate::fact::FactQueryError;

pub(in crate::compilation) const fn binding_query_error(
    error: BindingQueryError,
) -> FactQueryError {
    match error {
        BindingQueryError::Cancelled => FactQueryError::Cancelled,
        BindingQueryError::DependencyUnavailable => FactQueryError::InfrastructureFailure,
    }
}
