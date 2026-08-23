use std::collections::BTreeSet;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use bray_base::{
    Cancellation, StagedFile, atomic_rename_exclusive, atomic_rename_exclusive_is_supported,
    lowercase_hex, sync_directory,
};
use bray_codegen::ArtifactDigest;
use tempfile::{Builder, TempDir};

use super::cleanup::{generation_public_paths, retain_recent_generations, stale_public_paths};
use super::layout::{STAGING_DIRECTORY, product_store_relative};
use super::locator::GenerationLocator;
use super::lock::ProductPublicationLock;
use super::manifest::{
    GenerationManifest, GenerationReference, ManifestArtifact, ManifestPermissions,
    ManifestProduct, permission_key,
};
use super::projection::{
    commit_public_projections, prepare_public_projections, rollback_public_projections,
    sync_public_projections,
};

use crate::artifact::content::validate_staged_content;
use crate::publication::diagnostic::{PublicationError, PublicationErrorKind};
use crate::publication::operation::{
    ArtifactPublicationFailure, PreparedArtifact, artifact_failure, content_failure, copy_content,
    planned_error,
};
use crate::publication::staging::{create_new_artifact_file, replacement_mode};
use crate::{
    EmissionPlan, EmittedArtifact, EmittedArtifactSet, OutputSink, PlannedArtifactDestination,
    ProductGenerationIdentity, PublishedProductGeneration, ReplacementPolicy,
};

pub(super) const GENERATION_MANIFEST: &str = "manifest.json";
pub(super) const PUBLISHED_REFERENCE: &str = "published-generation.json";
const PRIVATE_GENERATION_PREFIX: &str = ".bray-generation-";
const LOCATOR_ATTEMPTS: u32 = 256;
pub(super) const MANIFEST_REVISION: u32 = 1;

pub(in crate::publication) struct ManagedGenerationPublication {
    pub(in crate::publication) artifacts: EmittedArtifactSet,
    pub(in crate::publication) generation: PublishedProductGeneration,
    pub(in crate::publication) warning: Option<PublicationError>,
}

pub(in crate::publication) fn publish_managed_generation(
    plan: &EmissionPlan,
    prepared: Vec<PreparedArtifact<'_, '_>>,
    replacement: ReplacementPolicy,
    cancellation: &dyn Cancellation,
) -> Result<ManagedGenerationPublication, ArtifactPublicationFailure> {
    let Some(first) = prepared.first() else {
        return Err(ArtifactPublicationFailure::Cancelled);
    };

    let root = managed_root(&prepared).ok_or_else(|| {
        artifact_failure(first.planned, PublicationErrorKind::InvalidContribution)
    })?;

    if cancellation.is_cancelled() {
        return Err(ArtifactPublicationFailure::Cancelled);
    }

    let layout = create_layout(root, first.planned)?;
    let _publication_lock = ProductPublicationLock::acquire(&layout.metadata, first.planned)?;
    let preceding_generation = referenced_generation(&layout);

    let preceding_public_paths = preceding_generation
        .map(|locator| generation_public_paths(root, &layout, locator, first.planned))
        .transpose()?
        .unwrap_or_default();

    let private = create_private_generation(&layout, first.planned)?;

    let (manifest, emitted) = stage_generation(root, &private, &prepared, cancellation)?;

    let manifest_bytes = encode_manifest(&manifest, first.planned)?;
    let manifest_digest = *blake3::hash(&manifest_bytes).as_bytes();
    let identity = ProductGenerationIdentity::new(manifest_digest);

    write_manifest(&private, &manifest_bytes, first.planned)?;
    make_private_generation_durable(&private, first.planned)?;

    if cancellation.is_cancelled() {
        return Err(ArtifactPublicationFailure::Cancelled);
    }

    let locator = commit_generation(
        &private,
        &layout,
        identity,
        &manifest_bytes,
        &manifest,
        &emitted,
        first.planned,
        cancellation,
    )?;

    if cancellation.is_cancelled() {
        return Err(ArtifactPublicationFailure::Cancelled);
    }

    let projections = prepare_public_projections(
        &layout,
        locator,
        &prepared,
        stale_public_paths(root, &manifest, preceding_public_paths, first.planned)?,
        replacement,
        cancellation,
    )?;

    let committed = commit_public_projections(projections, cancellation)?;

    if let Err(error) = sync_public_projections(&committed, first.planned) {
        rollback_public_projections(committed, first.planned)?;

        return Err(error);
    }

    let reference_bytes = encode_reference(locator, identity, first.planned)?;

    let reference_warning = match commit_reference(
        &layout,
        replacement,
        &reference_bytes,
        first.planned,
        cancellation,
    ) {
        Ok(warning) => warning,
        Err(error) => {
            rollback_public_projections(committed, first.planned)?;

            return Err(error);
        }
    };

    let retention_warning =
        retain_recent_generations(&layout, locator, preceding_generation, first.planned).err();

    let warning = reference_warning.or(retention_warning);

    let artifacts = EmittedArtifactSet::from_publication(plan, emitted);

    let generation = PublishedProductGeneration::new(
        identity,
        root.to_owned(),
        layout.metadata.clone(),
        layout.metadata.join(locator.to_hex()),
        layout.reference,
        // The generation and outcome share immutable Arc-backed artifact records.
        artifacts.clone(),
    );

    Ok(ManagedGenerationPublication {
        artifacts,
        generation,
        warning,
    })
}

