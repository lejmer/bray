use std::collections::BTreeSet;
use std::fs::File;
use std::path::{Path, PathBuf};

use bray_base::{
    Cancellation, CompletedStagedFile, FileReplacementMode, StagedFile, sync_directory,
};

use super::transaction::ManagedLayout;
use crate::publication::diagnostic::PublicationErrorKind;
use crate::publication::operation::{
    ArtifactPublicationFailure, PreparedArtifact, artifact_failure,
};
use crate::publication::staging::replacement_mode;
use crate::{OutputSink, PlannedArtifactDestination, ProductGenerationIdentity, ReplacementPolicy};

pub(super) struct CommittedPublicProjection {
    destination: PathBuf,
    backup: Option<CompletedStagedFile>,
}

pub(super) struct PreparedPublicProjection<'plan> {
    planned: &'plan crate::PlannedArtifact,
    destination: PathBuf,
    staged: Option<CompletedStagedFile>,
    backup: Option<CompletedStagedFile>,
}

pub(super) fn prepare_public_projections<'plan>(
    layout: &ManagedLayout,
    identity: ProductGenerationIdentity,
    prepared: &'plan [PreparedArtifact<'_, '_>],
    stale_paths: impl IntoIterator<Item = PathBuf>,
    replacement: ReplacementPolicy,
    cancellation: &dyn Cancellation,
) -> Result<Vec<PreparedPublicProjection<'plan>>, ArtifactPublicationFailure> {
    let generation = layout.generations.join(identity.to_hex());
    let mut projections = Vec::with_capacity(prepared.len());

    for artifact in prepared {
        if cancellation.is_cancelled() {
            return Err(ArtifactPublicationFailure::Cancelled);
        }

        let PlannedArtifactDestination::Publish(OutputSink::ManagedFilesystem {
            artifact: relative,
            published,
            ..
        }) = artifact.planned.destination()
        else {
            return Err(artifact_failure(
                artifact.planned,
                PublicationErrorKind::InvalidContribution,
            ));
        };

        if replacement == ReplacementPolicy::RequireAbsent && published.exists() {
            return Err(artifact_failure(
                artifact.planned,
                PublicationErrorKind::Commit(std::io::ErrorKind::AlreadyExists),
            ));
        }

        let source = generation.join(relative.to_path_buf());

        let permissions = std::fs::metadata(&source)
            .map_err(|error| {
                artifact_failure(artifact.planned, PublicationErrorKind::Read(error.kind()))
            })?
            .permissions();

        let backup = prepare_public_backup(&layout.staging, published, artifact.planned)?;

        let staged = copy_public_projection(
            &source,
            &layout.staging,
            published,
            replacement_mode(replacement),
            Some(permissions),
            artifact.planned,
        )?;

        projections.push(PreparedPublicProjection {
            planned: artifact.planned,
            destination: published.clone(),
            staged: Some(staged),
            backup,
        });
    }

    for destination in stale_paths {
        let backup = prepare_public_backup(&layout.staging, &destination, prepared[0].planned)?;

        if backup.is_some() {
            projections.push(PreparedPublicProjection {
                planned: prepared[0].planned,
                destination,
                staged: None,
                backup,
            });
        }
    }

    Ok(projections)
}

pub(super) fn commit_public_projections(
    projections: Vec<PreparedPublicProjection<'_>>,
    cancellation: &dyn Cancellation,
) -> Result<Vec<CommittedPublicProjection>, ArtifactPublicationFailure> {
    let mut committed = Vec::with_capacity(projections.len());

    for projection in projections {
        if cancellation.is_cancelled() {
            rollback_public_projections(committed, projection.planned)?;

            return Err(ArtifactPublicationFailure::Cancelled);
        }

        let promotion = match projection.staged {
            Some(staged) => staged.promote(&projection.destination),
            None => std::fs::remove_file(&projection.destination),
        };

        if let Err(error) = promotion {
            rollback_public_projections(committed, projection.planned)?;

            return Err(artifact_failure(
                projection.planned,
                PublicationErrorKind::Commit(error.kind()),
            ));
        }

        committed.push(CommittedPublicProjection {
            destination: projection.destination,
            backup: projection.backup,
        });
    }

    Ok(committed)
}

pub(super) fn rollback_public_projections(
    projections: Vec<CommittedPublicProjection>,
    planned: &crate::PlannedArtifact,
) -> Result<(), ArtifactPublicationFailure> {
    let directories = public_projection_directories(&projections);

    for projection in projections.into_iter().rev() {
        match projection.backup {
            Some(backup) => backup.promote(&projection.destination).map_err(|error| {
                artifact_failure(planned, PublicationErrorKind::Commit(error.kind()))
            })?,
            None => match std::fs::remove_file(&projection.destination) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(artifact_failure(
                        planned,
                        PublicationErrorKind::Commit(error.kind()),
                    ));
                }
            },
        }
    }

    sync_public_directories(directories, planned)
}

pub(super) fn sync_public_projections(
    projections: &[CommittedPublicProjection],
    planned: &crate::PlannedArtifact,
) -> Result<(), ArtifactPublicationFailure> {
    sync_public_directories(public_projection_directories(projections), planned)
}

fn prepare_public_backup(
    staging_directory: &Path,
    destination: &Path,
    planned: &crate::PlannedArtifact,
) -> Result<Option<CompletedStagedFile>, ArtifactPublicationFailure> {
    match std::fs::symlink_metadata(destination) {
        Ok(metadata) if metadata.file_type().is_file() => copy_public_projection(
            destination,
            staging_directory,
            destination,
            FileReplacementMode::ReplaceExisting,
            Some(metadata.permissions()),
            planned,
        )
        .map(Some),
        Ok(_) => Err(artifact_failure(
            planned,
            PublicationErrorKind::InvalidContribution,
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(artifact_failure(
            planned,
            PublicationErrorKind::Read(error.kind()),
        )),
    }
}

fn copy_public_projection(
    source: &Path,
    staging_directory: &Path,
    destination: &Path,
    replacement: FileReplacementMode,
    permissions: Option<std::fs::Permissions>,
    planned: &crate::PlannedArtifact,
) -> Result<CompletedStagedFile, ArtifactPublicationFailure> {
    let mut source = File::open(source)
        .map_err(|error| artifact_failure(planned, PublicationErrorKind::Read(error.kind())))?;

    let mut staged =
        StagedFile::create_in(staging_directory, destination, replacement, permissions)
            .map_err(|error| artifact_failure(planned, PublicationErrorKind::Open(error.kind())))?;

    std::io::copy(&mut source, &mut staged)
        .map_err(|error| artifact_failure(planned, PublicationErrorKind::Write(error.kind())))?;

    staged
        .finish()
        .map_err(|error| artifact_failure(planned, PublicationErrorKind::Flush(error.kind())))
}

fn public_projection_directories(projections: &[CommittedPublicProjection]) -> BTreeSet<PathBuf> {
    projections
        .iter()
        .map(|projection| {
            projection
                .destination
                .parent()
                .filter(|directory| !directory.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."))
                .to_owned()
        })
        .collect()
}

fn sync_public_directories(
    directories: BTreeSet<PathBuf>,
    planned: &crate::PlannedArtifact,
) -> Result<(), ArtifactPublicationFailure> {
    for directory in directories {
        sync_directory(&directory).map_err(|error| {
            artifact_failure(planned, PublicationErrorKind::Flush(error.kind()))
        })?;
    }

    Ok(())
}
