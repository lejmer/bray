use std::cmp::Ordering;
use std::io::{self, Write};

use bray_base::Cancellation;
use bray_codegen::{ArtifactContent, ArtifactDigest, ArtifactDigestAlgorithm};

use super::super::diagnostic::{PublicationDiagnostics, PublicationError, PublicationErrorKind};
use super::super::link::PreparedLinkedArtifact;
use super::failure::{
    ArtifactPublicationFailure, artifact_failure, contribution_error, planned_error,
};

use crate::artifact::content::{
    ContentReader, ContentValidationError, open_content, open_linked_staging, validate_content,
};
use crate::{
    ArtifactContribution, ArtifactId, ArtifactKind, ArtifactProducer, ArtifactRequirement,
    EmissionPlan, OutputSink, PlannedArtifact, PlannedArtifactDestination,
};

pub(in crate::publication) fn copy_content(
    cancellation: &dyn Cancellation,
    planned: &PlannedArtifact,
    content: &PreparedContent<'_, '_>,
    writer: &mut dyn Write,
) -> Result<(), ArtifactPublicationFailure> {
    if cancellation.is_cancelled() {
        return Err(ArtifactPublicationFailure::Cancelled);
    }

    let mut reader = content
        .open()
        .map_err(|kind| artifact_failure(planned, PublicationErrorKind::Read(kind)))?;

    crate::artifact::content::copy_reader(&mut reader, writer, cancellation).map_err(|error| {
        use crate::artifact::content::ContentCopyError;

        match error {
            ContentCopyError::Cancelled => ArtifactPublicationFailure::Cancelled,
            ContentCopyError::Read(kind) => {
                artifact_failure(planned, PublicationErrorKind::Read(kind))
            }
            ContentCopyError::Write(kind) => {
                artifact_failure(planned, PublicationErrorKind::Write(kind))
            }
        }
    })
}

pub(in crate::publication) struct PreparedArtifact<'plan, 'link> {
    pub(in crate::publication) planned: &'plan PlannedArtifact,
    pub(in crate::publication) content: PreparedContent<'plan, 'link>,
}

impl PreparedArtifact<'_, '_> {
    pub(super) fn is_managed(&self) -> bool {
        matches!(
            self.planned.destination(),
            PlannedArtifactDestination::Publish(OutputSink::ManagedFilesystem { .. })
        )
    }
}

pub(in crate::publication) enum PreparedContent<'plan, 'link> {
    Contribution(ArtifactContribution),
    Linked(PreparedLinkedArtifact<'plan, 'link>),
}

impl PreparedContent<'_, '_> {
    pub(in crate::publication) fn id(&self) -> &ArtifactId {
        match self {
            Self::Contribution(contribution) => contribution.id(),
            Self::Linked(linked) => linked.planned().id(),
        }
    }

    pub(in crate::publication) fn producer(&self) -> &ArtifactProducer {
        match self {
            Self::Contribution(contribution) => contribution.producer(),
            Self::Linked(linked) => linked.planned().producer(),
        }
    }

    pub(in crate::publication) fn byte_len(&self) -> u64 {
        match self {
            Self::Contribution(contribution) => contribution.content().byte_len(),
            Self::Linked(linked) => linked.byte_len(),
        }
    }

    pub(in crate::publication) fn digest(&self) -> Option<&ArtifactDigest> {
        match self {
            Self::Contribution(contribution) => contribution.digest(),
            Self::Linked(linked) => Some(linked.digest()),
        }
    }

    pub(in crate::publication) fn open(&self) -> Result<ContentReader<'_>, io::ErrorKind> {
        match self {
            Self::Contribution(contribution) => open_content(contribution.content()),
            Self::Linked(linked) => open_linked_staging(linked.path()),
        }
    }

    pub(super) fn validate(
        &self,
        cancellation: &dyn Cancellation,
    ) -> Result<ArtifactDigest, ContentValidationError> {
        match self {
            Self::Contribution(contribution) => {
                validate_content(contribution.content(), contribution.digest(), cancellation)
            }
            Self::Linked(linked) => {
                // Publication records retain the fixed-size digest after staging validation.
                Ok(linked.digest().clone())
            }
        }
    }
}