pub(super) struct ManagedLayout {
    pub(super) metadata: PathBuf,
    pub(super) reference: PathBuf,
    pub(super) staging: PathBuf,
}

fn managed_root<'prepared>(
    prepared: &'prepared [PreparedArtifact<'_, '_>],
) -> Option<&'prepared Path> {
    let mut selected = None;

    for artifact in prepared {
        let PlannedArtifactDestination::Publish(OutputSink::ManagedFilesystem { root, .. }) =
            artifact.planned.destination()
        else {
            return None;
        };

        match selected {
            Some(current) if current != root => return None,
            Some(_) => {}
            None => selected = Some(root.as_path()),
        }
    }

    selected
}

fn create_layout(
    root: &Path,
    planned: &crate::PlannedArtifact,
) -> Result<ManagedLayout, ArtifactPublicationFailure> {
    let PlannedArtifactDestination::Publish(OutputSink::ManagedFilesystem { published, .. }) =
        planned.destination()
    else {
        return Err(artifact_failure(
            planned,
            PublicationErrorKind::InvalidContribution,
        ));
    };

    let Some(public_directory) = published.parent() else {
        return Err(artifact_failure(
            planned,
            PublicationErrorKind::InvalidContribution,
        ));
    };

    let relative_public_directory = public_directory
        .strip_prefix(root)
        .map_err(|_| artifact_failure(planned, PublicationErrorKind::InvalidContribution))?;

    require_directory(root, planned)?;

    create_managed_path(root, relative_public_directory, planned)?;

    let metadata = create_managed_path(
        root,
        &product_store_relative(relative_public_directory, planned.id().product()),
        planned,
    )?;

    let staging = metadata.join(STAGING_DIRECTORY);

    create_managed_directory(&metadata, &staging, planned)?;

    let supported = atomic_rename_exclusive_is_supported(&metadata)
        .map_err(|error| artifact_failure(planned, PublicationErrorKind::Open(error.kind())))?;

    if !supported {
        return Err(artifact_failure(
            planned,
            PublicationErrorKind::ManagedPublicationUnsupported,
        ));
    }

    sync_directory(&metadata)
        .map_err(|error| artifact_failure(planned, PublicationErrorKind::Flush(error.kind())))?;

    Ok(ManagedLayout {
        reference: metadata.join(PUBLISHED_REFERENCE),
        metadata,
        staging,
    })
}

