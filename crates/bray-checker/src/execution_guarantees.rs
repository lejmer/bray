mod clause;
mod condition;
mod declaration;
mod implication;
mod model;
mod normalize;
mod place;
mod portable;
mod trusted_implication;
mod unsupported;

pub use bray_symbols::ExecutionProperty;
pub use condition::ExecutionCondition;
pub(crate) use condition::ObservationSet;
pub use declaration::declared_execution_properties;
pub use implication::{
    execution_condition_is_implied, map_execution_condition_references, substitute_execution_condition,
    remap_execution_condition_inputs,
};
pub(crate) use model::ExecutionValueEvidence;
pub use model::{
    DeclaredExecutionProperty, ExecutionCallEvidence, ExecutionCandidate, ExecutionCandidates,
    ExecutionCertification, ExecutionCleanup, ExecutionCompletionContract,
    ExecutionCompletionDependency, ExecutionDeclaration, ExecutionDependency, ExecutionDependencyTarget, ExecutionDomain,
    ExecutionObligation,
};
pub(crate) use normalize::{
    condition_literals, expression_condition, expression_place, pattern_conditions, pattern_value,
};
pub use normalize::{execution_conditions, predicate_conditions, predicate_conditions_with_inputs};
pub(crate) use place::ExecutionInput;
pub use place::ExecutionPlace;
pub use unsupported::check_execution_guarantees;

pub use portable::{
    execution_condition_from_term, execution_condition_from_type_term, execution_condition_term,
};

pub use clause::ExecutionClauseId;

pub(crate) use trusted_implication::trusted_condition_closure;
