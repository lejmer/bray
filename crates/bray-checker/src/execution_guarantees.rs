mod clause;
mod condition;
mod declaration;
mod implication;
mod model;
mod normalize;
mod place;
mod portable;
mod trusted_implication;

pub use bray_symbols::ExecutionProperty;
pub use condition::ExecutionCondition;
pub(crate) use condition::ObservationSet;
pub use declaration::declared_execution_properties;
pub use implication::{
    execution_condition_is_implied, map_execution_condition_references,
    remap_execution_condition_inputs, substitute_execution_condition,
};
pub(crate) use model::ExecutionValueEvidence;
pub use model::{
    DeclaredExecutionProperty, ExecutionCallEvidence, ExecutionCandidate, ExecutionCandidates,
    ExecutionCertification, ExecutionCleanup, ExecutionCompletionContract,
    ExecutionCompletionDependency, ExecutionDeclaration, ExecutionDependency,
    ExecutionDependencyTarget, ExecutionDomain, ExecutionObligation,
};
pub(crate) use normalize::{
    condition_literals, construction_condition, expression_condition, expression_place,
    pattern_conditions, pattern_value, selected_call_condition,
};
pub use normalize::{execution_conditions, predicate_conditions, predicate_conditions_with_inputs};
pub(crate) use place::ExecutionInput;
pub use place::ExecutionPlace;

pub use portable::{
    execution_condition_from_term, execution_condition_from_type_term, execution_condition_term,
};

pub use clause::ExecutionClauseId;

pub(crate) use trusted_implication::trusted_condition_closure;
