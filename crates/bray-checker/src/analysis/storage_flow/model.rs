mod domain;
mod input;
mod state;

#[cfg(test)]
mod tests;

pub(super) use domain::StorageFlowDomain;
pub(super) use input::StorageFlowInput;
pub(in crate::analysis) use state::{StorageAbsence, StorageFlowState};

pub(super) use state::{EffectPrefix, merge_fully_moved};
