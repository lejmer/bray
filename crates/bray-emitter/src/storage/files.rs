use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use bray_base::{
    Cancellation, CompletedStagedFile, FileReplacementMode, StagedFile, sync_directory,
};

use super::error::{StorageError, StorageErrorKind, StorageOperation};

pub(crate) fn require_directory(path: &Path) -> Result<(), StorageError> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| StorageError::io(path, StorageOperation::Inspect, error))?;

    if !metadata.file_type().is_dir() {
        return Err(StorageError::new(path, StorageErrorKind::UnsafePath));
    }

    Ok(())
}

pub(crate) fn create_managed_path(root: &Path, relative: &Path) -> Result<PathBuf, StorageError> {
    require_directory(root)?;

    let mut path = root.to_owned();

    for component in relative.components() {
        let Component::Normal(component) = component else {
            return Err(StorageError::new(
                &root.join(relative),
                StorageErrorKind::UnsafePath,
            ));
        };

        let child = path.join(component);

        match std::fs::create_dir(&child) {
            Ok(()) => sync_directory(&path)
                .map_err(|error| StorageError::io(&path, StorageOperation::Flush, error))?,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                require_directory(&child)?;
            }
            Err(error) => return Err(StorageError::io(&child, StorageOperation::Create, error)),
        }

        path = child;
    }

    Ok(path)
}

pub(crate) fn managed_directory_exists(root: &Path, relative: &Path) -> Result<bool, StorageError> {
    match require_directory(root) {
        Ok(()) => {}
        Err(error)
            if matches!(
                error.kind(),
                StorageErrorKind::Io {
                    cause: std::io::ErrorKind::NotFound,
                    ..
                }
            ) =>
        {
            return Ok(false);
        }
        Err(error) => return Err(error),
    }

    let mut path = root.to_owned();

    for component in relative.components() {
        let Component::Normal(component) = component else {
            return Err(StorageError::new(
                &root.join(relative),
                StorageErrorKind::UnsafePath,
            ));
        };

        path.push(component);

        match require_directory(&path) {
            Ok(()) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    StorageErrorKind::Io {
                        cause: std::io::ErrorKind::NotFound,
                        ..
                    }
                ) =>
            {
                return Ok(false);
            }
            Err(error) => return Err(error),
        }
    }

    Ok(true)
}

pub(crate) fn read_owned_file(path: &Path) -> Result<Vec<u8>, StorageError> {
    require_file(path)?;

    std::fs::read(path).map_err(|error| StorageError::io(path, StorageOperation::Read, error))
}

pub(crate) fn require_file(path: &Path) -> Result<std::fs::Metadata, StorageError> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| StorageError::io(path, StorageOperation::Inspect, error))?;

    if !metadata.file_type().is_file() {
        return Err(StorageError::new(path, StorageErrorKind::UnsafePath));
    }

    Ok(metadata)
}

pub(crate) fn write_owned_json(
    path: &Path,
    value: &impl serde::Serialize,
) -> Result<(), StorageError> {
    let bytes = serde_json::to_vec(value).map_err(|error| StorageError::json(path, error))?;

    let mut staging = StagedFile::create(path, FileReplacementMode::ReplaceExisting, None)
        .map_err(|error| StorageError::io(path, StorageOperation::Create, error))?;

    staging
        .write_all(&bytes)
        .map_err(|error| StorageError::io(path, StorageOperation::Write, error))?;

    let staging = staging
        .finish()
        .map_err(|error| StorageError::io(path, StorageOperation::Flush, error))?;

    staging
        .promote(path)
        .map_err(|error| StorageError::io(path, StorageOperation::Rename, error))?;

    if let Some(parent) = path.parent() {
        sync_directory(parent)
            .map_err(|error| StorageError::io(parent, StorageOperation::Flush, error))?;
    }

    Ok(())
}

pub(crate) fn stage_owned_copy(
    source: &Path,
    staging: &Path,
    destination: &Path,
    replacement: FileReplacementMode,
    cancellation: &dyn Cancellation,
) -> Result<CompletedStagedFile, StorageError> {
    let permissions = require_file(source)?.permissions();

    let mut input = File::open(source)
        .map_err(|error| StorageError::io(source, StorageOperation::Read, error))?;

    let mut output = StagedFile::create_in(staging, destination, replacement, Some(permissions))
        .map_err(|error| StorageError::io(destination, StorageOperation::Create, error))?;

    crate::artifact::content::copy_reader(&mut input, &mut output, cancellation).map_err(
        |error| {
            use crate::artifact::content::ContentCopyError;

            match error {
                ContentCopyError::Cancelled => {
                    StorageError::new(destination, StorageErrorKind::Cancelled)
                }
                ContentCopyError::Read(kind) => {
                    StorageError::io(source, StorageOperation::Read, kind.into())
                }
                ContentCopyError::Write(kind) => {
                    StorageError::io(destination, StorageOperation::Write, kind.into())
                }
            }
        },
    )?;

    output
        .finish()
        .map_err(|error| StorageError::io(destination, StorageOperation::Flush, error))
}

pub(crate) fn open_lock(path: &Path) -> Result<File, StorageError> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => {}
        Ok(_) => return Err(StorageError::new(path, StorageErrorKind::UnsafePath)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(StorageError::io(path, StorageOperation::Inspect, error)),
    }

    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(|error| StorageError::io(path, StorageOperation::Create, error))
}
