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

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    use super::open_lock_file;

    #[test]
    fn product_readers_wait_for_the_complete_publication_transaction() {
        let Ok(root) = tempfile::tempdir() else {
            panic!("test publication store must be created");
        };

        let writer = open_lock_file(root.path())
            .unwrap_or_else(|error| panic!("writer lock file must open: {error:?}"));

        writer
            .lock()
            .unwrap_or_else(|error| panic!("writer lock must be acquired: {error}"));

        let (sender, receiver) = mpsc::channel();

        let path = root.path().to_owned();

        let reader = std::thread::spawn(move || {
            let reader = open_lock_file(&path)
                .unwrap_or_else(|error| panic!("reader lock file must open: {error:?}"));

            reader
                .lock_shared()
                .unwrap_or_else(|error| panic!("reader lock must be acquired: {error}"));

            sender
                .send(())
                .unwrap_or_else(|_| panic!("reader completion must be observed"));
        });

        assert!(receiver.recv_timeout(Duration::from_millis(50)).is_err());
        drop(writer);

        receiver
            .recv_timeout(Duration::from_secs(2))
            .unwrap_or_else(|_| panic!("reader must continue after publication"));

        reader
            .join()
            .unwrap_or_else(|_| panic!("reader thread must finish"));
    }
}
