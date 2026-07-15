use bray_codegen::{ArtifactContent, ArtifactDigest};

use crate::{ArtifactId, ArtifactProducer};

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

    /// Returns a producer-supplied digest when one is already available.
    pub const fn digest(&self) -> Option<&ArtifactDigest> {
        self.digest.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::ArtifactContribution;

    #[test]
    fn contributions_are_safe_to_share_without_output_handles() {
        assert_send_sync::<ArtifactContribution>();
    }

    fn assert_send_sync<T: Send + Sync>() {}
}
