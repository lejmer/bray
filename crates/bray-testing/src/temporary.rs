use std::path::{Path, PathBuf};

use tempfile::TempDir;

/// Creates a private test directory beneath `target/test-tmp` in the repository.
///
/// Tests must keep every file they write inside the repository. The directory and its contents are removed when the
/// returned value is dropped.
pub fn temporary_directory() -> TempDir {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("bray-testing must live two levels beneath the repository root")
        .join("target/test-tmp");

    std::fs::create_dir_all(&parent).unwrap_or_else(|error| {
        panic!(
            "test temporary root {} should be created: {error:?}",
            parent.display()
        )
    });

    tempfile::tempdir_in(&parent).unwrap_or_else(|error| {
        panic!(
            "temporary test directory should be created in {}: {error:?}",
            parent.display()
        )
    })
}

/// Temporary test file removed with its private containing directory on drop.
pub struct TemporaryFile {
    _directory: TempDir,
    path: PathBuf,
}

impl TemporaryFile {
    /// Creates one temporary file containing the supplied bytes.
    pub fn write(file_name: &str, bytes: &[u8]) -> Self {
        let directory = temporary_directory();
        let path = directory.path().join(file_name);

        std::fs::write(&path, bytes).unwrap_or_else(|error| {
            panic!(
                "temporary test file {} should be written: {error:?}",
                path.display()
            )
        });

        Self {
            _directory: directory,
            path,
        }
    }

    /// Returns the temporary file path.
    pub fn path(&self) -> &Path {
        self.path.as_path()
    }
}
