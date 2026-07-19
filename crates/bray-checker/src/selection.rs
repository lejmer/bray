//! Selection of one applicable semantic target from binder-enumerated candidates.
//!
//! Candidate sets include callable overload arms, members, operators, indexing contracts,
//! construction targets, conversions, and implementation witnesses. Selection either retains the
//! exact chosen target or classifies why no unique candidate could be chosen.

mod call;
mod check;
mod model;
mod operation;
mod order;
mod publication;

pub(crate) use check::{select_callable, select_operation};
pub use model::{
    CallableCandidate, CallableCandidateState, CallableSelectionRequest, CandidateSelection,
    CompilerKnownOperationContract, CompilerKnownOperationEvidence, CompilerKnownOperationRole,
    ConstructionInputSurface, ImplementationSelectionEvidence, OperationCandidate,
    OperationCandidateState, OperationSelectionRequest, ReceiverCapability, ReceiverSelection,
    SelectionCandidateKey, SelectionFailure, SemanticSelectionInput,
};
pub(in crate::selection) use model::{CallableCandidateParts, OperationCandidatePlan};
pub(crate) use publication::check_semantic_selections;