fn create_managed_path(
    root: &Path,
    relative: &Path,
    planned: &crate::PlannedArtifact,
) -> Result<PathBuf, ArtifactPublicationFailure> {
    let mut path = root.to_owned();

    for component in relative.components() {
        let std::path::Component::Normal(component) = component else {
            return Err(artifact_failure(
                planned,
                PublicationErrorKind::InvalidContribution,
            ));
        };

        let child = path.join(component);

        create_managed_directory(&path, &child, planned)?;

        path = child;
    }

    Ok(path)
}

fn create_managed_directory(
    parent: &Path,
    path: &Path,
    planned: &crate::PlannedArtifact,
) -> Result<(), ArtifactPublicationFailure> {
    match std::fs::create_dir(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            require_directory(path, planned)?;
        }
        Err(error) => {
            return Err(artifact_failure(
                planned,
                PublicationErrorKind::Open(error.kind()),
            ));
        }
    }

    sync_directory(parent)
        .map_err(|error| artifact_failure(planned, PublicationErrorKind::Flush(error.kind())))
}

fn require_directory(
    path: &Path,
    planned: &crate::PlannedArtifact,
) -> Result<(), ArtifactPublicationFailure> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| artifact_failure(planned, PublicationErrorKind::Open(error.kind())))?;

    if !metadata.file_type().is_dir() {
        return Err(artifact_failure(
            planned,
            PublicationErrorKind::InvalidContribution,
        ));
    }

    Ok(())
}

fn create_private_generation(
    layout: &ManagedLayout,
    planned: &crate::PlannedArtifact,
) -> Result<TempDir, ArtifactPublicationFailure> {
    Builder::new()
        .prefix(PRIVATE_GENERATION_PREFIX)
        .tempdir_in(&layout.metadata)
        .map_err(|error| artifact_failure(planned, PublicationErrorKind::Open(error.kind())))
}

fn stage_generation(
    root: &Path,
    private: &TempDir,
    prepared: &[PreparedArtifact<'_, '_>],
    cancellation: &dyn Cancellation,
) -> Result<(GenerationManifest, Vec<EmittedArtifact>), ArtifactPublicationFailure> {
    let mut manifest_artifacts = Vec::with_capacity(prepared.len());
    let mut emitted = Vec::with_capacity(prepared.len());
    let mut created_directories = BTreeSet::new();

    for artifact in prepared {
        if cancellation.is_cancelled() {
            return Err(ArtifactPublicationFailure::Cancelled);
        }

        let PlannedArtifactDestination::Publish(OutputSink::ManagedFilesystem {
            artifact: relative,
            ..
        }) = artifact.planned.destination()
        else {
            return Err(artifact_failure(
                artifact.planned,
                PublicationErrorKind::InvalidContribution,
            ));
        };

        let destination = private.path().join(relative.to_path_buf());

        let Some(parent) = destination.parent() else {
            return Err(artifact_failure(
                artifact.planned,
                PublicationErrorKind::InvalidContribution,
            ));
        };

        std::fs::create_dir_all(parent).map_err(|error| {
            artifact_failure(artifact.planned, PublicationErrorKind::Open(error.kind()))
        })?;

        for directory in parent.ancestors() {
            if directory == private.path() {
                break;
            }

            created_directories.insert(directory.to_owned());
        }

        let (digest, permissions) = write_artifact(&destination, artifact, cancellation)?;

        let PlannedArtifactDestination::Publish(sink) = artifact.planned.destination() else {
            return Err(artifact_failure(
                artifact.planned,
                PublicationErrorKind::InvalidContribution,
            ));
        };

        let OutputSink::ManagedFilesystem { published, .. } = sink else {
            return Err(artifact_failure(
                artifact.planned,
                PublicationErrorKind::InvalidContribution,
            ));
        };

        let published = published.strip_prefix(root).map_err(|_| {
            artifact_failure(artifact.planned, PublicationErrorKind::InvalidContribution)
        })?;

        let published = portable_path(published).ok_or_else(|| {
            artifact_failure(artifact.planned, PublicationErrorKind::InvalidContribution)
        })?;

        manifest_artifacts.push(ManifestArtifact::new(
            artifact,
            relative.as_str(),
            &published,
            &digest,
            permissions,
        ));

        // Published records must own immutable plan records after publication returns.
        emitted.push(EmittedArtifact::new(
            artifact.planned.id().clone(),
            sink.clone(),
            artifact.planned.producer().clone(),
            artifact.planned.role(),
            artifact.content.byte_len(),
            digest,
        ));
    }

    sync_created_directories(created_directories, prepared[0].planned)?;

    let product = prepared[0].planned.id().product();

    let manifest = GenerationManifest {
        revision: MANIFEST_REVISION,
        product: ManifestProduct {
            package: product.package().as_str().to_owned(),
            name: product.name().to_owned(),
        },
        artifacts: manifest_artifacts,
    };

    Ok((manifest, emitted))
}

