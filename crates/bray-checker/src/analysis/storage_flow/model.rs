mod domain;
mod input;
mod outcomes;
mod state;

#[cfg(test)]
mod tests;

pub(super) use domain::StorageFlowDomain;
pub(crate) use input::StorageFlowInput;
pub(super) use outcomes::StorageFlowOutcomes;
pub(in crate::analysis) use state::{ReachedInputWrites, StorageFlowState};

pub(super) use state::{AdmittedFailure, EffectPrefix, merge_fully_moved};
