//! Semantic checking for bound Bray programs.

#![forbid(unsafe_code)]

mod analysis;
mod cancellation;
mod outcome;
mod request;
mod service;
#[cfg(test)]
mod test_support;

pub use cancellation::CheckerCancellation;
pub use outcome::{
    AnonymousCallableCheckConclusions, CallableBodyCheckConclusions, CheckerOutcome,
    ConstantTemplateCheckConclusions, ConstraintCheckConclusions, ContractClauseCheckConclusions,
    PredicateDefinitionCheckConclusions, RuntimeDefaultCheckConclusions, UnitCheckConclusions,
};
pub use request::{UnitCheckRequest, UnitCheckRequestError, UnitCheckRoot};
pub use service::UnitChecker;
