use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMP_DIRECTORY: AtomicU64 = AtomicU64::new(0);

pub(crate) fn package_identity() -> bray_symbols::PackageIdentity {
    match bray_symbols::PackageIdentity::try_new("test.package") {
        Some(identity) => identity,
        None => panic!("test package identity must be valid"),
    }
}

pub(crate) struct TemporaryFile {
    directory: PathBuf,
    path: PathBuf,
}

impl TemporaryFile {
    pub(crate) fn write(file_name: &str, bytes: &[u8]) -> Self {
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

    pub(crate) fn path(&self) -> &Path {
        self.path.as_path()
    }
}

impl Drop for TemporaryFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        let _ = std::fs::remove_dir(&self.directory);
    }
}

pub(crate) fn unique_temporary_directory() -> PathBuf {
    let sequence = NEXT_TEMP_DIRECTORY.fetch_add(1, Ordering::Relaxed);

    std::env::temp_dir().join(format!(
        "bray-driver-test-{}-{sequence}",
        std::process::id()
    ))
}
