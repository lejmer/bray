use std::collections::BTreeSet;
use std::path::Path;

use bray_base::Cancellation;

use super::error::{StorageError, StorageOperation};
use super::tree::{add_bytes, visit_files};

/// A report counts each file once in deterministic traversal order, without retaining open files.
#[derive(Default)]
pub(crate) struct StorageAccounting {
    seen: BTreeSet<file_id::FileId>,
}

impl StorageAccounting {
    pub(crate) fn measure(
        &mut self,
        root: &Path,
        cancellation: &dyn Cancellation,
    ) -> Result<(u64, u64), StorageError> {
        let mut bytes = 0;
        let mut shared = 0;

        visit_files(root, cancellation, &mut |path, length| {
            let identity = file_id::get_file_id(path)
                .map_err(|error| StorageError::io(path, StorageOperation::Inspect, error))?;

            bytes = add_bytes(path, bytes, length)?;

            if !self.seen.insert(identity) {
                shared = add_bytes(path, shared, length)?;
            }

            Ok(())
        })?;

        Ok((bytes, shared))
    }
}
