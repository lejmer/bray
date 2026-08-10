use std::fs::File;
#[cfg(windows)]
use std::fs::OpenOptions;
use std::io;
use std::path::Path;

/// Atomically renames one file or directory without replacing an existing destination.
pub fn atomic_rename_exclusive(source: &Path, destination: &Path) -> io::Result<()> {
    renamore::rename_exclusive(source, destination)
}

/// Returns whether the filesystem can atomically rename without replacement.
pub fn atomic_rename_exclusive_is_supported(path: &Path) -> io::Result<bool> {
    renamore::rename_exclusive_is_atomic(path)
}

/// Flushes directory-entry changes to the filesystem's durable storage boundary.
#[cfg(unix)]
pub fn sync_directory(path: &Path) -> io::Result<()> {
    File::open(path)?.sync_all()
}

/// Flushes directory-entry changes to the filesystem's durable storage boundary.
#[cfg(windows)]
pub fn sync_directory(path: &Path) -> io::Result<()> {
    open_directory(path)?.sync_all()
}

#[cfg(windows)]
fn open_directory(path: &Path) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;

    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;

    OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)
}

#[cfg(not(any(unix, windows)))]
pub fn sync_directory(_: &Path) -> io::Result<()> {
    Err(io::Error::from(io::ErrorKind::Unsupported))
}

#[cfg(test)]
mod tests {
    use super::{atomic_rename_exclusive, atomic_rename_exclusive_is_supported, sync_directory};

    #[test]
    fn directories_support_durable_exclusive_publication() {
        let Ok(root) = tempfile::tempdir() else {
            panic!("test output directory must be created");
        };

        let source = root.path().join("private");
        let destination = root.path().join("published");

        std::fs::create_dir(&source)
            .unwrap_or_else(|error| panic!("private directory must be created: {error}"));

        assert!(matches!(
            atomic_rename_exclusive_is_supported(root.path()),
            Ok(true)
        ));

        atomic_rename_exclusive(&source, &destination)
            .unwrap_or_else(|error| panic!("directory must publish exclusively: {error}"));

        sync_directory(root.path())
            .unwrap_or_else(|error| panic!("published directory must become durable: {error}"));

        assert!(!source.exists());
        assert!(destination.is_dir());
    }
}
