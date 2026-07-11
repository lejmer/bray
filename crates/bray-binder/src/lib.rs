//! Name binding and semantic reference resolution.

#![forbid(unsafe_code)]

mod result;

pub use bray_checker::{
    CheckerCancellation as BinderCancellation, CheckerOutcome as BindingOutcome,
};
pub use result::{
    CheckedAnonymousCallableResult, CheckedCallableBodyResult, CheckedConstantTemplateResult,
    CheckedConstraintResult, CheckedContractClauseResult, CheckedPredicateDefinitionResult,
    CheckedRuntimeDefaultResult,
};
