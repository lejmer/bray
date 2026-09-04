use std::path::Path;

use bray_base::Cancellation;

use super::error::{StorageError, StorageErrorKind, StorageOperation};

pub(crate) fn check_cancelled(
    path: &Path,
    cancellation: &dyn Cancellation,
) -> Result<(), StorageError> {
    if cancellation.is_cancelled() {
        return Err(StorageError::new(path, StorageErrorKind::Cancelled));
    }

    Ok(())
}

pub(crate) fn tree_bytes(
    root: &Path,
    cancellation: &dyn Cancellation,
) -> Result<u64, StorageError> {
    let mut bytes = 0_u64;

    visit_files(root, cancellation, &mut |path, length| {
        bytes = add_bytes(path, bytes, length)?;

        Ok(())
    })?;

    Ok(bytes)
}

pub(super) fn add_bytes(path: &Path, current: u64, added: u64) -> Result<u64, StorageError> {
    current.checked_add(added).ok_or_else(|| {
        StorageError::io(
            path,
            StorageOperation::Inspect,
            std::io::ErrorKind::FileTooLarge.into(),
        )
    })
}

pub(super) fn visit_files(
    root: &Path,
    cancellation: &dyn Cancellation,
    visit: &mut impl FnMut(&Path, u64) -> Result<(), StorageError>,
) -> Result<(), StorageError> {
    let mut pending = vec![root.to_owned()];

    while let Some(path) = pending.pop() {
        check_cancelled(&path, cancellation)?;

        let metadata = match std::fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && path == root => {
                return Ok(());
            }
            Err(error) => return Err(StorageError::io(&path, StorageOperation::Inspect, error)),
        };

        if metadata.file_type().is_file() {
            visit(&path, metadata.len())?;
        } else if metadata.file_type().is_dir() {
            pending.extend(children(&path)?.into_iter().rev());
        } else {
            return Err(StorageError::new(&path, StorageErrorKind::UnsafePath));
        }
    }

    Ok(())
}

pub(crate) fn remove_owned_tree(
    root: &Path,
    cancellation: &dyn Cancellation,
) -> Result<(), StorageError> {
    let mut pending = vec![(root.to_owned(), false)];

    while let Some((path, visited)) = pending.pop() {
        check_cancelled(&path, cancellation)?;

        let metadata = match std::fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(StorageError::io(&path, StorageOperation::Inspect, error)),
        };

        if metadata.file_type().is_file() {
            std::fs::remove_file(&path)
                .map_err(|error| StorageError::io(&path, StorageOperation::Remove, error))?;
        } else if metadata.file_type().is_dir() {
            if visited {
                std::fs::remove_dir(&path)
                    .map_err(|error| StorageError::io(&path, StorageOperation::Remove, error))?;
            } else {
                let children = children(&path)?;

                pending.push((path, true));
                pending.extend(children.into_iter().rev().map(|path| (path, false)));
            }
        } else {
            return Err(StorageError::new(&path, StorageErrorKind::UnsafePath));
        }
    }

    Ok(())
}

pub(crate) fn children(path: &Path) -> Result<Vec<std::path::PathBuf>, StorageError> {
    let mut children = std::fs::read_dir(path)
        .map_err(|error| StorageError::io(path, StorageOperation::Read, error))?
        .map(|entry| {
            entry
                .map(|entry| entry.path())
                .map_err(|error| StorageError::io(path, StorageOperation::Read, error))
        })
        .collect::<Result<Vec<_>, _>>()?;

    children.sort();

    Ok(children)
}
