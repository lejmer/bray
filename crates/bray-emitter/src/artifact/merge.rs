use std::collections::BTreeMap;
use std::io;

use bray_base::Cancellation;
use bray_codegen::{
    ArtifactDigest, BackendArtifactId, BackendArtifactSet,
};

use super::content::{ContentValidationError, validate_content};
use crate::{
    ArtifactContribution, ArtifactProducer, ArtifactRequirement, BackendContributionSet,
    EmissionPlan,
};

impl BackendContributionSet {
    /// Validates content and merges completed backend sets in deterministic plan order.
    pub fn try_from_backend<'artifact>(
        plan: &EmissionPlan,
        sets: impl IntoIterator<Item = &'artifact BackendArtifactSet>,
        cancellation: &dyn Cancellation,
    ) -> Result<Self, BackendContributionMergeError> {
        let contributions: BTreeMap<_, _> = sets
            .into_iter()
            .flat_map(BackendArtifactSet::contributions)
            .map(|contribution| (contribution.id().clone(), contribution))
            .collect();

        let mut validated = BTreeMap::new();

        for planned in plan.artifacts() {
            let ArtifactProducer::Backend { artifact, .. } = planned.producer() else {
                continue;
            };

            if validated.contains_key(artifact) {
                continue;
            }

            let Some(contribution) = contributions.get(artifact) else {
                assert_eq!(
                    planned.requirement(),
                    ArtifactRequirement::Optional,
                    "complete code generation omitted required artifact {artifact:?}"
                );

                continue;
            };

            let digest =
                validate_content(contribution.content(), contribution.digest(), cancellation)
                    .map_err(|error| content_error(contribution.id(), error))?;

            validated.insert(contribution.id().clone(), (*contribution, digest));
        }

        let mut contributions = Vec::new();

        for planned in plan.artifacts() {
            let ArtifactProducer::Backend { artifact, .. } = planned.producer() else {
                continue;
            };

            let Some((contribution, digest)) = validated.get(artifact) else {
                continue;
            };

            contributions.push(ArtifactContribution::new(
                planned.id().clone(),
                planned.producer().clone(),
                // The merged set must retain immutable content after the codegen outcome borrow ends.
                contribution.content().clone(),
                Some(digest.clone()),
            ));
        }

        Ok(Self::new(contributions))
    }
}

/// Failure to validate backend contribution content.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackendContributionMergeError {
    kind: BackendContributionMergeErrorKind,
}

impl BackendContributionMergeError {
    /// Returns the exact contribution-content failure.
    pub const fn kind(&self) -> &BackendContributionMergeErrorKind {
        &self.kind
    }
}

/// Structured reason backend contributions could not be merged.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BackendContributionMergeErrorKind {
    /// Cancellation was observed while validating contribution content.
    Cancelled,
    /// Contribution content could not be read.
    Read {
        /// Backend contribution whose content failed.
        artifact: BackendArtifactId,
        /// Host I/O error category.
        kind: io::ErrorKind,
    },
    /// Contribution content length differs from its immutable metadata.
    LengthMismatch {
        /// Backend contribution whose content failed.
        artifact: BackendArtifactId,
        /// Declared byte length.
        expected: u64,
        /// Observed byte length.
        actual: u64,
    },
    /// Contribution content differs from its producer-supplied digest.
    DigestMismatch {
        /// Backend contribution whose content failed.
        artifact: BackendArtifactId,
        /// Producer-supplied digest.
        expected: ArtifactDigest,
        /// Observed digest.
        actual: ArtifactDigest,
    },
    /// Contribution metadata could not represent its validated content.
    InvalidContent(BackendArtifactId),
}

fn content_error(
    artifact: &BackendArtifactId,
    error: ContentValidationError,
) -> BackendContributionMergeError {
    let kind = match error {
        ContentValidationError::Cancelled => BackendContributionMergeErrorKind::Cancelled,
        ContentValidationError::Read(kind) => BackendContributionMergeErrorKind::Read {
            artifact: artifact.clone(),
            kind,
        },
        ContentValidationError::LengthMismatch { expected, actual } => {
            BackendContributionMergeErrorKind::LengthMismatch {
                artifact: artifact.clone(),
                expected,
                actual,
            }
        }
        ContentValidationError::DigestMismatch { expected, actual } => {
            BackendContributionMergeErrorKind::DigestMismatch {
                artifact: artifact.clone(),
                expected,
                actual,
            }
        }
        ContentValidationError::LengthOverflow | ContentValidationError::DigestConstruction => {
            BackendContributionMergeErrorKind::InvalidContent(artifact.clone())
        }
    };

    BackendContributionMergeError { kind }
}

