//! Name binding and semantic reference resolution.

#![forbid(unsafe_code)]

mod entry;
mod fact;
mod publication;
// TODO(binder): Remove this expectation when checked-unit fact providers call the binders.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "expression binding is the next consumer of these grammar-facing binders"
    )
)]
mod binding;
mod result;

// TODO(binder): Remove this expectation when checked-unit fact providers expose binder entrypoints.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "checked-unit fact providers are the next production name-lookup consumers"
    )
)]
mod lookup;

// TODO(binder): Remove this expectation when checked-unit fact providers create requests.
#[expect(
    dead_code,
    reason = "checked-unit fact providers are the next production request consumers"
)]
mod request;

// TODO(binder): Remove this expectation when checked-unit fact providers construct units.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "checked-unit fact providers are the next production unit consumers"
    )
)]
mod unit;

pub use bray_checker::CheckerOutcome as BindingOutcome;
pub use entry::{
    CheckedUnitBindingError, PendingCheckedAnonymousCallable, PendingCheckedCallableBody,
    PendingCheckedConstantTemplate, PendingCheckedConstraint, PendingCheckedContractClause,
    PendingCheckedPredicateDefinition, PendingCheckedRuntimeDefault, bind_anonymous_callable,
    bind_callable_body, bind_constant_template, bind_constraint, bind_contract_clause,
    bind_predicate_definition, bind_runtime_default,
};
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
