use std::collections::BTreeSet;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use bray_base::{
    Cancellation, StagedFile, atomic_rename_exclusive, atomic_rename_exclusive_is_supported,
    lowercase_hex, sync_directory,
};
use bray_codegen::{ArtifactDigest, ArtifactDigestAlgorithm, BackendArtifactKind};
use serde::{Deserialize, Serialize};
use tempfile::{Builder, TempDir};

use crate::publication::diagnostic::{PublicationError, PublicationErrorKind};
use crate::publication::operation::{
    ArtifactPublicationFailure, PreparedArtifact, artifact_failure, content_failure, copy_content,
    planned_error,
};
use crate::publication::staging::{create_new_artifact_file, replacement_mode};
use crate::artifact::content::validate_staged_content;
use crate::{
    ArtifactKind, ArtifactProducer, ArtifactRequirement, ArtifactRole, EmissionPlan,
    EmittedArtifact, EmittedArtifactSet, OutputSink, PlannedArtifactDestination,
    ProductGenerationIdentity, PublishedProductGeneration, ReplacementPolicy,
};

pub(super) const METADATA_DIRECTORY: &str = ".bray";
pub(super) const GENERATIONS_DIRECTORY: &str = "generations";
pub(super) const GENERATION_MANIFEST: &str = "manifest.json";
pub(super) const PUBLISHED_REFERENCE: &str = "published-generation.json";
const PRIVATE_GENERATION_PREFIX: &str = ".bray-generation-";
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
    let private = create_private_generation(&layout, first.planned)?;

    let (manifest, emitted) = stage_generation(&private, &prepared, cancellation)?;

    let manifest_bytes = encode_manifest(&manifest, first.planned)?;
    let manifest_digest = *blake3::hash(&manifest_bytes).as_bytes();
    let identity = ProductGenerationIdentity::new(manifest_digest);

    write_manifest(&private, &manifest_bytes, first.planned)?;
    make_private_generation_durable(&private, first.planned)?;

    if cancellation.is_cancelled() {
        return Err(ArtifactPublicationFailure::Cancelled);
    }

    commit_generation(
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

    let reference_bytes = encode_reference(identity, manifest_digest, first.planned)?;

    let warning = commit_reference(
        &layout,
        replacement,
        &reference_bytes,
        first.planned,
        cancellation,
    )?;

    let artifacts = EmittedArtifactSet::from_publication(plan, emitted);

    let generation = PublishedProductGeneration::new(
        identity,
        manifest_digest,
        root.to_owned(),
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

struct ManagedLayout {
    metadata: PathBuf,
    generations: PathBuf,
    reference: PathBuf,
}

fn managed_root<'prepared>(prepared: &'prepared [PreparedArtifact<'_, '_>]) -> Option<&'prepared Path> {
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
    let metadata = root.join(METADATA_DIRECTORY);
    let generations = metadata.join(GENERATIONS_DIRECTORY);

    require_directory(root, planned)?;

    create_managed_directory(root, &metadata, planned)?;
    create_managed_directory(&metadata, &generations, planned)?;

    let supported = atomic_rename_exclusive_is_supported(&generations).map_err(|error| {
        artifact_failure(planned, PublicationErrorKind::Open(error.kind()))
    })?;

    if !supported {
        return Err(artifact_failure(
            planned,
            PublicationErrorKind::ManagedPublicationUnsupported,
        ));
    }

    sync_directory(&generations).map_err(|error| {
        artifact_failure(planned, PublicationErrorKind::Flush(error.kind()))
    })?;

    Ok(ManagedLayout {
        reference: metadata.join(PUBLISHED_REFERENCE),
        metadata,
        generations,
    })
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

    sync_directory(parent).map_err(|error| {
        artifact_failure(planned, PublicationErrorKind::Flush(error.kind()))
    })
}

fn require_directory(
    path: &Path,
    planned: &crate::PlannedArtifact,
) -> Result<(), ArtifactPublicationFailure> {
    let metadata = std::fs::symlink_metadata(path).map_err(|error| {
        artifact_failure(planned, PublicationErrorKind::Open(error.kind()))
    })?;

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
        .tempdir_in(&layout.generations)
        .map_err(|error| artifact_failure(planned, PublicationErrorKind::Open(error.kind())))
}

