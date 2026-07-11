//! Name binding and semantic reference resolution.

#![forbid(unsafe_code)]

mod fact;
mod result;

// Category-specific binder entrypoints will consume this task-local construction layer.
#[expect(
    dead_code,
    reason = "category-specific binder entrypoints do not consume this task-local layer yet"
)]
mod unit;

pub use bray_checker::CheckerOutcome as BindingOutcome;
pub use fact::{
    BinderCancellation, BinderFactContext, BinderFactError, BinderFactResult,
    BindingSymbolFactProvider, SymbolFactProvider, TargetFactProvider, TargetFactResult,
};
pub use result::{
    CheckedAnonymousCallableResult, CheckedCallableBodyResult, CheckedConstantTemplateResult,
    CheckedConstraintResult, CheckedContractClauseResult, CheckedPredicateDefinitionResult,
    CheckedRuntimeDefaultResult,
};
