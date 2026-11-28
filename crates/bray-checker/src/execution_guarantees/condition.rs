mod model;
mod operands;
mod hashing;
mod interning;
mod memo;
mod context;
mod observations;
mod retention;
mod retained;
mod proof;
mod substitution;
mod traversal;
mod values;
mod tags;
mod storage;

#[cfg(test)]
mod tests;

pub use model::{ExecutionCondition, ExecutionConditionKind, ExecutionDescriptorKind, ExecutionJoinSite, ExecutionObservationKind};
pub(in crate::execution_guarantees) use model::PreparedCondition;

pub(crate) use retention::Set as ObservationSet;
pub(crate) use retained::RetainedSnapshots;
pub(crate) use context::{ExecutionProof, ExecutionProofCache};
pub(crate) use substitution::{ExecutionSubstitution, GuardSubstitution, SnapshotSubstitution};

pub(super) use context::ProofMemo;

pub use operands::ExecutionOperands;

pub(super) use memo::{ConditionMemo, cached_condition};
pub(crate) use memo::memoize_condition;

pub(crate) use observations::memoize_observations;
