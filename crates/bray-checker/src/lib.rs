//! Semantic checking for bound Bray programs.

#![forbid(unsafe_code)]

mod analysis;
mod constant;
mod context;
mod diagnostic;
mod entry;
mod outcome;
mod representation;
mod request;
mod selection;
mod service;
mod type_check;

#[cfg(test)]
mod test_support;

pub use constant::{
    ConstantEvaluationInput, ConstantEvaluationLimits, ConstantReferenceResolution,
};
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
pub use selection::{
    CallableCandidate, CallableCandidateState, CallableSelectionRequest, CandidateSelection,
    CompilerKnownOperationContract, CompilerKnownOperationEvidence, CompilerKnownOperationRole,
    ConstructionInputSurface, ImplementationSelectionEvidence, OperationCandidate,
    OperationCandidateState, OperationSelectionRequest, ReceiverCapability, ReceiverSelection,
    SelectionCandidateKey, SelectionFailure,
};
pub use service::{
    ConstantEvaluator, ControlFlowChecker, DefaultConstantEvaluator, DefaultControlFlowChecker,
    DefaultExpressionTypeChecker, DefaultSemanticSelector, ExpressionTypeChecker, SemanticSelector,
};
pub use type_check::{ExpressionTypeEvidence, ExpressionTypeExpectation, ExpressionTypeInput};
