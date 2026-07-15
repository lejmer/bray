use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMP_DIRECTORY: AtomicU64 = AtomicU64::new(0);

/// Temporary test file removed with its private containing directory on drop.
pub struct TemporaryFile {
    directory: PathBuf,
    path: PathBuf,
}

impl TemporaryFile {
    /// Creates one temporary file containing the supplied bytes.
    pub fn write(file_name: &str, bytes: &[u8]) -> Self {
        let directory = unique_temporary_directory();

        match std::fs::create_dir(&directory) {
            Ok(()) => {}
            Err(error) => panic!("temporary test directory should be created: {error:?}"),
        }

        let path = directory.join(file_name);

        match std::fs::write(&path, bytes) {
            Ok(()) => {}
            Err(error) => panic!("temporary test file should be written: {error:?}"),
        }

        Self { directory, path }
    }

    /// Returns the temporary file path.
    pub fn path(&self) -> &Path {
        self.path.as_path()
    }
}

impl Drop for TemporaryFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        let _ = std::fs::remove_dir(&self.directory);
    }
}

/// Returns an uncreated process-local unique path beneath the host temporary directory.
pub fn unique_temporary_directory() -> PathBuf {
    let sequence = NEXT_TEMP_DIRECTORY.fetch_add(1, Ordering::Relaxed);

    std::env::temp_dir().join(format!("bray-test-{}-{sequence}", std::process::id()))
}
