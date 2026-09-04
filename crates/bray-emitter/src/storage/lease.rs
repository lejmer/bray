use std::fs::{File, TryLockError};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::error::{StorageError, StorageOperation};
use super::files::open_lock;

/// Shared ownership of a managed entry. The OS releases abandoned ownership on process exit.
#[derive(Clone, Debug)]
pub(crate) struct StorageLease {
    path: PathBuf,
    _file: Arc<File>,
}

impl StorageLease {
    /// The caller must hold the owning coordination lock until this lease is acquired.
    pub(crate) fn acquire(path: &Path) -> Result<Self, StorageError> {
        let file = open_lock(path)?;

        file.lock_shared()
            .map_err(|error| StorageError::io(path, StorageOperation::Lock, error))?;

        Ok(Self {
            path: path.to_owned(),
            _file: Arc::new(file),
        })
    }

    /// The caller keeps the coordination lock until removal finishes or the entry is retired.
    pub(crate) fn try_exclusive(path: &Path) -> Result<Option<File>, StorageError> {
        let file = open_lock(path)?;

        match file.try_lock() {
            Ok(()) => Ok(Some(file)),
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(TryLockError::Error(error)) => {
                Err(StorageError::io(path, StorageOperation::Lock, error))
            }
        }
    }
}

impl PartialEq for StorageLease {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path
    }
}

impl Eq for StorageLease {}
