use std::fs::Permissions;
use std::io::{self, Write};
use std::path::Path;

use bray_base::{CompletedStagedFile, FileReplacementMode, StagedFile};

use crate::{ArtifactKind, ReplacementPolicy};

pub(super) struct FilesystemStaging {
    file: StagedFile,
}

impl FilesystemStaging {
    pub(super) fn create(
        destination: &Path,
        kind: ArtifactKind,
        replacement: ReplacementPolicy,
    ) -> io::Result<Self> {
        let file = StagedFile::create(
            destination,
            replacement_mode(replacement),
            default_permissions(kind),
        )?;

        Ok(Self { file })
    }

    #[cfg(test)]
    pub(super) fn path(&self) -> &Path {
        self.file.path()
    }

    pub(super) fn finish(self) -> io::Result<CompletedFilesystemStaging> {
        Ok(CompletedFilesystemStaging {
            file: self.file.finish()?,
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
    file: CompletedStagedFile,
}

impl CompletedFilesystemStaging {
    pub(super) fn path(&self) -> &Path {
        self.file.path()
    }

    pub(super) fn promote(self, destination: &Path) -> io::Result<()> {
        self.file.promote(destination)
    }
}

fn replacement_mode(replacement: ReplacementPolicy) -> FileReplacementMode {
    match replacement {
        ReplacementPolicy::RequireAbsent => FileReplacementMode::RequireAbsent,
        ReplacementPolicy::ReplaceExisting => FileReplacementMode::ReplaceExisting,
    }
}

#[cfg(unix)]
fn default_permissions(kind: ArtifactKind) -> Option<Permissions> {
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

    Some(Permissions::from_mode(mode))
}

#[cfg(not(unix))]
fn default_permissions(_: ArtifactKind) -> Option<Permissions> {
    None
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

        let result = staging.promote(&destination);

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

        if staging.promote(&destination).is_err() {
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

        let staging_path: PathBuf = staging.path().to_owned();

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

        if staging.promote(&destination).is_err() {
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

        if staging.promote(&destination).is_err() {
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
