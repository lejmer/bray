use std::fs::File;
#[cfg(windows)]
use std::fs::OpenOptions;
use std::io;
use std::path::Path;
use std::time::Duration;

const PERMISSION_RETRIES: usize = if cfg!(windows) { 100 } else { 0 };
const PERMISSION_RETRY_INTERVAL: Duration = Duration::from_millis(50);

/// Atomically renames one file or directory without replacing an existing destination.
pub fn atomic_rename_exclusive(source: &Path, destination: &Path) -> io::Result<()> {
    retry_permission_denied(|| renamore::rename_exclusive(source, destination))
}

/// Retries a filesystem operation when the host temporarily denies access.
pub fn retry_permission_denied<T>(mut operation: impl FnMut() -> io::Result<T>) -> io::Result<T> {
    retry_permission_denied_with_policy(
        &mut operation,
        PERMISSION_RETRIES,
        PERMISSION_RETRY_INTERVAL,
    )
}

fn retry_permission_denied_with_policy<T>(
    operation: &mut impl FnMut() -> io::Result<T>,
    retries: usize,
    interval: Duration,
) -> io::Result<T> {
    for _ in 0..retries {
        match operation() {
            Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
                std::thread::sleep(interval);
            }
            result => return result,
        }
    }

    operation()
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
    use std::io::ErrorKind;
    use std::time::Duration;

    use super::{
        atomic_rename_exclusive, atomic_rename_exclusive_is_supported,
        retry_permission_denied_with_policy, sync_directory,
    };

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

    #[test]
    fn filesystem_operations_retry_short_lived_permission_denials() {
        let mut attempts = 0_u8;

        let value = retry_permission_denied_with_policy(
            &mut || {
                attempts = attempts.saturating_add(1);

                if attempts < 3 {
                    return Err(std::io::Error::from(ErrorKind::PermissionDenied));
                }

                Ok(17_u8)
            },
            4,
            Duration::ZERO,
        )
        .unwrap_or_else(|error| panic!("transient permission denial must be retried: {error}"));

        assert_eq!(value, 17);
        assert_eq!(attempts, 3);
    }
}
