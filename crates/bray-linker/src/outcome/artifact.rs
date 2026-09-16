use std::sync::Arc;

use crate::{LinkedArtifact, StagingDestinationId};

/// Complete canonically ordered staging records for one link plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LinkedArtifactSet {
    artifacts: Arc<[LinkedArtifact]>,
}

impl LinkedArtifactSet {
    pub(super) fn new(artifacts: impl IntoIterator<Item = LinkedArtifact>) -> Self {
        let mut artifacts: Vec<_> = artifacts.into_iter().collect();

        artifacts.sort_unstable_by_key(LinkedArtifact::destination);

        Self {
            artifacts: artifacts.into(),
        }
    }

    /// Returns staged artifacts in canonical staging-identity order.
    pub fn artifacts(&self) -> &[LinkedArtifact] {
        &self.artifacts
    }

    /// Returns one completed artifact by emitter-owned staging identity.
    pub fn artifact(&self, destination: StagingDestinationId) -> Option<&LinkedArtifact> {
        self.artifacts
            .binary_search_by_key(&destination, LinkedArtifact::destination)
            .ok()
            .map(|index| &self.artifacts[index])
    }
}