pub(super) fn prepare_contributions<'plan, 'link>(
    plan: &'plan EmissionPlan,
    contributions: impl IntoIterator<Item = ArtifactContribution>,
    linked: impl IntoIterator<Item = PreparedLinkedArtifact<'plan, 'link>>,
    diagnostics: &mut PublicationDiagnostics,
) -> Result<Vec<PreparedArtifact<'plan, 'link>>, PublicationError> {
    let mut contributions: Vec<_> = contributions
        .into_iter()
        .map(PreparedContent::Contribution)
        .collect();

    if let Some(package_interface) = package_interface_contribution(plan)? {
        contributions.push(PreparedContent::Contribution(package_interface));
    }

    contributions.extend(linked.into_iter().map(PreparedContent::Linked));

    contributions.sort_unstable_by(|left, right| left.id().cmp(right.id()));

    if let Some(pair) = contributions
        .windows(2)
        .find(|pair| pair[0].id() == pair[1].id())
    {
        return Err(contribution_error(
            pair[0].id(),
            plan,
            PublicationErrorKind::InvalidContribution,
        ));
    }

    for contribution in &contributions {
        validate_contribution_destination(plan, contribution.id())?;
    }

    let mut contributions = contributions.into_iter().peekable();
    let mut prepared = Vec::new();

    for planned in plan.published_artifacts() {
        let contribution = match contributions.peek() {
            Some(contribution) => match contribution.id().cmp(planned.id()) {
                Ordering::Equal => contributions.next(),
                Ordering::Greater => None,
                Ordering::Less => {
                    return Err(contribution_error(
                        contribution.id(),
                        plan,
                        PublicationErrorKind::InvalidContribution,
                    ));
                }
            },
            None => None,
        };

        let Some(contribution) = contribution else {
            if planned.requirement() == ArtifactRequirement::Required {
                return Err(planned_error(
                    planned,
                    PublicationErrorKind::MissingContribution,
                ));
            }

            continue;
        };

        if contribution.producer() != planned.producer() {
            let error = contribution_error(
                contribution.id(),
                plan,
                PublicationErrorKind::InvalidContribution,
            );

            if planned.requirement() == ArtifactRequirement::Optional {
                diagnostics.warning(error);

                continue;
            }

            return Err(error);
        }

        prepared.push(PreparedArtifact {
            planned,
            content: contribution,
        });
    }

    if let Some(contribution) = contributions.next() {
        return Err(contribution_error(
            contribution.id(),
            plan,
            PublicationErrorKind::InvalidContribution,
        ));
    }

    Ok(prepared)
}

fn package_interface_contribution(
    plan: &EmissionPlan,
) -> Result<Option<ArtifactContribution>, PublicationError> {
    let Some(interface) = plan.package_interface() else {
        return Ok(None);
    };

    let Some(planned) = plan
        .published_artifacts()
        .find(|artifact| artifact.id().kind() == ArtifactKind::PackageInterface)
    else {
        let id = ArtifactId::new(
            plan.request().product().clone(),
            ArtifactKind::PackageInterface,
            0,
        );

        return Err(PublicationError::new(
            id,
            None,
            PublicationErrorKind::InvalidContribution,
        ));
    };

    interface
        .validate_integrity()
        .map_err(|_| planned_error(planned, PublicationErrorKind::InvalidContribution))?;

    let content = ArtifactContent::try_memory(interface.shared_bytes())
        .map_err(|_| planned_error(planned, PublicationErrorKind::InvalidContribution))?;

    if content.byte_len() != interface.byte_len() {
        return Err(planned_error(
            planned,
            PublicationErrorKind::LengthMismatch {
                expected: interface.byte_len(),
                actual: content.byte_len(),
            },
        ));
    }

    let digest = ArtifactDigest::try_new(
        ArtifactDigestAlgorithm::Blake3,
        blake3::hash(interface.bytes()).as_bytes(),
    )
    .ok_or_else(|| planned_error(planned, PublicationErrorKind::InvalidContribution))?;

    Ok(Some(ArtifactContribution::new(
        planned.id().clone(),
        ArtifactProducer::PackageInterface,
        content,
        Some(digest),
    )))
}

fn validate_contribution_destination(
    plan: &EmissionPlan,
    artifact: &ArtifactId,
) -> Result<(), PublicationError> {
    let Some(planned) = plan.artifact(artifact) else {
        return Err(contribution_error(
            artifact,
            plan,
            PublicationErrorKind::InvalidContribution,
        ));
    };

    if !matches!(
        planned.destination(),
        PlannedArtifactDestination::Publish(_)
    ) {
        return Err(contribution_error(
            artifact,
            plan,
            PublicationErrorKind::InvalidContribution,
        ));
    }

    Ok(())
}
