use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::{StorageMutation, StorageTarget, StorageValuePhase, StorageValueBoundary, StorageValueRetirement};
use super::model::StorageFlow;

impl StorageFlow {
    /// Retains phase-specific physical retirements for replay of the same captured values.
    pub fn with_capture_retirements(mut self, retirements: BTreeMap<(StorageMutation, StorageValuePhase, StorageValueBoundary), BTreeSet<(StorageTarget, StorageValueRetirement)>>) -> Self {
        self.capture_retirements = retirements.into_iter().map(|(point, values)| (point, values.into_iter().collect())).collect();

        self
    }

    /// Returns retirements at this boundary. Replaying another phase cannot retire newly produced data.
    pub fn capture_retirements(&self, mutation: StorageMutation, phase: StorageValuePhase, boundary: StorageValueBoundary) -> &[(StorageTarget, StorageValueRetirement)] {
        self.capture_retirements.get(&(mutation, phase, boundary)).map_or(&[], Arc::as_ref)
    }
}
