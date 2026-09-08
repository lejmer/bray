use std::fs::Permissions;
use std::io::{self, Write};
use std::path::Path;

use tempfile::{Builder, NamedTempFile, TempPath};

const STAGING_FILE_PREFIX: &str = ".bray-stage-";

/// Writes complete file contents through atomic replacement, preserving existing file permissions.
///
/// Existing readers retain their original file. A failed write leaves the destination unchanged.
pub fn write_file_atomically(destination: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut staging = StagedFile::create(destination, FileReplacementMode::ReplaceExisting, None)?;

    staging.write_all(bytes)?;

    staging.finish()?.promote(destination)
}

/// Returns whether a filename belongs to [`StagedFile`] private storage.
pub fn is_staged_file_name(name: &str) -> bool {
    name.starts_with(STAGING_FILE_PREFIX)
}

/// Publication behavior for a staged filesystem file.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FileReplacementMode {
    /// Publish only when the destination does not exist.
    RequireAbsent,
    /// Atomically replace an existing destination.
    ReplaceExisting,
}

/// A private file being prepared for atomic publication.
pub struct StagedFile {
    file: NamedTempFile,
    final_permissions: Option<Permissions>,
    replacement: FileReplacementMode,
}

impl StagedFile {
    /// Creates private staging next to `destination`.
    ///
    /// Existing regular-file permissions are preserved for replacement. The
    /// supplied permissions apply only when no existing regular file provides
    /// permissions to preserve.
    pub fn create(
        destination: &Path,
        replacement: FileReplacementMode,
        default_permissions: Option<Permissions>,
    ) -> io::Result<Self> {
        Self::create_in(
            destination_directory(destination),
            destination,
            replacement,
            default_permissions,
        )
    }

    /// Creates private staging in `staging_directory` for `destination`.
    ///
    /// The staging directory and destination must be on the same filesystem so
    /// that promotion remains atomic. Existing regular-file permissions are
    /// preserved for replacement. The supplied permissions apply only when no
    /// existing regular file provides permissions to preserve.
    pub fn create_in(
        staging_directory: &Path,
        destination: &Path,
        replacement: FileReplacementMode,
        default_permissions: Option<Permissions>,
    ) -> io::Result<Self> {
        let final_permissions = replacement_permissions(destination, replacement)?;

        let mut builder = Builder::new();

        builder.prefix(STAGING_FILE_PREFIX);

        if let Some(permissions) = default_permissions {
            builder.permissions(permissions);
        }

        let file = builder.tempfile_in(staging_directory)?;

        Ok(Self {
            file,
            final_permissions,
            replacement,
        })
    }

    /// Returns the private staging path.
    pub fn path(&self) -> &Path {
        self.file.path()
    }

    /// Flushes staged bytes and completes preparation for publication.
    pub fn finish(mut self) -> io::Result<CompletedStagedFile> {
        self.file.flush()?;
        self.file.as_file().sync_all()?;

        Ok(CompletedStagedFile {
            path: self.file.into_temp_path(),
            final_permissions: self.final_permissions,
            replacement: self.replacement,
        })
    }
}

