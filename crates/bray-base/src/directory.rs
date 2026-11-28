use std::io;
use std::path::Path;
#[cfg(windows)]
use std::path::PathBuf;
use std::time::Duration;

const PERMISSION_RETRIES: usize = if cfg!(windows) { 100 } else { 0 };
const PERMISSION_RETRY_INTERVAL: Duration = Duration::from_millis(50);

/// Atomically renames one file or directory without replacing an existing destination.
pub fn atomic_rename_exclusive(source: &Path, destination: &Path) -> io::Result<()> {
    #[cfg(windows)]
    let (source, destination) = (
        windows_extended_path(source)?,
        windows_extended_path(destination)?,
    );

    retry_permission_denied(|| renamore::rename_exclusive(&source, &destination))
}

#[cfg(windows)]
fn windows_extended_path(path: &Path) -> io::Result<PathBuf> {
    use std::ffi::OsString;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};

    const SEPARATOR: u16 = b'\\' as u16;

    let absolute = std::path::absolute(path)?;
    let path: Vec<_> = absolute.as_os_str().encode_wide().collect();
    let verbatim = [SEPARATOR, SEPARATOR, b'?' as u16, SEPARATOR];

    if path.starts_with(&verbatim) {
        return Ok(absolute);
    }

    let mut extended = verbatim.to_vec();

    if path.starts_with(&[SEPARATOR, SEPARATOR]) {
        extended.extend("UNC\\".encode_utf16());
        extended.extend_from_slice(&path[2..]);
    } else {
        extended.extend(path);
    }

    Ok(OsString::from_wide(&extended).into())
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

#[cfg(test)]
mod tests {
    use std::io::ErrorKind;
    use std::time::Duration;

    use super::{atomic_rename_exclusive, retry_permission_denied_with_policy};

    #[test]
    fn directories_publish_atomically_without_replacing_existing_content() {
        let root = crate::test_support::temporary_directory();

        let source = root.path().join("private");
        let destination = root.path().join("published");

        std::fs::create_dir(&source)
            .unwrap_or_else(|error| panic!("private directory must be created: {error}"));

        std::fs::write(source.join("content"), b"original")
            .unwrap_or_else(|error| panic!("private content must be written: {error}"));

        atomic_rename_exclusive(&source, &destination)
            .unwrap_or_else(|error| panic!("directory must publish exclusively: {error}"));

        assert!(!source.exists());
        assert!(destination.is_dir());

        std::fs::create_dir(&source)
            .unwrap_or_else(|error| panic!("replacement directory must be created: {error}"));

        std::fs::write(source.join("content"), b"replacement")
            .unwrap_or_else(|error| panic!("replacement content must be written: {error}"));

        assert!(atomic_rename_exclusive(&source, &destination).is_err());

        assert_eq!(
            std::fs::read(destination.join("content"))
                .unwrap_or_else(|error| panic!("published content must remain readable: {error}")),
            b"original"
        );

        assert_eq!(
            std::fs::read(source.join("content")).unwrap_or_else(|error| panic!(
                "replacement content must remain readable: {error}"
            )),
            b"replacement"
        );
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
