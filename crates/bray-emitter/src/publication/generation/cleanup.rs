use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use bray_base::sync_directory;

use super::manifest::GenerationManifest;
use super::transaction::{GENERATION_MANIFEST, ManagedLayout};
use crate::ProductGenerationIdentity;
use crate::publication::diagnostic::{PublicationError, PublicationErrorKind};
use crate::publication::operation::{ArtifactPublicationFailure, artifact_failure, planned_error};

pub(super) fn generation_public_paths(
    root: &Path,
    layout: &ManagedLayout,
    identity: ProductGenerationIdentity,
    planned: &crate::PlannedArtifact,
) -> Result<BTreeSet<PathBuf>, ArtifactPublicationFailure> {
    let bytes = std::fs::read(
        layout
            .generations
            .join(identity.to_hex())
            .join(GENERATION_MANIFEST),
    )
    .map_err(|error| artifact_failure(planned, PublicationErrorKind::Read(error.kind())))?;

    let manifest = serde_json::from_slice::<GenerationManifest>(&bytes)
        .map_err(|_| artifact_failure(planned, PublicationErrorKind::InvalidGenerationManifest))?;

    manifest
        .artifacts
        .into_iter()
        .map(|artifact| {
            crate::ManagedArtifactPath::try_new(artifact.published_path)
                .map(|path| root.join(path.to_path_buf()))
                .ok_or_else(|| {
                    artifact_failure(planned, PublicationErrorKind::InvalidGenerationManifest)
                })
        })
        .collect()
}

pub(super) fn stale_public_paths(
    root: &Path,
    manifest: &GenerationManifest,
    mut preceding: BTreeSet<PathBuf>,
    planned: &crate::PlannedArtifact,
) -> Result<BTreeSet<PathBuf>, ArtifactPublicationFailure> {
    for artifact in &manifest.artifacts {
        let path = crate::ManagedArtifactPath::try_new(artifact.published_path.as_str())
            .ok_or_else(|| {
                artifact_failure(planned, PublicationErrorKind::InvalidGenerationManifest)
            })?;

        preceding.remove(&root.join(path.to_path_buf()));
    }

    Ok(preceding)
}

pub(super) fn retain_recent_generations(
    layout: &ManagedLayout,
    current: ProductGenerationIdentity,
    preceding: Option<ProductGenerationIdentity>,
    planned: &crate::PlannedArtifact,
) -> Result<(), PublicationError> {
    let entries = std::fs::read_dir(&layout.generations)
        .map_err(|error| planned_error(planned, PublicationErrorKind::Commit(error.kind())))?;

    let mut obsolete = Vec::new();

    for entry in entries {
        let entry = entry
            .map_err(|error| planned_error(planned, PublicationErrorKind::Commit(error.kind())))?;

        let file_type = entry
            .file_type()
            .map_err(|error| planned_error(planned, PublicationErrorKind::Commit(error.kind())))?;

        if !file_type.is_dir() {
            continue;
        }

        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };

        let Some(identity) =
            bray_base::decode_lowercase_hex::<32>(&name).map(ProductGenerationIdentity::new)
        else {
            continue;
        };

        if identity != current && Some(identity) != preceding {
            obsolete.push(entry.path());
        }
    }

    obsolete.sort();

    for path in obsolete {
        std::fs::remove_dir_all(path)
            .map_err(|error| planned_error(planned, PublicationErrorKind::Commit(error.kind())))?;
    }

    sync_directory(&layout.generations)
        .map_err(|error| planned_error(planned, PublicationErrorKind::Commit(error.kind())))
}