impl Write for StagedFile {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.file.write(buffer)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

/// A flushed staged file ready for publication.
pub struct CompletedStagedFile {
    path: TempPath,
    final_permissions: Option<Permissions>,
    replacement: FileReplacementMode,
}

impl CompletedStagedFile {
    /// Returns the private staging path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Publishes the staged file according to its replacement mode.
    pub fn promote(self, destination: &Path) -> io::Result<()> {
        let Self {
            path,
            final_permissions,
            replacement,
        } = self;

        if let Some(permissions) = final_permissions {
            std::fs::set_permissions(&path, permissions)?;
        }

        match replacement {
            FileReplacementMode::RequireAbsent => promote_exclusive(path, destination),
            FileReplacementMode::ReplaceExisting => promote_replacing(path, destination),
        }
    }
}

fn promote_exclusive(path: TempPath, destination: &Path) -> io::Result<()> {
    let result = crate::atomic_rename_exclusive(&path, destination);

    drop(path);

    result
}

fn promote_replacing(path: TempPath, destination: &Path) -> io::Result<()> {
    let result = crate::retry_permission_denied(|| std::fs::rename(&path, destination));

    drop(path);

    result
}

fn destination_directory(destination: &Path) -> &Path {
    destination
        .parent()
        .filter(|directory| !directory.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

fn replacement_permissions(
    destination: &Path,
    replacement: FileReplacementMode,
) -> io::Result<Option<Permissions>> {
    if replacement == FileReplacementMode::ReplaceExisting {
        match std::fs::symlink_metadata(destination) {
            Ok(metadata) if metadata.file_type().is_file() => {
                return Ok(Some(metadata.permissions()));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::path::{Path, PathBuf};

    use super::{FileReplacementMode, StagedFile};

    #[test]
    fn whole_file_replacement_preserves_open_readers_and_failed_destinations() {
        let directory = tempfile::tempdir().expect("test directory must be created");
        let destination = directory.path().join("source.rs");

        super::write_file_atomically(&destination, b"original").unwrap();

        let mut reader = std::fs::File::open(&destination).unwrap();

        super::write_file_atomically(&destination, b"replacement").unwrap();

        let mut original = Vec::new();

        reader.read_to_end(&mut original).unwrap();

        assert_eq!(original, b"original");
        assert_eq!(file_bytes(&destination), b"replacement");
        assert!(super::write_file_atomically(directory.path(), b"invalid").is_err());
        assert!(directory.path().is_dir());
        assert_eq!(file_bytes(&destination), b"replacement");
    }

    #[test]
    fn staging_stays_private_until_atomic_replacement() {
        let Ok(directory) = tempfile::tempdir() else {
            panic!("test output directory must be created");
        };

        let destination = directory.path().join("source.bray");

        write(&destination, b"existing");

        let mut staging = stage(&destination, FileReplacementMode::ReplaceExisting);
        write_staging(&mut staging, b"replacement");

        let staging = finish(staging);

        assert_eq!(file_bytes(&destination), b"existing");
        assert_eq!(staging.path().parent(), Some(directory.path()));

        if staging.promote(&destination).is_err() {
            panic!("test staging file must replace its destination");
        }

        assert_eq!(file_bytes(&destination), b"replacement");
    }

    #[test]
    fn failed_promotion_preserves_destination_and_cleans_staging() {
        let Ok(directory) = tempfile::tempdir() else {
            panic!("test output directory must be created");
        };

        let destination = directory.path().join("source.bray");

        write(&destination, b"existing");

        let mut staging = stage(&destination, FileReplacementMode::RequireAbsent);
        write_staging(&mut staging, b"replacement");

        let staging = finish(staging);
        let staging_path = staging.path().to_owned();
        let result = staging.promote(&destination);

        assert!(result.is_err());
        assert_eq!(file_bytes(&destination), b"existing");
        assert!(!staging_path.exists());
    }

    #[test]
    fn staging_directory_can_be_private_and_separate_from_destination() {
        let Ok(directory) = tempfile::tempdir() else {
            panic!("test output directory must be created");
        };

        let staging_directory = directory.path().join(".bray").join("staging");
        let public_directory = directory.path().join("native").join("debug");
        let destination = public_directory.join("application");

        if std::fs::create_dir_all(&staging_directory).is_err()
            || std::fs::create_dir_all(&public_directory).is_err()
        {
            panic!("test publication directories must be created");
        }

        let mut staging = StagedFile::create_in(
            &staging_directory,
            &destination,
            FileReplacementMode::ReplaceExisting,
            None,
        )
        .unwrap_or_else(|error| panic!("test staging file must be created: {error:?}"));

        write_staging(&mut staging, b"published");

        let staging = finish(staging);

        assert_eq!(staging.path().parent(), Some(staging_directory.as_path()));
        assert!(!destination.exists());

        if staging.promote(&destination).is_err() {
            panic!("test staging file must move to its public destination");
        }

        assert_eq!(file_bytes(&destination), b"published");
    }

    #[cfg(windows)]
    #[test]
    fn promotions_support_windows_paths_beyond_the_legacy_limit() {
        let Ok(directory) = tempfile::tempdir() else {
            panic!("test output directory must be created");
        };

        let staging_directory = directory
            .path()
            .join("private")
            .join("a".repeat(120))
            .join("b".repeat(120));

        std::fs::create_dir_all(&staging_directory)
            .unwrap_or_else(|error| panic!("long staging directory must be created: {error}"));

        assert!(staging_directory.as_os_str().len() > 260);

        for (name, replacement) in [
            ("exclusive", FileReplacementMode::RequireAbsent),
            ("replacing", FileReplacementMode::ReplaceExisting),
        ] {
            let destination = directory.path().join(name);

            if replacement == FileReplacementMode::ReplaceExisting {
                write(&destination, b"existing");
            }

            let mut staging =
                StagedFile::create_in(&staging_directory, &destination, replacement, None)
                    .unwrap_or_else(|error| panic!("long staging file must be created: {error}"));

            write_staging(&mut staging, b"published");

            finish(staging)
                .promote(&destination)
                .unwrap_or_else(|error| panic!("long staging file must be promoted: {error}"));

            assert_eq!(file_bytes(&destination), b"published");
        }
    }

    #[test]
    fn dropped_staging_cleans_only_its_owned_path() {
        let Ok(directory) = tempfile::tempdir() else {
            panic!("test output directory must be created");
        };

        let unrelated = directory.path().join("unrelated");
        let destination = directory.path().join("source.bray");

        write(&unrelated, b"keep");

        let staging = stage(&destination, FileReplacementMode::ReplaceExisting);
        let staging_path: PathBuf = staging.path().to_owned();

        drop(staging);

        assert!(!staging_path.exists());
        assert_eq!(file_bytes(&unrelated), b"keep");
    }

    #[cfg(unix)]
    #[test]
    fn replacement_preserves_existing_file_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let Ok(directory) = tempfile::tempdir() else {
            panic!("test output directory must be created");
        };

        let destination = directory.path().join("source.bray");

        write(&destination, b"existing");

        if std::fs::set_permissions(&destination, std::fs::Permissions::from_mode(0o750)).is_err() {
            panic!("test destination permissions must be configured");
        }

        let mut staging = stage(&destination, FileReplacementMode::ReplaceExisting);
        write_staging(&mut staging, b"replacement");

        if finish(staging).promote(&destination).is_err() {
            panic!("test staging file must replace its destination");
        }

        let Ok(metadata) = std::fs::metadata(&destination) else {
            panic!("test destination metadata must be readable");
        };

        assert_eq!(metadata.permissions().mode() & 0o777, 0o750);
    }

    fn stage(destination: &Path, replacement: FileReplacementMode) -> StagedFile {
        match StagedFile::create(destination, replacement, None) {
            Ok(staging) => staging,
            Err(error) => panic!("test staging file must be created: {error:?}"),
        }
    }

    fn write_staging(staging: &mut StagedFile, bytes: &[u8]) {
        if staging.write_all(bytes).is_err() {
            panic!("test staging content must be written");
        }
    }

    fn finish(staging: StagedFile) -> super::CompletedStagedFile {
        match staging.finish() {
            Ok(staging) => staging,
            Err(error) => panic!("test staging file must finish: {error:?}"),
        }
    }

    fn write(path: &Path, bytes: &[u8]) {
        if std::fs::write(path, bytes).is_err() {
            panic!("test file must be written");
        }
    }

    fn file_bytes(path: &Path) -> Vec<u8> {
        match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) => panic!("test file must be readable: {error:?}"),
        }
    }
}
