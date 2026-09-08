mod check;
mod condition;
mod constraint;
mod mismatch;
mod types;

pub(super) use check::{
    CompatibilityContext, fulfillment_is_compatible, subject_lifecycle_is_compatible,
};
pub(super) use mismatch::{
    CallableBehaviorComponent, CallableBehaviorPhase, CallableContractClauseCategory,
    CallableContractMismatch, ConstraintCategory, GenericConstraintMismatch,
    GenericParameterCategory, GenericSurfaceMismatch, TraitFulfillmentMismatch,
};
