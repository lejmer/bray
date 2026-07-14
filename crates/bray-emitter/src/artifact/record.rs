use std::sync::Arc;

use bray_codegen::ArtifactDigest;

use crate::{
    ArtifactId, ArtifactProducer, ArtifactRequirement, ArtifactRole, EmissionPlan, OutputSink,
    PlannedArtifactDestination, ProductIdentity,
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
    /// Creates one completed publication record before complete-set validation.
    pub const fn new(
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

/// Complete canonically ordered externally published artifacts for one emission plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmittedArtifactSet {
    product: ProductIdentity,
    artifacts: Arc<[EmittedArtifact]>,
}

impl EmittedArtifactSet {
    /// Validates and freezes complete externally published records for one plan.
    pub fn try_new(
        plan: &EmissionPlan,
        artifacts: impl IntoIterator<Item = EmittedArtifact>,
    ) -> Result<Self, EmittedArtifactSetBuildError> {
        let mut artifacts: Vec<_> = artifacts.into_iter().collect();

        artifacts.sort_unstable_by(|left, right| left.id().cmp(right.id()));

        if let Some(pair) = artifacts
            .windows(2)
            .find(|pair| pair[0].id() == pair[1].id())
        {
            // Set errors retain Arc-backed artifact identities after validation returns.
            return Err(EmittedArtifactSetBuildError::DuplicateArtifact(
                pair[0].id().clone(),
            ));
        }

        for artifact in &artifacts {
            validate_artifact(plan, artifact)?;
        }

        for planned in plan.published_artifacts() {
            if planned.requirement() == ArtifactRequirement::Required
                && artifacts
                    .binary_search_by(|artifact| artifact.id().cmp(planned.id()))
                    .is_err()
            {
                // Set errors retain Arc-backed artifact identities after validation returns.
                return Err(EmittedArtifactSetBuildError::MissingRequired(
                    planned.id().clone(),
                ));
            }
        }

        Ok(Self {
            // Complete sets retain the Arc-backed product identity independently of the plan.
            product: plan.request().product().clone(),
            artifacts: artifacts.into(),
        })
    }

    /// Returns the product represented by this complete publication set.
    pub const fn product(&self) -> &ProductIdentity {
        &self.product
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

/// A contract violation that prevents complete publication records from being exposed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EmittedArtifactSetBuildError {
    /// One logical artifact identity appears more than once.
    DuplicateArtifact(ArtifactId),
    /// A required externally published artifact has no completed record.
    MissingRequired(ArtifactId),
    /// A completed record was not an externally published artifact in the plan.
    UnplannedArtifact(ArtifactId),
    /// The completed record names a different destination than the plan.
    SinkMismatch(ArtifactId),
    /// The completed record names a different producer than the plan.
    ProducerMismatch(ArtifactId),
    /// The completed record names a different product-lifecycle role than the plan.
    RoleMismatch(ArtifactId),
}

fn validate_artifact(
    plan: &EmissionPlan,
    artifact: &EmittedArtifact,
) -> Result<(), EmittedArtifactSetBuildError> {
    let Some(planned) = plan.artifact(artifact.id()) else {
        // Set errors retain Arc-backed artifact identities after validation returns.
        return Err(EmittedArtifactSetBuildError::UnplannedArtifact(
            artifact.id().clone(),
        ));
    };

    let PlannedArtifactDestination::Publish(sink) = planned.destination() else {
        // Set errors retain Arc-backed artifact identities after validation returns.
        return Err(EmittedArtifactSetBuildError::UnplannedArtifact(
            artifact.id().clone(),
        ));
    };

    if artifact.sink() != sink {
        // Set errors retain Arc-backed artifact identities after validation returns.
        return Err(EmittedArtifactSetBuildError::SinkMismatch(
            artifact.id().clone(),
        ));
    }

    if artifact.producer() != planned.producer() {
        // Set errors retain Arc-backed artifact identities after validation returns.
        return Err(EmittedArtifactSetBuildError::ProducerMismatch(
            artifact.id().clone(),
        ));
    }

    if artifact.role() != planned.role() {
        // Set errors retain Arc-backed artifact identities after validation returns.
        return Err(EmittedArtifactSetBuildError::RoleMismatch(
            artifact.id().clone(),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{EmittedArtifactSet, EmittedArtifactSetBuildError};
    use crate::test_support::{emission_plan, emitted_artifact};

    #[test]
    fn emitted_sets_require_every_planned_external_artifact() {
        let plan = emission_plan();

        assert_eq!(
            EmittedArtifactSet::try_new(&plan, []),
            Err(EmittedArtifactSetBuildError::MissingRequired(
                required_artifact_id(&plan)
            ))
        );

        let artifact = emitted_artifact(&plan);

        let Ok(set) = EmittedArtifactSet::try_new(&plan, [artifact]) else {
            panic!("matching test artifact must complete the plan");
        };

        assert_eq!(set.product(), plan.request().product());
        assert_eq!(set.artifacts().len(), 1);
    }

    fn required_artifact_id(plan: &crate::EmissionPlan) -> crate::ArtifactId {
        let Some(artifact) = plan.published_artifacts().next() else {
            panic!("test plan must publish an artifact");
        };

        artifact.id().clone()
    }
}
