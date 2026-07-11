//! Name binding and semantic reference resolution.

#![forbid(unsafe_code)]

mod result;

// Category-specific binder entrypoints will consume this task-local construction layer.
#[allow(dead_code)]
mod unit;

pub use bray_checker::CheckerOutcome as BindingOutcome;
pub use result::{
    CheckedAnonymousCallableResult, CheckedCallableBodyResult, CheckedConstantTemplateResult,
    CheckedConstraintResult, CheckedContractClauseResult, CheckedPredicateDefinitionResult,
    CheckedRuntimeDefaultResult,
};
