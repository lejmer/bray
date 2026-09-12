mod condition;
mod declaration;
mod model;
mod normalize;
mod place;
mod proof;
mod unsupported;

pub use condition::ExecutionCondition;
pub use declaration::declared_execution_properties;
pub use model::{
    DeclaredExecutionProperty, ExecutionCallEvidence, ExecutionCandidate, ExecutionCandidates,
    ExecutionCertification, ExecutionCompletionContract, ExecutionCompletionDependency,
    ExecutionDeclaration, ExecutionDependency, ExecutionDomain, ExecutionObligation,
    ExecutionProperty,
};
pub use normalize::execution_conditions;
pub(crate) use normalize::{condition_literals, expression_condition, expression_place};
pub use place::ExecutionPlace;
pub use proof::{ExecutionProofFailure, check_execution_proof_dependencies};
pub use unsupported::check_execution_guarantees;
