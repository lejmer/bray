use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use bray_base::Cancellation;
use bray_symbols::ProductIdentity;

use super::layout::product_store;
use super::reader::{PublishedGenerationReadError, lock_product_destination};
use super::reference::GenerationReference;
use super::retention::read_manifest;
use super::transaction::PUBLISHED_REFERENCE;
use super::validation::{artifact_path, validate_artifact_file};
use crate::storage::StorageLease;
use crate::{
    ArtifactKind, ManagedFilesystemDestination, ManagedOutputDirectory, ProductGenerationIdentity,
};

/// One validated immutable generation, including every product file and companion.
///
/// Publication may advance while this value lives. Cleanup cannot remove its files. A caller
/// executing a retained product must also retain the generations of its required dependencies.
#[derive(Debug)]
pub struct RetainedProductGeneration {
    identity: ProductGenerationIdentity,
    build_identity: Option<crate::ProductBuildIdentity>,
    artifacts: BTreeMap<(ArtifactKind, u32), PathBuf>,
    _lease: StorageLease,
    _entry_lease: StorageLease,
}

impl RetainedProductGeneration {
    /// Returns the verified content identity of this complete product generation.
    pub const fn identity(&self) -> ProductGenerationIdentity {
        self.identity
    }

    /// Returns the complete build identity recorded for explicit product reuse.
    pub const fn build_identity(&self) -> Option<&crate::ProductBuildIdentity> {
        self.build_identity.as_ref()
    }

    /// Borrows a validated immutable artifact path while the entire generation remains pinned.
    pub fn artifact_path(&self, kind: ArtifactKind, ordinal: u32) -> Option<&Path> {
        self.artifacts.get(&(kind, ordinal)).map(PathBuf::as_path)
    }

    /// Returns every pinned artifact in kind and ordinal order, including required companions.
    pub fn artifacts(&self) -> impl Iterator<Item = (ArtifactKind, u32, &Path)> + '_ {
        self.artifacts
            .iter()
            .map(|(&(kind, ordinal), path)| (kind, ordinal, path.as_path()))
    }
}

/// Resolves and pins the current generation before validating all files needed by retained readers.
pub fn retain_published_generation(
    destination: impl Into<ManagedFilesystemDestination>,
    product: &ProductIdentity,
    cancellation: &dyn Cancellation,
) -> Result<RetainedProductGeneration, PublishedGenerationReadError> {
    let destination = destination.into();
    let guard = lock_product_destination(&destination, product)?;

    let public = destination
        .relative_directory()
        .map_or_else(PathBuf::new, ManagedOutputDirectory::to_path_buf);

    let store = product_store(destination.root(), &public, product);

    let reference = GenerationReference::read(&store.join(PUBLISHED_REFERENCE))?
        .ok_or(PublishedGenerationReadError::ArtifactUnavailable)?;

    let identity = reference.current.identity()?;
    let directory = store.join(reference.current.locator()?.to_hex());

    let manifest = read_manifest(&store, &reference.current)
        .map_err(|error| PublishedGenerationReadError::Storage(Box::new(error)))?;

    if manifest.product.identity().as_ref() != Some(product) {
        return Err(PublishedGenerationReadError::ProductMismatch);
    }

    let lease = StorageLease::acquire(&directory.join("lease.lock"))
        .map_err(|error| PublishedGenerationReadError::Storage(Box::new(error)))?;

    let mut artifacts = BTreeMap::new();

    let build_identity = manifest.build_identity;

    for artifact in manifest.artifacts {
        let path = artifact_path(&directory, &artifact.path)
            .map_err(|error| PublishedGenerationReadError::Storage(Box::new(error)))?;

        validate_artifact_file(&path, &artifact, cancellation)
            .map_err(|error| PublishedGenerationReadError::Storage(Box::new(error)))?;

        if artifacts
            .insert((artifact.kind, artifact.ordinal), path)
            .is_some()
        {
            return Err(PublishedGenerationReadError::ArtifactUnavailable);
        }
    }

    let entry_lease = guard.into_entry_lease();

    Ok(RetainedProductGeneration {
        identity,
        build_identity,
        artifacts,
        _lease: lease,
        _entry_lease: entry_lease,
    })
}
