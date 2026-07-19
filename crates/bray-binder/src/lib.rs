//! Name binding and semantic reference resolution.

#![forbid(unsafe_code)]

mod check_entry;
mod entry;
mod fact;
mod publication;
mod surface;

// TODO(binder): Narrow and remove this expectation as candidate binding becomes production-used.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "candidate binding remains reserved for later semantic decisions"
    )
)]
mod binding;
mod result;

// TODO(binder): Narrow and remove this expectation as typed lookup categories become production-used.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "some typed lookup categories are reserved for later semantic decisions"
    )
)]
mod lookup;

// TODO(binder): Narrow and remove this expectation as remaining binding contexts become production-used.
#[expect(
    dead_code,
    reason = "some binding contexts are reserved for later semantic decisions"
)]
mod binder;

// TODO(binder): Narrow and remove this expectation as remaining local builders become production-used.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "some local builder operations are reserved for later semantic decisions"
    )
)]
mod unit;

pub use binder::BinderDependency;
pub use binding::{
    BoundExpressionCheckInput, CallableTypeQualifiers, TypeExpressionBinder, TypeParameterBinding,
    bind_callable_abi, bind_expression_check_input,
};
pub use bray_checker::CheckerOutcome as BindingOutcome;
pub use check_entry::{UnitCheckEntryContextError, unit_check_entry_context};
pub use entry::{
    BoundUnitBindingError, PendingBoundAnonymousCallable, PendingBoundCallableBody,
    PendingBoundConstantTemplate, PendingBoundConstraint, PendingBoundContractClause,
    PendingBoundPredicateDefinition, PendingBoundRuntimeDefault, bind_anonymous_callable,
    bind_callable_body, bind_constant_template, bind_constraint, bind_contract_clause,
    bind_predicate_definition, bind_runtime_default,
};
pub use fact::{
    BinderFactContext, BinderFactError, BinderFactResult, BindingSymbolFactProvider,
    SymbolFactProvider, TargetFactProvider, TargetFactResult,
};
pub use result::BoundUnitComputation;
pub use surface::{
    PredicateClauseBindingContext, bind_predicate_clause, bind_trusted_capability_clause,
};
