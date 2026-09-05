use std::collections::BTreeSet;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use bray_base::{
    Cancellation, StagedFile, atomic_rename_exclusive, atomic_rename_exclusive_is_supported,
    sync_directory,
};
use bray_codegen::ArtifactDigest;
use tempfile::{Builder, TempDir};

use super::cleanup::{generation_public_paths, retain_recent_generations, stale_public_paths};
use super::layout::{PRIVATE_GENERATION_PREFIX, STAGING_DIRECTORY, product_store_relative};
use super::locator::GenerationLocator;
use super::lock::ProductPublicationLock;
use super::manifest::{GenerationManifest, ManifestArtifact, ManifestPermissions};
use super::reference::{GenerationReference, GenerationReferenceEntry};
use crate::storage::{
    ManagedStore, StorageContext, StorageLease, StorageProduct, create_managed_path,
    require_directory,
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

    let (layout, entry_lease) = create_layout(root, plan, first.planned, cancellation)?;

    let _publication_lock = ProductPublicationLock::acquire(&layout.metadata, first.planned)?;

    super::recovery::recover_publication(root, &layout.metadata, cancellation)
        .map_err(|error| storage_failure(first.planned, error))?;

    let prior_reference = GenerationReference::read(&layout.reference)
        .map_err(|error| reference_failure(first.planned, &layout.reference, error))?;

    let preceding_public_paths = prior_reference
        .as_ref()
        .map(|reference| generation_public_paths(root, &layout, &reference.current, first.planned))
        .transpose()?
        .unwrap_or_default();

    let private = create_private_generation(&layout, first.planned)?;

    let (manifest, emitted) = stage_generation(root, plan, &private, &prepared, cancellation)?;

    super::sharing::share_generation(root, private.path(), &manifest, cancellation)
        .map_err(|error| storage_failure(first.planned, error))?;

    let manifest_bytes = encode_manifest(
        &manifest,
        &private.path().join(GENERATION_MANIFEST),
        first.planned,
    )?;

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

    super::sharing::record_generation(root, &layout.metadata.join(locator.to_hex()), &manifest)
        .map_err(|error| storage_failure(first.planned, error))?;

    super::recovery::record_publication(&layout.metadata, &manifest)
        .map_err(|error| storage_failure(first.planned, error))?;

    let committed = commit_public_projections(projections, cancellation)?;

    if let Err(error) = sync_public_projections(&committed, first.planned) {
        rollback_public_projections(committed, first.planned)?;

        return Err(error);
    }

    let reference = GenerationReference::publish(
        GenerationReferenceEntry::new(locator, identity),
        prior_reference,
    );

    let reference_bytes = serde_json::to_vec(&reference).map_err(|_| {
        artifact_failure(
            first.planned,
            PublicationErrorKind::InvalidGenerationManifest,
        )
    })?;

    let lease = StorageLease::acquire(&layout.metadata.join(locator.to_hex()).join("lease.lock"))
        .map_err(|error| storage_failure(first.planned, error))?;

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

    let journal_warning = super::recovery::complete_publication(&layout.metadata)
        .err()
        .map(|error| {
            planned_error(
                first.planned,
                PublicationErrorKind::Storage(Box::new(error)),
            )
        });

    drop(committed);

    let retention_warning = if cancellation.is_cancelled() || journal_warning.is_some() {
        None
    } else {
        retain_recent_generations(root, &layout, &reference, first.planned, cancellation).err()
    };

    let warning = reference_warning.or(journal_warning).or(retention_warning);

    let artifacts = EmittedArtifactSet::from_publication(plan, emitted);

    let generation = PublishedProductGeneration::new(
        identity,
        root.to_owned(),
        layout.metadata.clone(),
        layout.metadata.join(locator.to_hex()),
        layout.reference,
        // The generation and outcome share immutable Arc-backed artifact records.
        artifacts.clone(),
        lease,
        entry_lease,
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
    plan: &EmissionPlan,
    planned: &crate::PlannedArtifact,
    cancellation: &dyn Cancellation,
) -> Result<(ManagedLayout, StorageLease), ArtifactPublicationFailure> {
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

    let mut store = ManagedStore::open(root).map_err(|error| storage_failure(planned, error))?;

    store
        .maintain(crate::StoragePolicy::default(), cancellation)
        .map_err(|error| storage_failure(planned, error))?;

    let entry_lease = store
        .register_product(
            &product_store_relative(relative_public_directory, planned.id().product()),
            plan,
            cancellation,
        )
        .map_err(|error| storage_failure(planned, error))?;

    require_directory(root).map_err(|error| storage_failure(planned, error))?;

    create_managed_path(root, relative_public_directory)
        .map_err(|error| storage_failure(planned, error))?;

    let metadata = create_managed_path(
        root,
        &product_store_relative(relative_public_directory, planned.id().product()),
    )
    .map_err(|error| storage_failure(planned, error))?;

    let staging = create_managed_path(&metadata, Path::new(STAGING_DIRECTORY))
        .map_err(|error| storage_failure(planned, error))?;

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

    Ok((
        ManagedLayout {
            reference: metadata.join(PUBLISHED_REFERENCE),
            metadata,
            staging,
        },
        entry_lease,
    ))
}

pub(super) fn storage_failure(
    planned: &crate::PlannedArtifact,
    error: crate::StorageError,
) -> ArtifactPublicationFailure {
    if matches!(error.kind(), crate::StorageErrorKind::Cancelled) {
        return ArtifactPublicationFailure::Cancelled;
    }

    artifact_failure(planned, PublicationErrorKind::Storage(Box::new(error)))
}

fn reference_failure(
    planned: &crate::PlannedArtifact,
    path: &Path,
    error: super::reader::PublishedGenerationReadError,
) -> ArtifactPublicationFailure {
    storage_failure(planned, error.into_storage_error(path))
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
    plan: &EmissionPlan,
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
        product: StorageProduct::new(product),
        context: StorageContext::for_plan(plan),
        artifacts: manifest_artifacts,
    };

    Ok((manifest, emitted))
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
    path: &Path,
    planned: &crate::PlannedArtifact,
) -> Result<Vec<u8>, ArtifactPublicationFailure> {
    serde_json::to_vec(manifest)
        .map_err(|error| storage_failure(planned, crate::StorageError::json(path, error)))
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
                require_directory(&destination).map_err(|error| storage_failure(planned, error))?;

                let existing_manifest =
                    crate::storage::read_owned_file(&destination.join(GENERATION_MANIFEST))
                        .map_err(|error| storage_failure(planned, error))?;

                if existing_manifest != manifest_bytes {
                    continue;
                }

                validate_existing_generation(&destination, manifest, planned, cancellation)?;

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
    planned: &crate::PlannedArtifact,
    cancellation: &dyn Cancellation,
) -> Result<(), ArtifactPublicationFailure> {
    for artifact in &manifest.artifacts {
        let path = super::validation::artifact_path(destination, &artifact.path)
            .map_err(|error| storage_failure(planned, error))?;

        super::validation::validate_artifact_file(&path, artifact, cancellation)
            .map_err(|error| storage_failure(planned, error))?;
    }

    Ok(())
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
