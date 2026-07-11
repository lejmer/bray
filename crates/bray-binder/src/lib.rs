//! Name binding and semantic reference resolution.

#![forbid(unsafe_code)]

mod fact;
mod result;

pub use bray_checker::CheckerOutcome as BindingOutcome;
pub use fact::{
    BinderCancellation, BinderFactContext, BinderFactError, BinderFactResult,
    BindingSymbolFactProvider, ImportedSymbolFactProvider, SymbolFactProvider, TargetFactProvider,
    TargetFactResult,
};
pub use result::{
    CheckedAnonymousCallableResult, CheckedCallableBodyResult, CheckedConstantTemplateResult,
    CheckedConstraintResult, CheckedContractClauseResult, CheckedPredicateDefinitionResult,
    CheckedRuntimeDefaultResult,
};