fn referenced_generation(layout: &ManagedLayout) -> Option<GenerationLocator> {
    let bytes = std::fs::read(&layout.reference).ok()?;
    let reference = serde_json::from_slice::<GenerationReference>(&bytes).ok()?;

    GenerationLocator::try_from_hex(&reference.locator)
}

fn portable_path(path: &Path) -> Option<String> {
    let mut parts = Vec::new();

    for component in path.components() {
        let std::path::Component::Normal(component) = component else {
            return None;
        };

        parts.push(component.to_str()?);
    }

    (!parts.is_empty()).then(|| parts.join("/"))
}

fn write_artifact(
    destination: &Path,
    artifact: &PreparedArtifact<'_, '_>,
    cancellation: &dyn Cancellation,
) -> Result<(ArtifactDigest, ManifestPermissions), ArtifactPublicationFailure> {
    let mut file =
        create_new_artifact_file(destination, artifact.planned.id().kind()).map_err(|error| {
            artifact_failure(artifact.planned, PublicationErrorKind::Open(error.kind()))
        })?;

    copy_content(cancellation, artifact.planned, &artifact.content, &mut file)?;

    file.flush().map_err(|error| {
        artifact_failure(artifact.planned, PublicationErrorKind::Flush(error.kind()))
    })?;

    file.sync_all().map_err(|error| {
        artifact_failure(artifact.planned, PublicationErrorKind::Flush(error.kind()))
    })?;

    let digest = validate_staged_content(
        destination,
        artifact.content.byte_len(),
        artifact.content.digest(),
        cancellation,
    )
    .map_err(|error| content_failure(artifact.planned, error))?;

    let permissions = ManifestPermissions::read(destination, artifact.planned.id().kind())
        .map_err(|error| {
            artifact_failure(artifact.planned, PublicationErrorKind::Read(error.kind()))
        })?;

    Ok((digest, permissions))
}

fn sync_created_directories(
    directories: BTreeSet<PathBuf>,
    planned: &crate::PlannedArtifact,
) -> Result<(), ArtifactPublicationFailure> {
    let mut directories: Vec<_> = directories.into_iter().collect();

    directories.sort_unstable_by_key(|path| std::cmp::Reverse(path.components().count()));

    for directory in directories {
        sync_directory(&directory).map_err(|error| {
            artifact_failure(planned, PublicationErrorKind::Flush(error.kind()))
        })?;
    }

    Ok(())
}

fn encode_manifest(
    manifest: &GenerationManifest,
    planned: &crate::PlannedArtifact,
) -> Result<Vec<u8>, ArtifactPublicationFailure> {
    serde_json::to_vec(manifest)
        .map_err(|_| artifact_failure(planned, PublicationErrorKind::InvalidGenerationManifest))
}

