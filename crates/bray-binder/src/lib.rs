//! Name binding and semantic reference resolution.

#![forbid(unsafe_code)]

mod entry;
mod fact;
mod publication;
mod semantic_context;
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
    CallableTypeQualifiers, TypeExpressionBinder, TypeExpressionScope, TypeParameterBinding,
    bind_callable_abi,
};
pub use bray_checker::CheckerOutcome as BindingOutcome;
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
pub use semantic_context::{SemanticUnitContextError, semantic_unit_context};
pub use surface::{
    PredicateClauseBindingContext, bind_predicate_clause, bind_trusted_capability_clause,
};
