//! Name binding and semantic reference resolution.

#![forbid(unsafe_code)]

mod result;
mod unit;

pub use bray_checker::CheckerOutcome as BindingOutcome;
pub use result::{
    CheckedAnonymousCallableResult, CheckedCallableBodyResult, CheckedConstantTemplateResult,
    CheckedConstraintResult, CheckedContractClauseResult, CheckedPredicateDefinitionResult,
    CheckedRuntimeDefaultResult,
};
pub use unit::{
    AnonymousCallableBoundary, BoundUnitConstructionError, BoundUnitConstructionResult,
    BoundUnitLocalBuilder,
};
