mod diagnostic;
mod diagnostic_context;
mod model;
mod runtime_diagnostic;
mod semantic_diagnostic;

pub(crate) use diagnostic::{
    diagnostic_binding_failure, diagnostic_checker_failure, diagnostic_cycle_failure,
    diagnostic_semantic_context_failure, diagnostic_semantic_value_failure,
};
pub(crate) use runtime_diagnostic::diagnostic_fact_runtime_failure;
pub(crate) use semantic_diagnostic::diagnostic_semantic_query_failure;
pub use model::{FactCycle, FactQueryError, LocatedLoweringFailure};
