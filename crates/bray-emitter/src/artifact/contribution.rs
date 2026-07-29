use std::sync::Arc;

use bray_codegen::{ArtifactContent, ArtifactDigest};

use crate::{
    ArtifactId, ArtifactProducer, EmissionPlan, PlannedArtifactDestination,
};

/// Immutable completed content for one planned artifact identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactContribution {
    id: ArtifactId,
    producer: ArtifactProducer,
    content: ArtifactContent,
    digest: Option<ArtifactDigest>,
}

impl ArtifactContribution {
    /// Creates one complete immutable contribution before plan validation and publication.
    pub const fn new(
        id: ArtifactId,
        producer: ArtifactProducer,
        content: ArtifactContent,
        digest: Option<ArtifactDigest>,
    ) -> Self {
        Self {
            id,
            producer,
            content,
            digest,
        }
    }

    /// Returns the planned logical artifact identity.
    pub const fn id(&self) -> &ArtifactId {
        &self.id
    }

    /// Returns the producer that supplied this content.
    pub const fn producer(&self) -> &ArtifactProducer {
        &self.producer
    }

    /// Returns the immutable contribution content.
    pub const fn content(&self) -> &ArtifactContent {
        &self.content
    }

    /// Returns the validated deterministic content digest when available.
    pub const fn digest(&self) -> Option<&ArtifactDigest> {
        self.digest.as_ref()
    }
}

/// Complete backend contributions validated and ordered by one emission plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackendContributionSet {
    contributions: Arc<[ArtifactContribution]>,
}

impl BackendContributionSet {
    pub(crate) fn new(contributions: impl Into<Arc<[ArtifactContribution]>>) -> Self {
        Self {
            contributions: contributions.into(),
        }
    }

    /// Returns completed backend contributions in deterministic plan order.
    pub fn contributions(&self) -> &[ArtifactContribution] {
        &self.contributions
    }

    /// Iterates over contributions selected for external publication.
    pub fn published<'artifact>(
        &'artifact self,
        plan: &'artifact EmissionPlan,
    ) -> impl Iterator<Item = &'artifact ArtifactContribution> + 'artifact {
        self.contributions.iter().filter(|contribution| {
            plan.artifact(contribution.id()).is_some_and(|artifact| {
                matches!(
                    artifact.destination(),
                    PlannedArtifactDestination::Publish(_)
                )
            })
        })
    }

    /// Iterates over contributions retained for a later compiler operation.
    pub fn staged<'artifact>(
        &'artifact self,
        plan: &'artifact EmissionPlan,
    ) -> impl Iterator<Item = &'artifact ArtifactContribution> + 'artifact {
        self.contributions.iter().filter(|contribution| {
            plan.artifact(contribution.id()).is_some_and(|artifact| {
                artifact.destination() == &PlannedArtifactDestination::Stage
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{ArtifactContribution, BackendContributionSet};

    #[test]
    fn contributions_are_safe_to_share_without_output_handles() {
        assert_send_sync::<ArtifactContribution>();
        assert_send_sync::<BackendContributionSet>();
    }

    fn assert_send_sync<T: Send + Sync>() {}
}
