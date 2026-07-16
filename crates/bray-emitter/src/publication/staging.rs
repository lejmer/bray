use std::fs::Permissions;
use std::io::{self, Write};
use std::path::Path;

use tempfile::{Builder, NamedTempFile, PathPersistError, TempPath};

use crate::{ArtifactKind, ReplacementPolicy};

const STAGING_FILE_PREFIX: &str = ".bray-stage-";

pub(super) struct FilesystemStaging {
    file: NamedTempFile,
    final_permissions: Option<Permissions>,
}

impl FilesystemStaging {
    pub(super) fn create(
        destination: &Path,
        kind: ArtifactKind,
        replacement: ReplacementPolicy,
    ) -> io::Result<Self> {
        let final_permissions = replacement_permissions(destination, replacement)?;
        let directory = destination_directory(destination);
        let mut builder = Builder::new();

        builder.prefix(STAGING_FILE_PREFIX);
        configure_default_permissions(&mut builder, kind);

        let file = builder.tempfile_in(directory)?;

        Ok(Self {
            file,
            final_permissions,
        })
    }

    pub(super) fn finish(mut self) -> io::Result<CompletedFilesystemStaging> {
        self.file.flush()?;

        Ok(CompletedFilesystemStaging {
            path: self.file.into_temp_path(),
            final_permissions: self.final_permissions,
        })
    }
}

