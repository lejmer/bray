use std::sync::Arc;

use bray_target::TargetIdentity;

use crate::{
    ArtifactContent, ArtifactDigest, BackendArtifactId, BackendIdentity, CodegenRequest,
    CodegenUnitKey,
};

/// One immutable logically identified artifact contribution produced by a backend.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackendArtifactContribution {
    id: BackendArtifactId,
    content: ArtifactContent,
    backend: BackendIdentity,
    target: TargetIdentity,
    digest: Option<ArtifactDigest>,
}

impl BackendArtifactContribution {
    /// Creates one complete backend artifact contribution.
    pub const fn new(
        id: BackendArtifactId,
        content: ArtifactContent,
        backend: BackendIdentity,
        target: TargetIdentity,
        digest: Option<ArtifactDigest>,
    ) -> Self {
        Self {
            id,
            content,
            backend,
            target,
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

    /// Returns the producing backend identity.
    pub const fn backend(&self) -> &BackendIdentity {
        &self.backend
    }

    /// Returns the target identity used to produce the artifact.
    pub const fn target(&self) -> &TargetIdentity {
        &self.target
    }

    /// Returns the deterministic content digest when one was requested.
    pub const fn digest(&self) -> Option<&ArtifactDigest> {
        self.digest.as_ref()
    }
}

/// Complete immutable artifact contributions for one authoritative codegen request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackendArtifactSet {
    unit: CodegenUnitKey,
    backend: BackendIdentity,
    target: TargetIdentity,
    contributions: Arc<[BackendArtifactContribution]>,
}

impl BackendArtifactSet {
    pub(crate) fn try_new(
        request: CodegenRequest<'_>,
        contributions: impl IntoIterator<Item = BackendArtifactContribution>,
    ) -> Result<Self, BackendArtifactSetBuildError> {
        let mut contributions: Vec<_> = contributions.into_iter().collect();

        contributions.sort_unstable_by(|left, right| left.id().cmp(right.id()));

        if let Some(pair) = contributions
            .windows(2)
            .find(|pair| pair[0].id() == pair[1].id())
        {
            // Validation errors retain the Arc-backed planned identity after this borrow ends.
            return Err(BackendArtifactSetBuildError::DuplicateArtifact(
                pair[0].id().clone(),
            ));
        }

        for contribution in &contributions {
            validate_contribution(request, contribution)?;
        }

        for required in request.artifacts().required() {
            if contributions
                .binary_search_by(|contribution| contribution.id().cmp(required.id()))
                .is_err()
            {
                // Validation errors retain the Arc-backed planned identity after this borrow ends.
                return Err(BackendArtifactSetBuildError::MissingRequired(
                    required.id().clone(),
                ));
            }
        }

        Ok(Self {
            // Complete sets retain Arc-backed structural identities after the request borrow ends.
            unit: request.unit().key().clone(),
            backend: request.backend().clone(),
            target: request.target().identity().clone(),
            contributions: contributions.into(),
        })
    }

    /// Returns the codegen unit represented by this complete set.
    pub const fn unit(&self) -> &CodegenUnitKey {
        &self.unit
    }

    /// Returns the backend that produced every contribution.
    pub const fn backend(&self) -> &BackendIdentity {
        &self.backend
    }

    /// Returns the target shared by every contribution.
    pub const fn target(&self) -> &TargetIdentity {
        &self.target
    }

    /// Returns contributions in canonical logical-identity order.
    pub fn contributions(&self) -> &[BackendArtifactContribution] {
        &self.contributions
    }
}

/// A contract violation that prevents atomic artifact-set publication.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BackendArtifactSetBuildError {
    /// Two contributions have the same planned logical identity.
    DuplicateArtifact(BackendArtifactId),
    /// A required planned contribution was not produced.
    MissingRequired(BackendArtifactId),
    /// A contribution was not present in the emitter-derived request.
    UnrequestedArtifact(BackendArtifactId),
    /// A contribution was produced by another backend identity.
    BackendMismatch(BackendArtifactId),
    /// A contribution was produced for another target identity.
    TargetMismatch(BackendArtifactId),
}

fn validate_contribution(
    request: CodegenRequest<'_>,
    contribution: &BackendArtifactContribution,
) -> Result<(), BackendArtifactSetBuildError> {
    let id = contribution.id();

    if request.artifacts().entry(id).is_none() {
        // Validation errors retain the Arc-backed planned identity after this borrow ends.
        return Err(BackendArtifactSetBuildError::UnrequestedArtifact(
            id.clone(),
        ));
    }

    if contribution.backend() != request.backend() {
        return Err(BackendArtifactSetBuildError::BackendMismatch(id.clone()));
    }

    if contribution.target() != request.target().identity() {
        return Err(BackendArtifactSetBuildError::TargetMismatch(id.clone()));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{BackendArtifactContribution, BackendArtifactSet, BackendArtifactSetBuildError};
    use crate::test_support::{artifact_content, codegen_request, contribution};
    use crate::{BackendArtifactId, BackendArtifactKind};

    #[test]
    fn artifact_sets_validate_against_authoritative_request_identities() {
        let fixture = codegen_request();

        assert_eq!(
            BackendArtifactSet::try_new(fixture.request(), []),
            Err(BackendArtifactSetBuildError::MissingRequired(
                fixture.required_artifact().clone()
            ))
        );

        let foreign_id = BackendArtifactId::new(
            fixture.request().unit().key().clone(),
            BackendArtifactKind::Assembly,
            7,
        );
        let foreign = BackendArtifactContribution::new(
            foreign_id.clone(),
            artifact_content(),
            fixture.request().backend().clone(),
            fixture.request().target().identity().clone(),
            None,
        );

        assert_eq!(
            BackendArtifactSet::try_new(fixture.request(), [foreign]),
            Err(BackendArtifactSetBuildError::UnrequestedArtifact(
                foreign_id
            ))
        );
    }

    #[test]
    fn artifact_sets_retain_multiple_same_kind_logical_contributions() {
        let fixture = codegen_request();
        let first = contribution(&fixture, fixture.required_artifact().clone());
        let second = contribution(&fixture, fixture.optional_artifact().clone());

        let Ok(set) = BackendArtifactSet::try_new(fixture.request(), [second, first]) else {
            panic!("requested test contributions must form a complete set");
        };

        assert_eq!(set.contributions().len(), 2);
        assert_eq!(set.contributions()[0].id(), fixture.required_artifact());
        assert_eq!(set.contributions()[1].id(), fixture.optional_artifact());
    }
}
