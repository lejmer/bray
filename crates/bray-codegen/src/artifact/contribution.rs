use std::sync::Arc;

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

/// Complete immutable artifact contributions for one authoritative codegen request.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct BackendArtifactSet {
    contributions: Arc<[BackendArtifactContribution]>,
}

impl BackendArtifactSet {
    pub(crate) fn new(
        contributions: impl IntoIterator<Item = BackendArtifactContribution>,
    ) -> Self {
        let mut contributions: Vec<_> = contributions.into_iter().collect();

        contributions.sort_unstable_by(|left, right| left.id().cmp(right.id()));

        Self {
            contributions: contributions.into(),
        }
    }

    /// Returns contributions in canonical logical-identity order.
    pub fn contributions(&self) -> &[BackendArtifactContribution] {
        &self.contributions
    }
}

#[cfg(test)]
mod tests {
    use super::BackendArtifactSet;
    use crate::test_support::{codegen_request, contribution};

    #[test]
    fn artifact_sets_retain_multiple_same_kind_logical_contributions() {
        let fixture = codegen_request();
        let first = contribution(fixture.required_artifact().clone());
        let second = contribution(fixture.optional_artifact().clone());

        let set = BackendArtifactSet::new([second, first]);

        assert_eq!(set.contributions().len(), 2);
        assert_eq!(set.contributions()[0].id(), fixture.required_artifact());
        assert_eq!(set.contributions()[1].id(), fixture.optional_artifact());
    }
}