fn write_manifest(
    private: &TempDir,
    bytes: &[u8],
    planned: &crate::PlannedArtifact,
) -> Result<(), ArtifactPublicationFailure> {
    let path = private.path().join(GENERATION_MANIFEST);
    let mut file = create_new_file(&path, planned)?;

    file.write_all(bytes)
        .map_err(|error| artifact_failure(planned, PublicationErrorKind::Write(error.kind())))?;

    file.flush()
        .map_err(|error| artifact_failure(planned, PublicationErrorKind::Flush(error.kind())))?;

    file.sync_all()
        .map_err(|error| artifact_failure(planned, PublicationErrorKind::Flush(error.kind())))
}

fn create_new_file(
    path: &Path,
    planned: &crate::PlannedArtifact,
) -> Result<File, ArtifactPublicationFailure> {
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| artifact_failure(planned, PublicationErrorKind::Open(error.kind())))
}

fn make_private_generation_durable(
    private: &TempDir,
    planned: &crate::PlannedArtifact,
) -> Result<(), ArtifactPublicationFailure> {
    sync_directory(private.path())
        .map_err(|error| artifact_failure(planned, PublicationErrorKind::Flush(error.kind())))
}

fn commit_generation(
    private: &TempDir,
    layout: &ManagedLayout,
    identity: ProductGenerationIdentity,
    manifest_bytes: &[u8],
    manifest: &GenerationManifest,
    artifacts: &[EmittedArtifact],
    planned: &crate::PlannedArtifact,
    cancellation: &dyn Cancellation,
) -> Result<GenerationLocator, ArtifactPublicationFailure> {
    for attempt in 0..LOCATOR_ATTEMPTS {
        let locator = GenerationLocator::for_identity(identity, attempt);
        let destination = layout.metadata.join(locator.to_hex());

        return match atomic_rename_exclusive(private.path(), &destination) {
            Ok(()) => {
                sync_directory(&layout.metadata).map_err(|error| {
                    artifact_failure(planned, PublicationErrorKind::Commit(error.kind()))
                })?;

                Ok(locator)
            }
            Err(_) if destination.exists() => {
                let existing_manifest = std::fs::read(destination.join(GENERATION_MANIFEST))
                    .map_err(|_| {
                        artifact_failure(planned, PublicationErrorKind::GenerationCollision)
                    })?;

                if existing_manifest != manifest_bytes {
                    continue;
                }

                validate_existing_generation(
                    &destination,
                    manifest,
                    artifacts,
                    planned,
                    cancellation,
                )?;

                Ok(locator)
            }
            Err(error) => Err(artifact_failure(
                planned,
                PublicationErrorKind::Commit(error.kind()),
            )),
        };
    }

    Err(artifact_failure(
        planned,
        PublicationErrorKind::GenerationCollision,
    ))
}

fn validate_existing_generation(
    destination: &Path,
    manifest: &GenerationManifest,
    artifacts: &[EmittedArtifact],
    planned: &crate::PlannedArtifact,
    cancellation: &dyn Cancellation,
) -> Result<(), ArtifactPublicationFailure> {
    if manifest.artifacts.len() != artifacts.len() {
        return Err(artifact_failure(
            planned,
            PublicationErrorKind::GenerationCollision,
        ));
    }

    for (artifact, expected) in artifacts.iter().zip(&manifest.artifacts) {
        if cancellation.is_cancelled() {
            return Err(ArtifactPublicationFailure::Cancelled);
        }

        let OutputSink::ManagedFilesystem {
            artifact: relative, ..
        } = artifact.sink()
        else {
            return Err(artifact_failure(
                planned,
                PublicationErrorKind::GenerationCollision,
            ));
        };

        let path = destination.join(relative.to_path_buf());

        if expected.path != relative.as_str()
            || expected.permissions.logical != permission_key(artifact.id().kind())
            || !expected.permissions.matches(&path).unwrap_or(false)
        {
            return Err(artifact_failure(
                planned,
                PublicationErrorKind::GenerationCollision,
            ));
        }

        validate_staged_content(
            &path,
            artifact.byte_len(),
            Some(artifact.digest()),
            cancellation,
        )
        .map_err(|_| artifact_failure(planned, PublicationErrorKind::GenerationCollision))?;
    }

    Ok(())
}

