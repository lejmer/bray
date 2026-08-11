use std::sync::Arc;

use bray_codegen::ArtifactDigest;

use crate::{
    ArtifactId, ArtifactProducer, ArtifactRole, EmissionPlan, OutputSink, ProductIdentity,
};

/// Immutable record of one completely published external artifact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmittedArtifact {
    id: ArtifactId,
    sink: OutputSink,
    producer: ArtifactProducer,
    role: ArtifactRole,
    byte_len: u64,
    digest: ArtifactDigest,
}

impl EmittedArtifact {
    /// Creates one record for a completely published artifact.
    pub(crate) const fn new(
        id: ArtifactId,
        sink: OutputSink,
        producer: ArtifactProducer,
        role: ArtifactRole,
        byte_len: u64,
        digest: ArtifactDigest,
    ) -> Self {
        Self {
            id,
            sink,
            producer,
            role,
            byte_len,
            digest,
        }
    }

    /// Returns the planned logical artifact identity.
    pub const fn id(&self) -> &ArtifactId {
        &self.id
    }

    /// Returns the exact destination that received the complete bytes.
    pub const fn sink(&self) -> &OutputSink {
        &self.sink
    }

    /// Returns the producer that supplied the published bytes.
    pub const fn producer(&self) -> &ArtifactProducer {
        &self.producer
    }

    /// Returns this artifact's relationship to the product lifecycle.
    pub const fn role(&self) -> ArtifactRole {
        self.role
    }

    /// Returns the exact number of published bytes.
    pub const fn byte_len(&self) -> u64 {
        self.byte_len
    }

    /// Returns the digest of the complete published content.
    pub const fn digest(&self) -> &ArtifactDigest {
        &self.digest
    }
}

/// Canonically ordered artifacts published during one emission operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmittedArtifactSet {
    product: ProductIdentity,
    target: bray_target::TargetIdentity,
    artifacts: Arc<[EmittedArtifact]>,
}

impl EmittedArtifactSet {
    pub(crate) fn from_publication(
        plan: &EmissionPlan,
        artifacts: impl IntoIterator<Item = EmittedArtifact>,
    ) -> Self {
        let mut artifacts: Vec<_> = artifacts.into_iter().collect();

        artifacts.sort_unstable_by(|left, right| left.id().cmp(right.id()));

        Self {
            // Complete sets retain the Arc-backed product identity independently of the plan.
            product: plan.request().product().clone(),
            target: plan.request().target().clone(),
            artifacts: artifacts.into(),
        }
    }

    /// Returns the product associated with this publication operation.
    pub const fn product(&self) -> &ProductIdentity {
        &self.product
    }

    /// Returns the target associated with this publication operation.
    pub const fn target(&self) -> &bray_target::TargetIdentity {
        &self.target
    }

    /// Returns emitted artifacts in canonical logical-identity order.
    pub fn artifacts(&self) -> &[EmittedArtifact] {
        &self.artifacts
    }

    /// Returns one emitted artifact by logical identity.
    pub fn artifact(&self, id: &ArtifactId) -> Option<&EmittedArtifact> {
        self.artifacts
            .binary_search_by(|artifact| artifact.id().cmp(id))
            .ok()
            .map(|index| &self.artifacts[index])
    }
}

#[cfg(test)]
mod tests {
    use super::EmittedArtifactSet;
    use crate::test_support::{emission_plan, emitted_artifact};

    #[test]
    fn emitted_sets_expose_canonically_published_artifacts() {
        let plan = emission_plan();
        let artifact = emitted_artifact(&plan);
        let set = EmittedArtifactSet::from_publication(&plan, [artifact]);

        assert_eq!(set.product(), plan.request().product());
        assert_eq!(set.artifacts().len(), 1);
    }
}
