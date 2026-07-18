//! Semantic checking for bound Bray programs.

#![forbid(unsafe_code)]

mod analysis;
mod context;
mod entry;
mod outcome;
mod request;
mod service;
mod type_check;

#[cfg(test)]
mod test_support;

pub use context::{
    CheckerFactError, CheckerFactResult, CheckerInfrastructureError, CheckerRequestContext,
    CheckerSemanticFactProvider, CheckerSource,
};
pub use entry::{
    AnonymousCallableCheckEntry, ContractClauseCheckEntry, DeclaredUnitCheckEntry,
    UnitCheckEntryContext,
};
pub use outcome::{CheckerOutcome, ControlFlowCheckResult};
pub use request::{UnitCheckRequest, UnitCheckRequestError, UnitCheckRoot};
pub use service::{
    ControlFlowChecker, DefaultControlFlowChecker, DefaultExpressionTypeChecker,
    ExpressionTypeChecker,
};
pub use type_check::{
    ExpressionTypeCheckResult, ExpressionTypeEntry, ExpressionTypeEvidence,
    ExpressionTypeExpectation, ExpressionTypeInput, ExpressionTypeResult, ExpressionTypeStatus,
};
