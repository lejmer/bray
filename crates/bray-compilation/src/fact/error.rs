mod diagnostic;
mod model;

pub(crate) use diagnostic::{
    diagnostic_binding_failure, diagnostic_checker_failure, diagnostic_semantic_query_failure,
    diagnostic_semantic_value_failure,
};
pub use model::{FactCycle, FactQueryError, LocatedLoweringFailure};
