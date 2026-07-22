mod context;
mod symbol;
mod value_type;
mod value_type_surface;

use bray_binder::BinderFactError;

use crate::fact::FactQueryError;

pub(super) use context::CompilationBinderFacts;
pub(in crate::compilation) use symbol::{CompilationSymbolFacts, imported_implementation};
pub(in crate::compilation) use value_type::bind_declared_value_type_templates;

pub(in crate::compilation) const fn binder_fact_error(error: BinderFactError) -> FactQueryError {
    match error {
        BinderFactError::Cancelled => FactQueryError::Cancelled,
        BinderFactError::DependencyUnavailable => FactQueryError::InfrastructureFailure,
    }
}
