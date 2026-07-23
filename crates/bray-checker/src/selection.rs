//! Selection of one applicable semantic target from binder-enumerated candidates.
//!
//! Candidate sets include callable overload arms, members, operators, indexing contracts,
//! construction targets, conversions, and implementation witnesses. Selection either retains the
//! exact chosen target or classifies why no unique candidate could be chosen.

mod call;
mod check;
mod iteration;
mod model;
mod operation;
mod order;

pub(crate) use call::{map_argument_parameter_indices, viable_candidate_indices};
pub(crate) use check::{
    select_callable, select_callable_candidates, select_iteration_source, select_operation,
};
pub(in crate::selection) use model::OperationCandidatePlan;
pub use model::{
    CallableCandidate, CallableCandidateState, CallableCandidateTemplate,
    CallableCandidateTemplateState, CallableCandidateTemplates,
    CallableDeclarationCandidateTemplate, CallableParameterDefaultTemplate,
    CallableSelectionRequest, CallableValueCandidateTemplate, CandidateAbsence, CandidateSelection,
    CompilerKnownOperationEvidence, ConstructionInputSurface, ExpressionCandidateSet,
    ImplementationSelectionEvidence, IterationSourceCandidate, IterationSourceSelectionRequest,
    OperationCandidate, OperationCandidateSource, OperationCandidateState,
    OperationSelectionRequest, ReceiverCapability, ReceiverSelection, SelectionCandidateKey,
    SelectionFailure,
};
