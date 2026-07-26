use bray_binder::BinderFactError;

use crate::fact::FactQueryError;

pub(in crate::compilation) const fn binder_fact_error(error: BinderFactError) -> FactQueryError {
    match error {
        BinderFactError::Cancelled => FactQueryError::Cancelled,
        BinderFactError::DependencyUnavailable => FactQueryError::InfrastructureFailure,
    }
}
