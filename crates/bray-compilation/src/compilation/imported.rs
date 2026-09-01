mod body;
mod diagnostic;
mod model;
mod query;

pub(in crate::compilation) use diagnostic::{
    standard_library_failure_diagnostic, with_standard_library_product_context,
};
pub(super) use model::LoadedDependencyInterface;
