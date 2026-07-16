use std::io::{self, Write};
use std::path::Path;

use tempfile::{Builder, NamedTempFile, PathPersistError, TempPath};

use crate::ReplacementPolicy;

const STAGING_FILE_PREFIX: &str = ".bray-stage-";

pub(super) struct FilesystemStaging {
    file: NamedTempFile,
}

impl FilesystemStaging {
    pub(super) fn create(destination: &Path) -> io::Result<Self> {
        let directory = destination_directory(destination);
        let file = Builder::new()
            .prefix(STAGING_FILE_PREFIX)
            .tempfile_in(directory)?;

        Ok(Self { file })
    }

    pub(super) fn finish(mut self) -> io::Result<CompletedFilesystemStaging> {
        self.file.flush()?;

        Ok(CompletedFilesystemStaging {
            path: self.file.into_temp_path(),
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
        let result = match replacement {
            ReplacementPolicy::RequireAbsent => self.path.persist_noclobber(destination),
            ReplacementPolicy::ReplaceExisting => self.path.persist(destination),
        };

        result.map_err(promotion_error)
    }
}

fn destination_directory(destination: &Path) -> &Path {
    destination
        .parent()
        .filter(|directory| !directory.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

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
    use crate::ReplacementPolicy;

    #[test]
    fn staging_stays_private_until_atomic_replacement() {
        let Ok(directory) = tempfile::tempdir() else {
            panic!("test output directory must be created");
        };

        let destination = directory.path().join("application");

        if std::fs::write(&destination, b"existing").is_err() {
            panic!("test destination must be written");
        }

        let Ok(mut staging) = FilesystemStaging::create(&destination) else {
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

        let Ok(mut staging) = FilesystemStaging::create(&destination) else {
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
    fn dropped_staging_cleans_only_its_owned_path() {
        let Ok(directory) = tempfile::tempdir() else {
            panic!("test output directory must be created");
        };

        let unrelated = directory.path().join("unrelated");

        if std::fs::write(&unrelated, b"keep").is_err() {
            panic!("unrelated test file must be written");
        }

        let destination = directory.path().join("application");

        let Ok(staging) = FilesystemStaging::create(&destination) else {
            panic!("test staging file must be created");
        };

        let staging_path: PathBuf = staging.file.path().to_owned();

        drop(staging);

        assert!(!staging_path.exists());
        assert_eq!(file_bytes(&unrelated), b"keep");
    }

    fn file_bytes(path: &std::path::Path) -> Vec<u8> {
        let Ok(bytes) = std::fs::read(path) else {
            panic!("test file must be readable");
        };

        bytes
    }
}
