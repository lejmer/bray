//! Name binding and semantic reference resolution.

#![forbid(unsafe_code)]

mod binder;
mod binding;
mod entry;
mod fact;
mod lookup;
mod publication;
mod result;
mod semantic_context;
mod surface;
mod unit;

pub use binder::BinderDependency;
pub use binding::{
    CallableTypeQualifiers, TypeExpressionBinder, TypeExpressionImports, TypeExpressionScope,
    TypeParameterBinding, bind_callable_abi, bind_callable_type_directives,
    bind_directive_template, bind_expression_candidates, malformed_directive_argument_diagnostic,
    qualified_union_variant,
};
pub use bray_checker::CheckerOutcome as BindingOutcome;
pub use entry::{
    BoundUnitBindingError, PendingBoundAnonymousCallable, PendingBoundCallableBody,
    PendingBoundConstantTemplate, PendingBoundConstraint, PendingBoundContractClause,
    PendingBoundEmbeddedConstant, PendingBoundPredicateDefinition, PendingBoundRuntimeDefault,
    PendingBoundTargetGate, bind_anonymous_callable, bind_callable_body, bind_constant_template,
    bind_constraint, bind_contract_clause, bind_embedded_constant, bind_predicate_definition,
    bind_runtime_default, bind_target_gate,
};
pub use fact::{
    BinderFactContext, BinderFactError, BinderFactResult, BindingSymbolFactProvider,
    ImportedPathRoot, SymbolFactProvider,
};
pub use lookup::{
    BoundImplementationUsing, NameAccess, bind_implementation_using,
    bind_named_trait_implementation_path, bind_surface_path_with_re_exports,
};
pub use result::BoundUnitComputation;
pub use semantic_context::{SemanticUnitContextError, semantic_unit_context};
pub use surface::{
    BoundTrustedCapability, PredicateClauseBindingContext, bind_predicate_clause,
    bind_trusted_capability_clause,
};
