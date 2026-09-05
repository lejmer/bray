use std::collections::BTreeSet;
use std::path::Path;

use bray_base::Cancellation;

use super::layout::is_product_intermediate;
use super::locator::GenerationLocator;
use super::reader::PublishedGenerationReadError;
use super::reference::GenerationReference;
use super::retention::{read_manifest, read_unreferenced_manifest};
use super::transaction::PUBLISHED_REFERENCE;
use crate::storage::{
    ManagedStore, StorageAccounting, StorageContext, StorageProduct, check_cancelled, children,
    managed_directory_exists, remove_owned_tree, write_owned_json,
};
use crate::{
    ManagedArtifactPath, StorageCategory, StorageEntryReport, StorageError, StorageKind,
    StorageOperation, StorageSelection,
};

pub(crate) fn product_storage_rows(
    root: &Path,
    store: &Path,
    fallback_product: Option<&StorageProduct>,
    fallback_context: &StorageContext,
    selection: &StorageSelection,
    accounting: &mut StorageAccounting,
    cancellation: &dyn Cancellation,
) -> Result<Vec<StorageEntryReport>, StorageError> {
    let reference_path = store.join(PUBLISHED_REFERENCE);

    let reference = GenerationReference::read(&reference_path)
        .map_err(|error| error.into_storage_error(&reference_path))?;

    let mut rows = Vec::new();
    let mut retained = BTreeSet::new();

    if let Some(reference) = &reference {
        for (entry, category) in
            std::iter::once((&reference.current, StorageCategory::RetainedRerun)).chain(
                reference
                    .previous
                    .iter()
                    .map(|entry| (entry, StorageCategory::RetainedHistory)),
            )
        {
            let manifest = read_manifest(store, entry)?;
            let directory = store.join(&entry.locator);

            retained.insert(directory.clone());

            if selection.kind == Some(StorageKind::Intermediates)
                || !selection.matches_context(&manifest.context)
            {
                continue;
            }

            rows.push(StorageEntryReport::measure(
                &directory,
                Some(&manifest.product),
                &manifest.context,
                category,
                accounting,
                cancellation,
            )?);

            if category == StorageCategory::RetainedRerun {
                let mut bytes = 0_u64;
                let mut shared = 0_u64;

                for artifact in manifest.artifacts {
                    let relative = ManagedArtifactPath::try_new(artifact.published_path)
                        .ok_or_else(|| {
                            PublishedGenerationReadError::InvalidArtifact
                                .into_storage_error(&reference_path)
                        })?;

                    let path = root.join(relative.to_path_buf());
                    let parent = relative.to_path_buf();
                    let parent = parent.parent().unwrap_or_else(|| Path::new(""));

                    if managed_directory_exists(root, parent)? {
                        let (file_bytes, file_shared) = accounting.measure(&path, cancellation)?;

                        bytes = bytes.checked_add(file_bytes).ok_or_else(|| {
                            StorageError::io(
                                &path,
                                StorageOperation::Inspect,
                                std::io::ErrorKind::FileTooLarge.into(),
                            )
                        })?;

                        shared = shared.checked_add(file_shared).ok_or_else(|| {
                            StorageError::io(
                                &path,
                                StorageOperation::Inspect,
                                std::io::ErrorKind::FileTooLarge.into(),
                            )
                        })?;
                    }
                }

                let mut row = StorageEntryReport::new(
                    root,
                    Some(&manifest.product),
                    &manifest.context,
                    StorageCategory::CurrentOutputs,
                    Some(bytes),
                )?;

                row.shared_bytes = Some(shared);
                rows.push(row);
            }
        }
    }

    for path in children(store)? {
        if retained.contains(&path) {
            continue;
        }

        let name = path.file_name().and_then(|name| name.to_str());

        let category = if matches!(
            name,
            Some(PUBLISHED_REFERENCE | "entry.lock" | "publication.lock")
        ) {
            StorageCategory::RetainedRerun
        } else {
            StorageCategory::Reclaimable
        };

        let manifest = if name.is_some_and(|name| GenerationLocator::try_from_hex(name).is_some()) {
            read_unreferenced_manifest(&path)?
        } else {
            None
        };

        let context = manifest
            .as_ref()
            .map_or(fallback_context, |manifest| &manifest.context);

        let product = manifest
            .as_ref()
            .map(|manifest| &manifest.product)
            .or(fallback_product);

        let child_kind = if name.is_some_and(is_product_intermediate) {
            StorageKind::Intermediates
        } else {
            StorageKind::Products
        };

        if !selection.matches_context(context)
            || selection
                .kind
                .is_some_and(|selected| selected != child_kind)
        {
            continue;
        }

        rows.push(StorageEntryReport::measure(
            &path,
            product,
            context,
            category,
            accounting,
            cancellation,
        )?);
    }

    Ok(rows)
}

pub(crate) fn clean_product_rows(
    managed: &mut ManagedStore,
    store: &Path,
    rows: &mut [StorageEntryReport],
    cancellation: &dyn Cancellation,
) -> Result<(), StorageError> {
    super::recovery::recover_publication(managed.root(), store, cancellation)?;
    let reference_path = store.join(PUBLISHED_REFERENCE);

    let mut reference = GenerationReference::read(&reference_path)
        .map_err(|error| error.into_storage_error(&reference_path))?;

    if let Some(reference) = &mut reference
        && reference.previous.as_ref().is_some_and(|previous| {
            rows.iter()
                .any(|row| row.path == store.join(&previous.locator))
        })
    {
        reference.previous = None;
        write_owned_json(&reference_path, reference)?;
    }

    for row in rows {
        if !matches!(
            row.category,
            StorageCategory::Reclaimable | StorageCategory::RetainedHistory
        ) {
            continue;
        }

        check_cancelled(&row.path, cancellation)?;
        managed.forget_content_directory(&row.path)?;
        remove_owned_tree(&row.path, cancellation)?;
        row.removed = true;
    }

    Ok(())
}

pub(crate) fn selected_current_product(
    store: &Path,
    selection: &StorageSelection,
) -> Result<Option<bool>, StorageError> {
    let reference_path = store.join(PUBLISHED_REFERENCE);

    let reference = GenerationReference::read(&reference_path)
        .map_err(|error| error.into_storage_error(&reference_path))?;

    reference
        .map(|reference| {
            let current = read_manifest(store, &reference.current)?;

            Ok(selection.kind != Some(StorageKind::Intermediates)
                && selection.matches_context(&current.context))
        })
        .transpose()
}
