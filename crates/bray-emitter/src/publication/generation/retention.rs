use std::collections::BTreeSet;
use std::path::Path;

use bray_base::Cancellation;

use super::layout::is_product_intermediate;
use super::locator::GenerationLocator;
use super::manifest::GenerationManifest;
use super::reader::PublishedGenerationReadError;
use super::reference::{GenerationReference, GenerationReferenceEntry, decode_revision};
use super::transaction::{GENERATION_MANIFEST, MANIFEST_REVISION, PUBLISHED_REFERENCE};
use crate::storage::{
    ManagedStore, StorageLease, check_cancelled, children, managed_directory_exists,
    read_owned_file, remove_owned_tree, require_directory,
};
use crate::{ManagedArtifactPath, StorageError, StorageErrorKind, StorageOperation};

pub(super) fn read_manifest(
    store: &Path,
    entry: &GenerationReferenceEntry,
) -> Result<GenerationManifest, StorageError> {
    let locator = entry
        .locator()
        .map_err(|error| error.into_storage_error(store))?;

    let generation = store.join(locator.to_hex());

    require_directory(&generation)?;

    let path = generation.join(GENERATION_MANIFEST);
    let bytes = read_owned_file(&path)?;

    let identity = entry
        .identity()
        .map_err(|error| error.into_storage_error(&path))?;

    let actual = *blake3::hash(&bytes).as_bytes();

    if actual != identity.as_bytes() {
        let digest = |bytes| {
            bray_codegen::ArtifactDigest::try_new(
                bray_codegen::ArtifactDigestAlgorithm::Blake3,
                bytes,
            )
            .ok_or_else(|| {
                PublishedGenerationReadError::InvalidGenerationReference.into_storage_error(&path)
            })
        };

        return Err(StorageError::new(
            &path,
            StorageErrorKind::ArtifactDigest {
                expected: digest(identity.as_bytes())?,
                actual: digest(actual)?,
            },
        ));
    }

    decode_manifest(&path, &bytes)
}

pub(super) fn read_unreferenced_manifest(
    directory: &Path,
) -> Result<Option<GenerationManifest>, StorageError> {
    let path = directory.join(GENERATION_MANIFEST);

    let bytes = match read_owned_file(&path) {
        Ok(bytes) => bytes,
        Err(error)
            if matches!(
                error.kind(),
                StorageErrorKind::Io {
                    cause: std::io::ErrorKind::NotFound,
                    ..
                }
            ) =>
        {
            return Ok(None);
        }
        Err(error) => return Err(error),
    };

    decode_manifest(&path, &bytes).map(Some)
}

fn decode_manifest(path: &Path, bytes: &[u8]) -> Result<GenerationManifest, StorageError> {
    let revision = decode_revision(bytes).map_err(|error| StorageError::json(path, error))?;

    if revision != MANIFEST_REVISION {
        return Err(
            PublishedGenerationReadError::UnsupportedManifestRevision(revision)
                .into_storage_error(path),
        );
    }

    let manifest: GenerationManifest =
        serde_json::from_slice(bytes).map_err(|error| StorageError::json(path, error))?;

    manifest.context.validate(path, Some(&manifest.product))?;
    super::validation::validate_manifest(path, &manifest)?;

    Ok(manifest)
}

pub(crate) fn maintain_product(
    managed: &mut ManagedStore,
    store: &Path,
    product_key: &str,
    retire: bool,
    cancellation: &dyn Cancellation,
) -> Result<(), StorageError> {
    super::recovery::recover_publication(managed.root(), store, cancellation)?;
    let reference_path = store.join(PUBLISHED_REFERENCE);

    let reference = GenerationReference::read(&reference_path)
        .map_err(|error| error.into_storage_error(&reference_path))?;

    let current_paths = reference
        .as_ref()
        .map(|reference| {
            read_manifest(store, &reference.current).and_then(|manifest| {
                manifest
                    .artifacts
                    .into_iter()
                    .map(|artifact| {
                        ManagedArtifactPath::try_new(artifact.published_path).ok_or_else(|| {
                            PublishedGenerationReadError::InvalidArtifact
                                .into_storage_error(&reference_path)
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()
            })
        })
        .transpose()?
        .unwrap_or_default();

    managed.reconcile_product_public_paths(product_key, current_paths)?;

    if retire {
        for relative in managed.product_public_paths(product_key)? {
            check_cancelled(store, cancellation)?;
            remove_public_file(managed.root(), &relative)?;
        }
    } else {
        prune_generations(managed, store, reference.as_ref(), cancellation)?;
    }

    Ok(())
}

pub(super) fn prune_generations(
    managed: &mut ManagedStore,
    store: &Path,
    reference: Option<&GenerationReference>,
    cancellation: &dyn Cancellation,
) -> Result<(), StorageError> {
    let retained: BTreeSet<_> = reference
        .into_iter()
        .flat_map(|reference| std::iter::once(&reference.current).chain(reference.previous.iter()))
        .map(|entry| entry.locator.as_str())
        .collect();

    for path in children(store)? {
        check_cancelled(store, cancellation)?;

        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };

        if GenerationLocator::try_from_hex(name).is_some() && !retained.contains(name) {
            require_directory(&path)?;

            let Some(lease) = StorageLease::try_exclusive(&path.join("lease.lock"))? else {
                continue;
            };

            // The caller owns the product publication or root maintenance lock until deletion completes.
            managed.forget_content_directory(&path)?;
            drop(lease);
            remove_owned_tree(&path, cancellation)?;
        } else if is_product_intermediate(name) && name != "staging" {
            remove_owned_tree(&path, cancellation)?;
        } else if name == "staging" {
            require_directory(&path)?;

            for staging in children(&path)? {
                remove_owned_tree(&staging, cancellation)?;
            }
        }
    }

    Ok(())
}

pub(super) fn remove_public_file(root: &Path, relative: &Path) -> Result<(), StorageError> {
    let parent = relative.parent().unwrap_or_else(|| Path::new(""));

    if !managed_directory_exists(root, parent)? {
        return Ok(());
    }

    let path = root.join(relative);

    match std::fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_file() => std::fs::remove_file(&path)
            .map_err(|error| StorageError::io(&path, StorageOperation::Remove, error)),
        Ok(_) => Err(StorageError::new(&path, StorageErrorKind::UnsafePath)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(StorageError::io(&path, StorageOperation::Inspect, error)),
    }
}