impl Write for FilesystemStaging {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.file.write(buffer)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

pub(super) struct CompletedFilesystemStaging {
    path: TempPath,
    final_permissions: Option<Permissions>,
}

impl CompletedFilesystemStaging {
    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    pub(super) fn promote(
        self,
        destination: &Path,
        replacement: ReplacementPolicy,
    ) -> io::Result<()> {
        let Self {
            path,
            final_permissions,
        } = self;

        if let Some(permissions) = final_permissions {
            std::fs::set_permissions(&path, permissions)?;
        }

        match replacement {
            ReplacementPolicy::RequireAbsent => promote_exclusive(path, destination),
            ReplacementPolicy::ReplaceExisting => {
                path.persist(destination).map_err(promotion_error)
            }
        }
    }
}

fn promote_exclusive(path: TempPath, destination: &Path) -> io::Result<()> {
    let result = renamore::rename_exclusive(&path, destination);

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
    replacement: ReplacementPolicy,
) -> io::Result<Option<Permissions>> {
    if replacement == ReplacementPolicy::ReplaceExisting {
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

#[cfg(unix)]
fn configure_default_permissions(builder: &mut Builder<'_, '_>, kind: ArtifactKind) {
    use std::os::unix::fs::PermissionsExt;

    let mode = match kind {
        ArtifactKind::Executable | ArtifactKind::ExecutableModule => 0o755,
        ArtifactKind::Assembly
        | ArtifactKind::BackendIr
        | ArtifactKind::BackendBitcode
        | ArtifactKind::RelocatableObject
        | ArtifactKind::DebugCompanion
        | ArtifactKind::PackageInterface
        | ArtifactKind::DependencyMetadata
        | ArtifactKind::StaticLibrary
        | ArtifactKind::SharedLibrary
        | ArtifactKind::LinkedCompanion => 0o644,
    };

    builder.permissions(Permissions::from_mode(mode));
}

#[cfg(not(unix))]
fn configure_default_permissions(_: &mut Builder<'_, '_>, _: ArtifactKind) {}

fn promotion_error(error: PathPersistError) -> io::Error {
    let PathPersistError { error, path } = error;

    drop(path);

    error
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::path::PathBuf;

    use super::FilesystemStaging;
    use crate::{ArtifactKind, ReplacementPolicy};

    #[test]
    fn staging_stays_private_until_atomic_replacement() {
        let Ok(directory) = tempfile::tempdir() else {
            panic!("test output directory must be created");
        };

        let destination = directory.path().join("application");

        if std::fs::write(&destination, b"existing").is_err() {
            panic!("test destination must be written");
        }

        let Ok(mut staging) = FilesystemStaging::create(
            &destination,
            ArtifactKind::PackageInterface,
            ReplacementPolicy::ReplaceExisting,
        ) else {
            panic!("test staging file must be created");
        };

        if staging.write_all(b"replacement").is_err() {
            panic!("test staging content must be written");
        }

        let Ok(staging) = staging.finish() else {
            panic!("test staging file must finish");
        };

        assert_eq!(file_bytes(&destination), b"existing");
        assert_eq!(staging.path().parent(), Some(directory.path()));

        if staging
            .promote(&destination, ReplacementPolicy::ReplaceExisting)
            .is_err()
        {
            panic!("test staging file must replace its destination");
        }

        assert_eq!(file_bytes(&destination), b"replacement");
    }

    #[test]
    fn failed_promotion_preserves_destination_and_cleans_staging() {
        let Ok(directory) = tempfile::tempdir() else {
            panic!("test output directory must be created");
        };

        let destination = directory.path().join("application");

        if std::fs::write(&destination, b"existing").is_err() {
            panic!("test destination must be written");
        }

        let Ok(mut staging) = FilesystemStaging::create(
            &destination,
            ArtifactKind::PackageInterface,
            ReplacementPolicy::RequireAbsent,
        ) else {
            panic!("test staging file must be created");
        };

        if staging.write_all(b"replacement").is_err() {
            panic!("test staging content must be written");
        }

        let Ok(staging) = staging.finish() else {
            panic!("test staging file must finish");
        };

        let staging_path = staging.path().to_owned();

        let result = staging.promote(&destination, ReplacementPolicy::RequireAbsent);

        assert!(result.is_err());
        assert_eq!(file_bytes(&destination), b"existing");
        assert!(!staging_path.exists());
    }

    #[test]
    fn exclusive_promotion_publishes_once_and_cleans_staging() {
        let Ok(directory) = tempfile::tempdir() else {
            panic!("test output directory must be created");
        };

        let destination = directory.path().join("application");

        let Ok(mut staging) = FilesystemStaging::create(
            &destination,
            ArtifactKind::PackageInterface,
            ReplacementPolicy::RequireAbsent,
        ) else {
            panic!("test staging file must be created");
        };

        if staging.write_all(b"published").is_err() {
            panic!("test staging content must be written");
        }

        let Ok(staging) = staging.finish() else {
            panic!("test staging file must finish");
        };

        let staging_path = staging.path().to_owned();

        if staging
            .promote(&destination, ReplacementPolicy::RequireAbsent)
            .is_err()
        {
            panic!("test staging file must be promoted exclusively");
        }

        assert_eq!(file_bytes(&destination), b"published");
        assert!(!staging_path.exists());
    }

    #[test]
    fn dropped_staging_cleans_only_its_owned_path() {
        let Ok(directory) = tempfile::tempdir() else {
            panic!("test output directory must be created");
        };

        let unrelated = directory.path().join("unrelated");

        if std::fs::write(&unrelated, b"keep").is_err() {
            panic!("unrelated test file must be written");
        }

        let destination = directory.path().join("application");

        let Ok(staging) = FilesystemStaging::create(
            &destination,
            ArtifactKind::PackageInterface,
            ReplacementPolicy::ReplaceExisting,
        ) else {
            panic!("test staging file must be created");
        };

        let staging_path: PathBuf = staging.file.path().to_owned();

        drop(staging);

        assert!(!staging_path.exists());
        assert_eq!(file_bytes(&unrelated), b"keep");
    }

    #[cfg(unix)]
    #[test]
    fn executable_staging_applies_executable_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let Ok(directory) = tempfile::tempdir() else {
            panic!("test output directory must be created");
        };

        let destination = directory.path().join("application");

        let Ok(mut staging) = FilesystemStaging::create(
            &destination,
            ArtifactKind::Executable,
            ReplacementPolicy::RequireAbsent,
        ) else {
            panic!("test staging file must be created");
        };

        if staging.write_all(b"executable").is_err() {
            panic!("test staging content must be written");
        }

        let Ok(staging) = staging.finish() else {
            panic!("test staging file must finish");
        };

        if staging
            .promote(&destination, ReplacementPolicy::RequireAbsent)
            .is_err()
        {
            panic!("test executable must be promoted");
        }

        let Ok(metadata) = std::fs::metadata(&destination) else {
            panic!("test executable metadata must be readable");
        };

        assert_ne!(metadata.permissions().mode() & 0o100, 0);
    }

    #[cfg(unix)]
    #[test]
    fn replacement_preserves_existing_file_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let Ok(directory) = tempfile::tempdir() else {
            panic!("test output directory must be created");
        };

        let destination = directory.path().join("application");

        if std::fs::write(&destination, b"existing").is_err() {
            panic!("test destination must be written");
        }

        if std::fs::set_permissions(&destination, std::fs::Permissions::from_mode(0o750)).is_err() {
            panic!("test destination permissions must be configured");
        }

        let Ok(mut staging) = FilesystemStaging::create(
            &destination,
            ArtifactKind::Executable,
            ReplacementPolicy::ReplaceExisting,
        ) else {
            panic!("test staging file must be created");
        };

        if staging.write_all(b"replacement").is_err() {
            panic!("test staging content must be written");
        }

        let Ok(staging) = staging.finish() else {
            panic!("test staging file must finish");
        };

        if staging
            .promote(&destination, ReplacementPolicy::ReplaceExisting)
            .is_err()
        {
            panic!("test executable must replace its destination");
        }

        let Ok(metadata) = std::fs::metadata(&destination) else {
            panic!("test executable metadata must be readable");
        };

        assert_eq!(metadata.permissions().mode() & 0o777, 0o750);
    }

    fn file_bytes(path: &std::path::Path) -> Vec<u8> {
        let Ok(bytes) = std::fs::read(path) else {
            panic!("test file must be readable");
        };

        bytes
    }
}
