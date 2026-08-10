use std::collections::BTreeSet;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use bray_base::{
    Cancellation, StagedFile, atomic_rename_exclusive, atomic_rename_exclusive_is_supported,
    lowercase_hex, sync_directory,
};
use bray_codegen::{ArtifactDigest, ArtifactDigestAlgorithm, BackendArtifactKind};
use serde::Serialize;
use tempfile::{Builder, TempDir};

use super::diagnostic::PublicationErrorKind;
use super::operation::{
    ArtifactPublicationFailure, PreparedArtifact, artifact_failure, content_failure, copy_content,
};
use super::staging::{default_permissions, replacement_mode};
use crate::artifact::content::validate_staged_content;
use crate::{
    ArtifactKind, ArtifactProducer, ArtifactRequirement, ArtifactRole, EmissionPlan,
    EmittedArtifact, EmittedArtifactSet, OutputSink, PlannedArtifactDestination,
    ProductGenerationIdentity, PublishedProductGeneration, ReplacementPolicy,
};

const METADATA_DIRECTORY: &str = ".bray";
const GENERATIONS_DIRECTORY: &str = "generations";
const GENERATION_MANIFEST: &str = "manifest.json";
const PUBLISHED_REFERENCE: &str = "published-generation.json";
const PRIVATE_GENERATION_PREFIX: &str = ".bray-generation-";
const MANIFEST_REVISION: u32 = 1;

pub(super) struct ManagedGenerationPublication {
    pub(super) artifacts: EmittedArtifactSet,
    pub(super) generation: PublishedProductGeneration,
}

pub(super) fn publish_managed_generation(
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
        &emitted,
        first.planned,
        cancellation,
    )?;

    if cancellation.is_cancelled() {
        return Err(ArtifactPublicationFailure::Cancelled);
    }

    let reference_bytes = encode_reference(identity, manifest_digest, first.planned)?;

    commit_reference(
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

    std::fs::create_dir_all(&generations).map_err(|error| {
        artifact_failure(planned, PublicationErrorKind::Open(error.kind()))
    })?;

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

    sync_directory(&metadata).map_err(|error| {
        artifact_failure(planned, PublicationErrorKind::Flush(error.kind()))
    })?;

    Ok(ManagedLayout {
        reference: metadata.join(PUBLISHED_REFERENCE),
        metadata,
        generations,
    })
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

        created_directories.insert(parent.to_owned());

        let digest = write_artifact(&destination, artifact, cancellation)?;

        let PlannedArtifactDestination::Publish(sink) = artifact.planned.destination() else {
            return Err(artifact_failure(
                artifact.planned,
                PublicationErrorKind::InvalidContribution,
            ));
        };

        manifest_artifacts.push(ManifestArtifact::new(artifact, relative.as_str(), &digest));

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
) -> Result<ArtifactDigest, ArtifactPublicationFailure> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|error| {
            artifact_failure(artifact.planned, PublicationErrorKind::Open(error.kind()))
        })?;

    copy_content(cancellation, artifact.planned, &artifact.content, &mut file)?;

    file.flush().map_err(|error| {
        artifact_failure(artifact.planned, PublicationErrorKind::Flush(error.kind()))
    })?;

    if let Some(permissions) = default_permissions(artifact.planned.id().kind()) {
        file.set_permissions(permissions).map_err(|error| {
            artifact_failure(artifact.planned, PublicationErrorKind::Flush(error.kind()))
        })?;
    }

    file.sync_all().map_err(|error| {
        artifact_failure(artifact.planned, PublicationErrorKind::Flush(error.kind()))
    })?;

    validate_staged_content(
        destination,
        artifact.content.byte_len(),
        artifact.content.digest(),
        cancellation,
    )
    .map_err(|error| content_failure(artifact.planned, error))
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
    OpenOptions::new()
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
    manifest: &[u8],
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
    manifest: &[u8],
    artifacts: &[EmittedArtifact],
    planned: &crate::PlannedArtifact,
    cancellation: &dyn Cancellation,
) -> Result<(), ArtifactPublicationFailure> {
    let existing_manifest = std::fs::read(destination.join(GENERATION_MANIFEST)).map_err(|_| {
        artifact_failure(planned, PublicationErrorKind::GenerationCollision)
    })?;

    if existing_manifest != manifest {
        return Err(artifact_failure(
            planned,
            PublicationErrorKind::GenerationCollision,
        ));
    }

    for artifact in artifacts {
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

        validate_staged_content(
            &destination.join(relative.to_path_buf()),
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
) -> Result<(), ArtifactPublicationFailure> {
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

    sync_directory(&layout.metadata).map_err(|error| {
        artifact_failure(planned, PublicationErrorKind::Commit(error.kind()))
    })
}

#[derive(Serialize)]
struct GenerationManifest {
    revision: u32,
    product: ManifestProduct,
    artifacts: Vec<ManifestArtifact>,
}

#[derive(Serialize)]
struct ManifestProduct {
    package: String,
    name: String,
}

#[derive(Serialize)]
struct ManifestArtifact {
    kind: &'static str,
    ordinal: u32,
    requirement: &'static str,
    role: &'static str,
    path: String,
    byte_len: u64,
    digest_algorithm: &'static str,
    digest: String,
    permissions: &'static str,
    producer: ManifestProducer,
}

impl ManifestArtifact {
    fn new(
        artifact: &PreparedArtifact<'_, '_>,
        path: &str,
        digest: &ArtifactDigest,
    ) -> Self {
        Self {
            kind: artifact.planned.id().kind().machine_key(),
            ordinal: artifact.planned.id().ordinal(),
            requirement: requirement_key(artifact.planned.requirement()),
            role: role_key(artifact.planned.role()),
            path: path.to_owned(),
            byte_len: artifact.content.byte_len(),
            digest_algorithm: digest_algorithm_key(digest.algorithm()),
            digest: lowercase_hex(digest.bytes()),
            permissions: permission_key(artifact.planned.id().kind()),
            producer: ManifestProducer::new(artifact.planned.producer()),
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ManifestProducer {
    Backend {
        name: String,
        revision: String,
        toolchain_revision: String,
        unit: String,
        artifact_kind: &'static str,
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
                artifact_kind: backend_artifact_kind_key(artifact.kind()),
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

#[derive(Serialize)]
struct GenerationReference {
    revision: u32,
    generation: String,
    manifest_digest: String,
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

const fn permission_key(kind: ArtifactKind) -> &'static str {
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
