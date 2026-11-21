use std::fs::File;
use std::path::Path;

use super::transaction::storage_failure;
use crate::publication::operation::ArtifactPublicationFailure;
use crate::storage::{StorageError, StorageOperation, open_lock};

pub(super) struct ProductPublicationLock {
    _file: File,
}

pub(super) fn open_lock_file(metadata: &Path) -> Result<File, StorageError> {
    open_lock(&metadata.join("publication.lock"))
}

impl ProductPublicationLock {
    pub(super) fn acquire(
        metadata: &Path,
        planned: &crate::PlannedArtifact,
    ) -> Result<Self, ArtifactPublicationFailure> {
        let file = open_lock_file(metadata).map_err(|error| storage_failure(planned, error))?;

        file.lock().map_err(|error| {
            storage_failure(
                planned,
                StorageError::io(
                    &metadata.join("publication.lock"),
                    StorageOperation::Lock,
                    error,
                ),
            )
        })?;

        Ok(Self { _file: file })
    }
}
