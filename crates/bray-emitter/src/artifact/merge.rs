use std::collections::BTreeMap;
use std::io;
use std::sync::Arc;

use bray_base::Cancellation;
use bray_codegen::{
    ArtifactDigest, BackendArtifactContribution, BackendArtifactId, BackendArtifactSet,
    CodegenUnitKey,
};

use super::content::{ContentValidationError, validate_content};
use crate::{
    ArtifactContribution, ArtifactId, ArtifactKind, ArtifactProducer, ArtifactRequirement,
    BackendContributionSet, EmissionPlan,
};

impl BackendContributionSet {
    /// Validates and merges completed backend sets in deterministic plan order.
    pub fn try_from_backend<'artifact>(
        plan: &EmissionPlan,
        sets: impl IntoIterator<Item = &'artifact BackendArtifactSet>,
        cancellation: &dyn Cancellation,
    ) -> Result<Self, BackendContributionMergeError> {
        let mut sets: Vec<_> = sets.into_iter().collect();

        sets.sort_unstable_by(|left, right| left.unit().cmp(right.unit()));

        validate_sets(plan, &sets)?;

        let mut validated = BTreeMap::new();

        for set in &sets {
            for contribution in set.contributions() {
                validate_backend_contribution(plan, contribution)?;

                let digest =
                    validate_content(contribution.content(), contribution.digest(), cancellation)
                        .map_err(|error| content_error(plan, contribution.id(), error))?;

                validated.insert(contribution.id(), (contribution, digest));
            }
        }

        let mut contributions = Vec::new();

        for planned in plan.artifacts() {
            let ArtifactProducer::Backend { artifact, .. } = planned.producer() else {
                continue;
            };

            let Some((contribution, digest)) = validated.get(artifact) else {
                if planned.requirement() == ArtifactRequirement::Required {
                    return Err(merge_error(
                        planned.id(),
                        BackendContributionMergeErrorKind::MissingArtifact(artifact.clone()),
                    ));
                }

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

/// A backend contribution conflict with an immutable emission plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackendContributionMergeError {
    artifact: Option<ArtifactId>,
    kind: Arc<BackendContributionMergeErrorKind>,
}

impl BackendContributionMergeError {
    /// Returns the planned artifact associated with the failure when one exists.
    pub const fn artifact(&self) -> Option<&ArtifactId> {
        self.artifact.as_ref()
    }

    /// Returns the exact contribution contract violation.
    pub fn kind(&self) -> &BackendContributionMergeErrorKind {
        &self.kind
    }
}

/// Structured reason backend contributions could not be merged.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BackendContributionMergeErrorKind {
    /// Cancellation was observed while validating contribution content.
    Cancelled,
    /// Two completed sets cover the same code generation unit.
    DuplicateUnit(CodegenUnitKey),
    /// A planned code generation unit has no completed set.
    MissingUnit(CodegenUnitKey),
    /// A completed set belongs to a unit absent from the plan.
    UnrequestedUnit(CodegenUnitKey),
    /// A completed set was produced by another backend.
    BackendMismatch(CodegenUnitKey),
    /// A completed set was produced under another backend capability contract.
    CapabilityRevisionMismatch(CodegenUnitKey),
    /// A completed set was produced for another target.
    TargetMismatch(CodegenUnitKey),
    /// A required planned artifact was not produced.
    MissingArtifact(BackendArtifactId),
    /// A backend contribution is not represented by the plan.
    UnrequestedArtifact(BackendArtifactId),
    /// Backend and emitter artifact categories disagree.
    ArtifactKindMismatch(BackendArtifactId),
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

fn validate_sets(
    plan: &EmissionPlan,
    sets: &[&BackendArtifactSet],
) -> Result<(), BackendContributionMergeError> {
    if let Some(pair) = sets
        .windows(2)
        .find(|pair| pair[0].unit() == pair[1].unit())
    {
        return Err(unit_error(
            BackendContributionMergeErrorKind::DuplicateUnit(pair[0].unit().clone()),
        ));
    }

    for set in sets {
        if plan.backend_request(set.unit()).is_none() {
            return Err(unit_error(
                BackendContributionMergeErrorKind::UnrequestedUnit(set.unit().clone()),
            ));
        }

        if plan
            .backend()
            .is_none_or(|backend| backend != set.backend())
        {
            return Err(unit_error(
                BackendContributionMergeErrorKind::BackendMismatch(set.unit().clone()),
            ));
        }

        if plan.capability_revision() != Some(set.capability_revision()) {
            return Err(unit_error(
                BackendContributionMergeErrorKind::CapabilityRevisionMismatch(set.unit().clone()),
            ));
        }

        if set.target() != plan.request().target() {
            return Err(unit_error(
                BackendContributionMergeErrorKind::TargetMismatch(set.unit().clone()),
            ));
        }
    }

    for request in plan.backend_requests() {
        if sets
            .binary_search_by(|set| set.unit().cmp(request.unit()))
            .is_err()
        {
            return Err(unit_error(BackendContributionMergeErrorKind::MissingUnit(
                request.unit().clone(),
            )));
        }
    }

    Ok(())
}

fn validate_backend_contribution(
    plan: &EmissionPlan,
    contribution: &BackendArtifactContribution,
) -> Result<(), BackendContributionMergeError> {
    let mut matches = plan
        .artifacts()
        .iter()
        .filter(|planned| planned.producer().backend_artifact() == Some(contribution.id()));

    let Some(first) = matches.next() else {
        return Err(unit_error(
            BackendContributionMergeErrorKind::UnrequestedArtifact(contribution.id().clone()),
        ));
    };

    let expected_kind = ArtifactKind::from(contribution.id().kind());

    if first.id().kind() != expected_kind {
        return Err(merge_error(
            first.id(),
            BackendContributionMergeErrorKind::ArtifactKindMismatch(contribution.id().clone()),
        ));
    }

    if let Some(planned) = matches.find(|planned| planned.id().kind() != expected_kind) {
        return Err(merge_error(
            planned.id(),
            BackendContributionMergeErrorKind::ArtifactKindMismatch(contribution.id().clone()),
        ));
    }

    Ok(())
}

fn content_error(
    plan: &EmissionPlan,
    artifact: &BackendArtifactId,
    error: ContentValidationError,
) -> BackendContributionMergeError {
    let planned = plan
        .artifacts()
        .iter()
        .find(|planned| planned.producer().backend_artifact() == Some(artifact))
        .map(|planned| planned.id().clone());

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

    BackendContributionMergeError {
        artifact: planned,
        kind: Arc::new(kind),
    }
}

fn unit_error(kind: BackendContributionMergeErrorKind) -> BackendContributionMergeError {
    BackendContributionMergeError {
        artifact: None,
        kind: Arc::new(kind),
    }
}

fn merge_error(
    artifact: &ArtifactId,
    kind: BackendContributionMergeErrorKind,
) -> BackendContributionMergeError {
    BackendContributionMergeError {
        artifact: Some(artifact.clone()),
        kind: Arc::new(kind),
    }
}

#[cfg(test)]
mod tests {
    use bray_codegen::test_support::{codegen_request_for_backend, contribution};
    use bray_codegen::{
        ArtifactContent, ArtifactDigest, ArtifactDigestAlgorithm, BackendArtifactContribution,
        BackendCapabilityRevision, CodegenOutcome, CodegenRuntimeMetadata,
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

        let Ok(plan) = EmissionPlan::try_new(
            request,
            Some(backend.clone()),
            Some(backend_capability_revision()),
            [staged, published],
            [backend_request],
            None,
        ) else {
            panic!("test backend plan must be valid");
        };

        let fixture = codegen_request_for_backend(backend);
        let artifact = contribution(&fixture, fixture.required_artifact().clone());
        let outcome = complete_outcome(&fixture, [artifact]);

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
    fn merge_rejects_missing_unrequested_and_digest_mismatched_contributions() {
        let (request, backend, artifact, backend_request) = backend_artifact_plan_parts();

        let Ok(plan) = EmissionPlan::try_new(
            request,
            Some(backend.clone()),
            Some(backend_capability_revision()),
            [artifact],
            [backend_request],
            None,
        ) else {
            panic!("test backend plan must be valid");
        };

        let fixture = codegen_request_for_backend(backend.clone());

        let missing = BackendContributionSet::try_from_backend(&plan, [], &never_cancelled);

        assert!(matches!(
            missing,
            Err(error)
                if matches!(
                    error.kind(),
                    BackendContributionMergeErrorKind::MissingUnit(_)
                )
        ));

        let required = contribution(&fixture, fixture.required_artifact().clone());
        let optional = contribution(&fixture, fixture.optional_artifact().clone());
        let unrequested = complete_outcome(&fixture, [required, optional]);

        let Some(unrequested) = unrequested.artifacts() else {
            panic!("test code generation must complete");
        };

        let merged =
            BackendContributionSet::try_from_backend(&plan, [unrequested], &never_cancelled);

        assert!(matches!(
            merged,
            Err(error)
                if matches!(
                    error.kind(),
                    BackendContributionMergeErrorKind::UnrequestedArtifact(_)
                )
        ));

        let content = ArtifactContent::try_memory([1_u8, 2, 3].as_slice())
            .unwrap_or_else(|error| panic!("test content must be valid: {error:?}"));

        let digest = ArtifactDigest::try_new(ArtifactDigestAlgorithm::Blake3, [0_u8; 32])
            .unwrap_or_else(|| panic!("test digest must be valid"));

        let mismatched = BackendArtifactContribution::new(
            fixture.required_artifact().clone(),
            content,
            backend,
            fixture.request().capability_revision(),
            fixture.request().target().identity().clone(),
            Some(digest),
        );

        let mismatched = complete_outcome(&fixture, [mismatched]);

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

    #[test]
    fn merge_rejects_artifacts_from_another_capability_contract() {
        let (request, backend, artifact, backend_request) = backend_artifact_plan_parts();

        let revision = BackendCapabilityRevision::try_new(2)
            .unwrap_or_else(|| panic!("test capability revision must be valid"));

        let Ok(plan) = EmissionPlan::try_new(
            request,
            Some(backend.clone()),
            Some(revision),
            [artifact],
            [backend_request],
            None,
        ) else {
            panic!("test backend plan must be valid");
        };

        let fixture = codegen_request_for_backend(backend);
        let contribution = contribution(&fixture, fixture.required_artifact().clone());
        let outcome = complete_outcome(&fixture, [contribution]);

        let Some(artifacts) = outcome.artifacts() else {
            panic!("test code generation must complete");
        };

        let merged = BackendContributionSet::try_from_backend(&plan, [artifacts], &never_cancelled);

        assert!(matches!(
            merged,
            Err(error)
                if matches!(
                    error.kind(),
                    BackendContributionMergeErrorKind::CapabilityRevisionMismatch(_)
                )
        ));
    }

    fn complete_outcome(
        fixture: &bray_codegen::test_support::CodegenRequestFixture,
        contributions: impl IntoIterator<Item = BackendArtifactContribution>,
    ) -> CodegenOutcome {
        CodegenOutcome::try_complete(
            fixture.request(),
            contributions,
            CodegenRuntimeMetadata::default(),
            DiagnosticBag::new(),
        )
        .unwrap_or_else(|error| panic!("test code generation must complete: {error:?}"))
    }

    fn never_cancelled() -> bool {
        false
    }
}
