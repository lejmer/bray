//! Name binding and semantic reference resolution.

#![forbid(unsafe_code)]

mod fact;
// TODO(binder): Remove this expectation when BRA-117 connects expression binding to these
// grammar-facing block, pattern, and anonymous-callable binders.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "expression binding is the next consumer of these grammar-facing binders"
    )
)]
mod binding;
mod result;

// TODO(binder): Remove this expectation when category-specific entrypoints consume name lookup.
#[expect(
    dead_code,
    reason = "category-specific binder entrypoints do not consume name lookup yet"
)]
mod lookup;

// TODO(binder): Remove this expectation when category-specific entrypoints construct transactions.
#[expect(
    dead_code,
    reason = "category-specific binder entrypoints do not consume request transactions yet"
)]
mod request;

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
pub use request::BinderDependency;
pub use result::{
    CheckedAnonymousCallableResult, CheckedCallableBodyResult, CheckedConstantTemplateResult,
    CheckedConstraintResult, CheckedContractClauseResult, CheckedPredicateDefinitionResult,
    CheckedRuntimeDefaultResult, CheckedUnitComputation,
};
