use std::path::PathBuf;

use super::layout::{product_store, product_store_relative};
use crate::storage::{ManagedStore, StorageLease};

use super::lock::open_lock_file;
use super::reference::GenerationReference;
use super::retention::read_manifest;
use super::transaction::{MANIFEST_REVISION, PUBLISHED_REFERENCE};
use super::validation::{artifact_path, validate_artifact_file};
use crate::{ArtifactKind, ManagedFilesystemDestination, ManagedOutputDirectory};

/// Failure to resolve one artifact from the atomically published product generation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PublishedGenerationReadError {
    /// Managed storage cannot be pinned or read.
    Storage(Box<crate::StorageError>),
    /// The publication reference uses an unsupported schema revision.
    UnsupportedReferenceRevision(u32),
    /// The referenced generation locator or manifest digest is not canonical.
    InvalidGenerationReference,
    /// The generation manifest uses an unsupported schema revision.
    UnsupportedManifestRevision(u32),
    /// The generation belongs to another product.
    ProductMismatch,
    /// The requested artifact is absent or ambiguous.
    ArtifactUnavailable,
    /// The requested artifact file violates its manifest contract.
    InvalidArtifact,
}

impl PublishedGenerationReadError {
    /// Preserves the validation or operating-system cause with the affected retained-state path.
    pub fn into_storage_error(self, path: &std::path::Path) -> crate::StorageError {
        use bray_diagnostics::DiagnosticRetainedGenerationProblem as Problem;

        let problem = match self {
            Self::Storage(error) => return *error,
            Self::UnsupportedReferenceRevision(actual) => Problem::ReferenceRevision {
                expected: super::reference::REFERENCE_REVISION,
                actual,
            },
            Self::InvalidGenerationReference => Problem::ReferenceIdentity,
            Self::UnsupportedManifestRevision(actual) => Problem::ManifestRevision {
                expected: MANIFEST_REVISION,
                actual,
            },
            Self::ProductMismatch => Problem::ProductIdentity,
            Self::ArtifactUnavailable => Problem::ArtifactSelection,
            Self::InvalidArtifact => Problem::ArtifactContract,
        };

        crate::StorageError::new(path, crate::StorageErrorKind::Generation(problem))
    }
}

/// Shared product-publication lock held while stable public artifact paths are consumed.
#[derive(Debug)]
pub struct PublishedProductReadGuard {
    _file: std::fs::File,
    _entry_lease: StorageLease,
}

impl PublishedProductReadGuard {
    pub(super) fn into_entry_lease(self) -> StorageLease {
        self._entry_lease
    }
}

/// A stable public artifact and the ownership that protects it and its product companions.
#[derive(Debug)]
pub struct PublishedArtifact {
    path: PathBuf,
    _guard: PublishedProductReadGuard,
}

impl PublishedArtifact {
    /// Returns the stable path while publication and cleanup remain excluded by this handle.
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl AsRef<std::path::Path> for PublishedArtifact {
    fn as_ref(&self) -> &std::path::Path {
        self.path()
    }
}

/// Waits until any product publication finishes and prevents replacement until this guard drops.
pub fn lock_published_product(
    destination: impl Into<ManagedFilesystemDestination>,
    product: &bray_symbols::ProductIdentity,
) -> Result<PublishedProductReadGuard, PublishedGenerationReadError> {
    let destination = destination.into();

    lock_product_destination(&destination, product)
}

pub(super) fn lock_product_destination(
    destination: &ManagedFilesystemDestination,
    product: &bray_symbols::ProductIdentity,
) -> Result<PublishedProductReadGuard, PublishedGenerationReadError> {
    let public_directory = destination
        .relative_directory()
        .map_or_else(PathBuf::new, ManagedOutputDirectory::to_path_buf);

    let store = product_store(destination.root(), &public_directory, product);

    let entry_lease = {
        let mut managed = ManagedStore::open(destination.root())
            .map_err(|error| PublishedGenerationReadError::Storage(Box::new(error)))?;

        managed
            .pin_product(
                &product_store_relative(&public_directory, product),
                crate::StoragePolicy::default(),
            )
            .map_err(|error| PublishedGenerationReadError::Storage(Box::new(error)))?
    };

    let file = open_lock_file(&store)
        .map_err(|error| PublishedGenerationReadError::Storage(Box::new(error)))?;

    let lock_error = |error| {
        PublishedGenerationReadError::Storage(Box::new(crate::StorageError::io(
            &store.join("publication.lock"),
            crate::StorageOperation::Lock,
            error,
        )))
    };

    file.lock_shared().map_err(lock_error)?;

    while super::recovery::pending_publication(&store)
        .map_err(|error| PublishedGenerationReadError::Storage(Box::new(error)))?
    {
        file.unlock().map_err(lock_error)?;
        file.lock().map_err(lock_error)?;

        super::recovery::recover_publication(destination.root(), &store, &|| false)
            .map_err(|error| PublishedGenerationReadError::Storage(Box::new(error)))?;

        file.unlock().map_err(lock_error)?;
        file.lock_shared().map_err(lock_error)?;
    }

    Ok(PublishedProductReadGuard {
        _file: file,
        _entry_lease: entry_lease,
    })
}

/// Resolves and validates one artifact from the currently published product generation.
pub fn resolve_published_artifact(
    destination: impl Into<ManagedFilesystemDestination>,
    product: &bray_symbols::ProductIdentity,
    kind: ArtifactKind,
    ordinal: u32,
) -> Result<PublishedArtifact, PublishedGenerationReadError> {
    let destination = destination.into();

    let public_directory = destination
        .relative_directory()
        .map_or_else(PathBuf::new, ManagedOutputDirectory::to_path_buf);

    let store = product_store(destination.root(), &public_directory, product);
    let guard = lock_product_destination(&destination, product)?;

    let reference_path = store.join(PUBLISHED_REFERENCE);

    let reference = GenerationReference::read(&reference_path)?
        .ok_or(PublishedGenerationReadError::ArtifactUnavailable)?;

    let locator = reference.current.locator()?;
    let generation = store.join(locator.to_hex());

    let manifest = read_manifest(&store, &reference.current)
        .map_err(|error| PublishedGenerationReadError::Storage(Box::new(error)))?;

    if manifest.product.package != product.package().as_str()
        || manifest.product.name != product.name()
    {
        return Err(PublishedGenerationReadError::ProductMismatch);
    }

    let mut matching = manifest
        .artifacts
        .iter()
        .filter(|artifact| artifact.kind == kind && artifact.ordinal == ordinal);

    let artifact = matching
        .next()
        .ok_or(PublishedGenerationReadError::ArtifactUnavailable)?;

    if matching.next().is_some() {
        return Err(PublishedGenerationReadError::ArtifactUnavailable);
    }

    let internal_path = artifact_path(&generation, &artifact.path)
        .map_err(|error| PublishedGenerationReadError::Storage(Box::new(error)))?;

    let path = artifact_path(destination.root(), &artifact.published_path)
        .map_err(|error| PublishedGenerationReadError::Storage(Box::new(error)))?;

    validate_artifact_file(&internal_path, artifact, &|| false)
        .map_err(|error| PublishedGenerationReadError::Storage(Box::new(error)))?;

    validate_artifact_file(&path, artifact, &|| false)
        .map_err(|error| PublishedGenerationReadError::Storage(Box::new(error)))?;

    Ok(PublishedArtifact {
        path,
        _guard: guard,
    })
}
