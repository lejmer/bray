//! Selection of one applicable semantic target from binder-enumerated candidates.
//!
//! Candidate sets include callable overload arms, members, operators, indexing contracts,
//! construction targets, conversions, and implementation witnesses. Selection either retains the
//! exact chosen target or classifies why no unique candidate could be chosen.

mod call;
mod capacity;
mod check;
mod iteration;
mod model;
mod operation;
mod order;
mod propagation;

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
    OperationSelectionRequest, PredicateCandidateTemplate, ReceiverCapability, ReceiverSelection,
    SelectionCallableArgumentRejection, SelectionCandidateKey, SelectionCandidateRejectionReason,
    SelectionCandidateSignature, SelectionConstructionInputRejection, SelectionFailure,
    SelectionFailureCandidate, SelectionInaccessibility, SelectionRejectedCandidate,
};
pub use operation::{
    built_in_conversion_plan, built_in_conversion_plan_for_context, built_in_operation_result_type,
    built_in_operator_supported, built_in_trait_constraint_outcome, compiler_known_operation_role,
    composite_conversion_children,
};
pub(crate) use operation::{
    callable_contract_conversion_is_valid, representation_supports_operator,
};
pub(crate) use propagation::select_propagations;
