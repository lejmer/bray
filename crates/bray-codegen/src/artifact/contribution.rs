use crate::{ArtifactContent, ArtifactDigest, BackendArtifactId};

/// One immutable logically identified artifact contribution produced by a backend.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct BackendArtifactContribution {
    id: BackendArtifactId,
    content: ArtifactContent,
    digest: Option<ArtifactDigest>,
}

impl BackendArtifactContribution {
    /// Creates one complete backend artifact contribution.
    pub const fn new(
        id: BackendArtifactId,
        content: ArtifactContent,
        digest: Option<ArtifactDigest>,
    ) -> Self {
        Self {
            id,
            content,
            digest,
        }
    }

    /// Returns the planned logical contribution identity.
    pub const fn id(&self) -> &BackendArtifactId {
        &self.id
    }

    /// Returns the immutable artifact content.
    pub const fn content(&self) -> &ArtifactContent {
        &self.content
    }

    /// Returns the deterministic content digest when one was requested.
    pub const fn digest(&self) -> Option<&ArtifactDigest> {
        self.digest.as_ref()
    }
}