fn encode_reference(
    locator: GenerationLocator,
    identity: ProductGenerationIdentity,
    planned: &crate::PlannedArtifact,
) -> Result<Vec<u8>, ArtifactPublicationFailure> {
    serde_json::to_vec(&GenerationReference {
        revision: MANIFEST_REVISION,
        locator: locator.to_hex(),
        manifest_digest: lowercase_hex(&identity.as_bytes()),
    })
    .map_err(|_| artifact_failure(planned, PublicationErrorKind::InvalidGenerationManifest))
}

fn commit_reference(
    layout: &ManagedLayout,
    replacement: ReplacementPolicy,
    bytes: &[u8],
    planned: &crate::PlannedArtifact,
    cancellation: &dyn Cancellation,
) -> Result<Option<PublicationError>, ArtifactPublicationFailure> {
    commit_reference_with_sync(
        layout,
        replacement,
        bytes,
        planned,
        cancellation,
        sync_directory,
    )
}

fn commit_reference_with_sync<Sync>(
    layout: &ManagedLayout,
    replacement: ReplacementPolicy,
    bytes: &[u8],
    planned: &crate::PlannedArtifact,
    cancellation: &dyn Cancellation,
    sync: Sync,
) -> Result<Option<PublicationError>, ArtifactPublicationFailure>
where
    Sync: FnOnce(&Path) -> std::io::Result<()>,
{
    let mut staging = StagedFile::create(&layout.reference, replacement_mode(replacement), None)
        .map_err(|error| artifact_failure(planned, PublicationErrorKind::Open(error.kind())))?;

    staging
        .write_all(bytes)
        .map_err(|error| artifact_failure(planned, PublicationErrorKind::Write(error.kind())))?;

    let staging = staging
        .finish()
        .map_err(|error| artifact_failure(planned, PublicationErrorKind::Flush(error.kind())))?;

    if cancellation.is_cancelled() {
        return Err(ArtifactPublicationFailure::Cancelled);
    }

    staging
        .promote(&layout.reference)
        .map_err(|error| artifact_failure(planned, PublicationErrorKind::Commit(error.kind())))?;

    Ok(sync(&layout.metadata)
        .err()
        .map(|error| planned_error(planned, PublicationErrorKind::Commit(error.kind()))))
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::{ManagedLayout, commit_reference_with_sync};
    use crate::ReplacementPolicy;
    use crate::test_support::emission_plan;

    #[test]
    fn reference_visibility_remains_committed_when_post_commit_sync_fails() {
        let Ok(root) = tempfile::tempdir() else {
            panic!("test managed root must be created");
        };

        let metadata = root.path().join(".bray");

        std::fs::create_dir(&metadata)
            .unwrap_or_else(|error| panic!("test metadata directory must be created: {error}"));

        let reference = metadata.join("published-generation.json");

        let layout = ManagedLayout {
            reference: reference.clone(),
            staging: metadata.join("staging"),
            metadata,
        };

        let plan = emission_plan();
        let planned = &plan.artifacts()[0];

        let Ok(warning) = commit_reference_with_sync(
            &layout,
            ReplacementPolicy::ReplaceExisting,
            b"published",
            planned,
            &|| false,
            fail_sync,
        ) else {
            panic!("visible reference must remain committed");
        };

        assert!(warning.is_some());

        assert_eq!(
            std::fs::read(reference)
                .unwrap_or_else(|error| panic!("published reference must be readable: {error}")),
            b"published"
        );
    }

    fn fail_sync(_: &std::path::Path) -> io::Result<()> {
        Err(io::Error::other("injected post-commit sync failure"))
    }
}