#[cfg(test)]
mod tests {
    use bray_codegen::test_support::{codegen_request_for_backend, contribution};
    use bray_codegen::{
        ArtifactContent, ArtifactDigest, ArtifactDigestAlgorithm, BackendArtifactContribution,
        CodegenOutcome,
    };
    use bray_diagnostics::DiagnosticBag;

    use super::BackendContributionMergeErrorKind;
    use crate::test_support::{backend_artifact_plan_parts, backend_capability_revision};
    use crate::{
        ArtifactId, ArtifactKind, ArtifactRequirement, ArtifactRole, BackendContributionSet,
        EmissionPlan, PlannedArtifact, PlannedArtifactDestination,
    };

    #[test]
    fn backend_sets_merge_once_in_plan_order_for_publication_and_staging() {
        let (request, backend, published, backend_request) = backend_artifact_plan_parts();

        let staged = PlannedArtifact::new(
            ArtifactId::new(
                request.product().clone(),
                ArtifactKind::RelocatableObject,
                1,
            ),
            ArtifactRequirement::Required,
            ArtifactRole::LinkInput,
            published.producer().clone(),
            PlannedArtifactDestination::Stage,
        );

        let plan = EmissionPlan::new(
            request,
            Some(backend.clone()),
            Some(backend_capability_revision()),
            [staged, published],
            [backend_request],
            None,
        );

        let fixture = codegen_request_for_backend(backend);
        let artifact = contribution(fixture.required_artifact().clone());
        let outcome = complete_outcome([artifact]);

        let Some(artifacts) = outcome.artifacts() else {
            panic!("test code generation must complete");
        };

        let Ok(merged) =
            BackendContributionSet::try_from_backend(&plan, [artifacts], &never_cancelled)
        else {
            panic!("matching backend contributions must merge");
        };

        assert_eq!(merged.contributions().len(), 2);
        assert_eq!(merged.contributions()[0].id().ordinal(), 0);
        assert_eq!(merged.contributions()[1].id().ordinal(), 1);
        assert_eq!(merged.published(&plan).count(), 1);
        assert_eq!(merged.staged(&plan).count(), 1);

        let expected = blake3::hash(&[1_u8, 2, 3]);

        assert_eq!(
            merged.contributions()[0]
                .digest()
                .map(bray_codegen::ArtifactDigest::bytes),
            Some(expected.as_bytes().as_slice())
        );
    }

    #[test]
    fn merge_rejects_digest_mismatched_contributions() {
        let (request, backend, artifact, backend_request) = backend_artifact_plan_parts();

        let plan = EmissionPlan::new(
            request,
            Some(backend.clone()),
            Some(backend_capability_revision()),
            [artifact],
            [backend_request],
            None,
        );

        let fixture = codegen_request_for_backend(backend.clone());

        let content = ArtifactContent::try_memory([1_u8, 2, 3].as_slice())
            .unwrap_or_else(|error| panic!("test content must be valid: {error:?}"));

        let digest = ArtifactDigest::try_new(ArtifactDigestAlgorithm::Blake3, [0_u8; 32])
            .unwrap_or_else(|| panic!("test digest must be valid"));

        let mismatched = BackendArtifactContribution::new(
            fixture.required_artifact().clone(),
            content,
            Some(digest),
        );

        let mismatched = complete_outcome([mismatched]);

        let Some(mismatched) = mismatched.artifacts() else {
            panic!("test code generation must complete");
        };

        let merged =
            BackendContributionSet::try_from_backend(&plan, [mismatched], &never_cancelled);

        assert!(matches!(
            merged,
            Err(error)
                if matches!(
                    error.kind(),
                    BackendContributionMergeErrorKind::DigestMismatch { .. }
                )
        ));
    }

    fn complete_outcome(
        contributions: impl IntoIterator<Item = BackendArtifactContribution>,
    ) -> CodegenOutcome {
        CodegenOutcome::complete(contributions, DiagnosticBag::new())
    }

    fn never_cancelled() -> bool {
        false
    }
}