fn stage_generation(
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

        manifest_artifacts.push(ManifestArtifact::new(
            artifact,
            relative.as_str(),
            &digest,
            permissions,
        ));

        // Published records must own immutable plan facts after publication returns.
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

fn write_artifact(
    destination: &Path,
    artifact: &PreparedArtifact<'_, '_>,
    cancellation: &dyn Cancellation,
) -> Result<(ArtifactDigest, ManifestPermissions), ArtifactPublicationFailure> {
    let mut file = create_new_artifact_file(destination, artifact.planned.id().kind()).map_err(
        |error| artifact_failure(artifact.planned, PublicationErrorKind::Open(error.kind())),
    )?;

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
    serde_json::to_vec(manifest).map_err(|_| {
        artifact_failure(planned, PublicationErrorKind::InvalidGenerationManifest)
    })
}

fn write_manifest(
    private: &TempDir,
    bytes: &[u8],
    planned: &crate::PlannedArtifact,
) -> Result<(), ArtifactPublicationFailure> {
    let path = private.path().join(GENERATION_MANIFEST);
    let mut file = create_new_file(&path, planned)?;

    file.write_all(bytes).map_err(|error| {
        artifact_failure(planned, PublicationErrorKind::Write(error.kind()))
    })?;

    file.flush().map_err(|error| {
        artifact_failure(planned, PublicationErrorKind::Flush(error.kind()))
    })?;

    file.sync_all().map_err(|error| {
        artifact_failure(planned, PublicationErrorKind::Flush(error.kind()))
    })
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
    sync_directory(private.path()).map_err(|error| {
        artifact_failure(planned, PublicationErrorKind::Flush(error.kind()))
    })
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
) -> Result<(), ArtifactPublicationFailure> {
    let destination = layout.generations.join(identity.to_hex());

    match atomic_rename_exclusive(private.path(), &destination) {
        Ok(()) => {
            sync_directory(&layout.generations).map_err(|error| {
                artifact_failure(planned, PublicationErrorKind::Commit(error.kind()))
            })?;
        }
        Err(_) if destination.exists() => {
            validate_existing_generation(
                &destination,
                manifest_bytes,
                manifest,
                artifacts,
                planned,
                cancellation,
            )?;
        }
        Err(error) => {
            return Err(artifact_failure(
                planned,
                PublicationErrorKind::Commit(error.kind()),
            ));
        }
    }

    Ok(())
}

fn validate_existing_generation(
    destination: &Path,
    manifest_bytes: &[u8],
    manifest: &GenerationManifest,
    artifacts: &[EmittedArtifact],
    planned: &crate::PlannedArtifact,
    cancellation: &dyn Cancellation,
) -> Result<(), ArtifactPublicationFailure> {
    let existing_manifest = std::fs::read(destination.join(GENERATION_MANIFEST)).map_err(|_| {
        artifact_failure(planned, PublicationErrorKind::GenerationCollision)
    })?;

    if existing_manifest != manifest_bytes || manifest.artifacts.len() != artifacts.len() {
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
            artifact: relative,
            ..
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
        .map_err(|_| {
            artifact_failure(planned, PublicationErrorKind::GenerationCollision)
        })?;
    }

    Ok(())
}

fn encode_reference(
    identity: ProductGenerationIdentity,
    manifest_digest: [u8; 32],
    planned: &crate::PlannedArtifact,
) -> Result<Vec<u8>, ArtifactPublicationFailure> {
    serde_json::to_vec(&GenerationReference {
        revision: MANIFEST_REVISION,
        generation: identity.to_hex(),
        manifest_digest: lowercase_hex(&manifest_digest),
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
        .map_err(|error| {
            artifact_failure(planned, PublicationErrorKind::Open(error.kind()))
        })?;

    staging.write_all(bytes).map_err(|error| {
        artifact_failure(planned, PublicationErrorKind::Write(error.kind()))
    })?;

    let staging = staging.finish().map_err(|error| {
        artifact_failure(planned, PublicationErrorKind::Flush(error.kind()))
    })?;

    if cancellation.is_cancelled() {
        return Err(ArtifactPublicationFailure::Cancelled);
    }

    staging.promote(&layout.reference).map_err(|error| {
        artifact_failure(planned, PublicationErrorKind::Commit(error.kind()))
    })?;

    Ok(sync(&layout.metadata)
        .err()
        .map(|error| planned_error(planned, PublicationErrorKind::Commit(error.kind()))))
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GenerationManifest {
    revision: u32,
    pub(super) product: ManifestProduct,
    pub(super) artifacts: Vec<ManifestArtifact>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ManifestProduct {
    pub(super) package: String,
    pub(super) name: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ManifestArtifact {
    pub(super) kind: String,
    pub(super) ordinal: u32,
    requirement: String,
    role: String,
    pub(super) path: String,
    pub(super) byte_len: u64,
    pub(super) digest_algorithm: String,
    pub(super) digest: String,
    pub(super) permissions: ManifestPermissions,
    producer: ManifestProducer,
}

impl ManifestArtifact {
    fn new(
        artifact: &PreparedArtifact<'_, '_>,
        path: &str,
        digest: &ArtifactDigest,
        permissions: ManifestPermissions,
    ) -> Self {
        Self {
            kind: artifact.planned.id().kind().machine_key().to_owned(),
            ordinal: artifact.planned.id().ordinal(),
            requirement: requirement_key(artifact.planned.requirement()).to_owned(),
            role: role_key(artifact.planned.role()).to_owned(),
            path: path.to_owned(),
            byte_len: artifact.content.byte_len(),
            digest_algorithm: digest_algorithm_key(digest.algorithm()).to_owned(),
            digest: lowercase_hex(digest.bytes()),
            permissions,
            producer: ManifestProducer::new(artifact.planned.producer()),
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ManifestPermissions {
    pub(super) logical: String,
    unix_mode: Option<u32>,
}

impl ManifestPermissions {
    fn read(path: &Path, kind: ArtifactKind) -> std::io::Result<Self> {
        let metadata = std::fs::symlink_metadata(path)?;

        if !metadata.file_type().is_file() {
            return Err(std::io::Error::from(std::io::ErrorKind::InvalidData));
        }

        Ok(Self {
            logical: permission_key(kind).to_owned(),
            unix_mode: unix_mode(&metadata.permissions()),
        })
    }

    pub(super) fn matches(&self, path: &Path) -> std::io::Result<bool> {
        let metadata = std::fs::symlink_metadata(path)?;

        Ok(metadata.file_type().is_file()
            && self.unix_mode == unix_mode(&metadata.permissions()))
    }
}

#[cfg(unix)]
fn unix_mode(permissions: &std::fs::Permissions) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;

    Some(permissions.mode() & 0o777)
}

#[cfg(not(unix))]
const fn unix_mode(_: &std::fs::Permissions) -> Option<u32> {
    None
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ManifestProducer {
    Backend {
        name: String,
        revision: String,
        toolchain_revision: String,
        unit: String,
        artifact_kind: String,
        ordinal: u32,
    },
    PackageInterface,
    PackageImplementation,
    DependencyMetadata { ordinal: u32 },
    Linker { ordinal: u32 },
}

impl ManifestProducer {
    fn new(producer: &ArtifactProducer) -> Self {
        match producer {
            ArtifactProducer::Backend { artifact, backend } => Self::Backend {
                name: backend.name().to_owned(),
                revision: backend.revision().to_owned(),
                toolchain_revision: backend.toolchain_revision().to_owned(),
                unit: lowercase_hex(&artifact.unit().content_identity()),
                artifact_kind: backend_artifact_kind_key(artifact.kind()).to_owned(),
                ordinal: artifact.ordinal(),
            },
            ArtifactProducer::PackageInterface => Self::PackageInterface,
            ArtifactProducer::PackageImplementation => Self::PackageImplementation,
            ArtifactProducer::DependencyMetadata(identity) => Self::DependencyMetadata {
                ordinal: identity.ordinal(),
            },
            ArtifactProducer::Linker(identity) => Self::Linker {
                ordinal: identity.ordinal(),
            },
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GenerationReference {
    revision: u32,
    pub(super) generation: String,
    pub(super) manifest_digest: String,
}

const fn requirement_key(requirement: ArtifactRequirement) -> &'static str {
    match requirement {
        ArtifactRequirement::Required => "required",
        ArtifactRequirement::Optional => "optional",
    }
}

const fn role_key(role: ArtifactRole) -> &'static str {
    match role {
        ArtifactRole::Product => "product",
        ArtifactRole::Inspection => "inspection",
        ArtifactRole::LinkInput => "link_input",
        ArtifactRole::Companion => "companion",
    }
}

const fn digest_algorithm_key(algorithm: ArtifactDigestAlgorithm) -> &'static str {
    match algorithm {
        ArtifactDigestAlgorithm::Blake3 => "blake3",
        ArtifactDigestAlgorithm::Sha256 => "sha256",
    }
}

pub(super) const fn permission_key(kind: ArtifactKind) -> &'static str {
    match kind {
        ArtifactKind::Executable | ArtifactKind::ExecutableModule => "executable",
        ArtifactKind::Assembly
        | ArtifactKind::BackendIr
        | ArtifactKind::BackendBitcode
        | ArtifactKind::RelocatableObject
        | ArtifactKind::DebugCompanion
        | ArtifactKind::PackageInterface
        | ArtifactKind::PackageImplementation
        | ArtifactKind::DependencyMetadata
        | ArtifactKind::StaticLibrary
        | ArtifactKind::SharedLibrary
        | ArtifactKind::LinkedCompanion => "data",
    }
}

const fn backend_artifact_kind_key(kind: BackendArtifactKind) -> &'static str {
    match kind {
        BackendArtifactKind::RelocatableObject => "relocatable_object",
        BackendArtifactKind::Assembly => "assembly",
        BackendArtifactKind::BackendIr => "backend_ir",
        BackendArtifactKind::BackendBitcode => "backend_bitcode",
        BackendArtifactKind::ExecutableModule => "executable_module",
        BackendArtifactKind::DebugCompanion => "debug_companion",
    }
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
            generations: metadata.join("generations"),
            reference: reference.clone(),
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
