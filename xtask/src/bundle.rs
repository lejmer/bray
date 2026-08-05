use std::ffi::OsString;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use bray_target::{NativeTarget, TargetIdentity};

#[derive(Debug)]
pub(crate) struct NativeBuildOptions {
    output: PathBuf,
    target: Option<NativeTarget>,
}

impl NativeBuildOptions {
    pub(crate) fn output(&self) -> &Path {
        &self.output
    }

    pub(crate) const fn target(&self) -> Option<NativeTarget> {
        self.target
    }

    pub(crate) fn target_output(&self, target: NativeTarget) -> PathBuf {
        self.output.join(target.as_str())
    }
}

#[derive(Default)]
pub(crate) struct NativeBuildOptionsBuilder {
    output: Option<PathBuf>,
    target: Option<NativeTarget>,
}

impl NativeBuildOptionsBuilder {
    pub(crate) fn parse_option(
        &mut self,
        argument: &str,
        arguments: &mut impl Iterator<Item = String>,
    ) -> Result<bool, NativeBuildOptionsError> {
        match argument {
            "--output" if self.output.is_none() => {
                self.output = Some(PathBuf::from(required_value(arguments, "--output")?));

                Ok(true)
            }
            "--target" if self.target.is_none() => {
                let value = required_value(arguments, "--target")?;

                self.target = TargetIdentity::try_new(value.as_str())
                    .and_then(|identity| NativeTarget::for_identity(&identity))
                    .map(Some)
                    .ok_or(NativeBuildOptionsError::InvalidTarget(value))?;

                Ok(true)
            }
            "--output" | "--target" => Err(NativeBuildOptionsError::DuplicateOption(
                argument.to_owned(),
            )),
            _ => Ok(false),
        }
    }

    pub(crate) fn finish(self) -> Result<NativeBuildOptions, NativeBuildOptionsError> {
        let output = self.output.ok_or(NativeBuildOptionsError::MissingOutput)?;

        if output.as_os_str().is_empty() {
            return Err(NativeBuildOptionsError::EmptyOutput);
        }

        Ok(NativeBuildOptions {
            output,
            target: self.target,
        })
    }
}

#[derive(Debug)]
pub(crate) enum NativeBuildOptionsError {
    MissingValue(&'static str),
    MissingOutput,
    EmptyOutput,
    InvalidTarget(String),
    DuplicateOption(String),
}

impl fmt::Display for NativeBuildOptionsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingValue(option) => write!(formatter, "missing value for {option}"),
            Self::MissingOutput => formatter.write_str("missing --output"),
            Self::EmptyOutput => formatter.write_str("--output cannot be empty"),
            Self::InvalidTarget(target) => write!(formatter, "unsupported native target: {target}"),
            Self::DuplicateOption(option) => write!(formatter, "duplicate option: {option}"),
        }
    }
}

pub(crate) struct DirectoryPublication {
    contents: PathBuf,
    destination: PathBuf,
    staging: tempfile::TempDir,
    work: PathBuf,
    _lock: PublicationLock,
}

impl DirectoryPublication {
    pub(crate) fn begin(
        destination: &Path,
        prefix: &str,
    ) -> Result<Self, DirectoryPublicationError> {
        let parent = destination.parent().unwrap_or_else(|| Path::new("."));

        fs::create_dir_all(parent)
            .map_err(|error| DirectoryPublicationError::write(parent, error))?;

        let lock = PublicationLock::acquire(destination)?;

        let staging = tempfile::Builder::new()
            .prefix(prefix)
            .tempdir_in(parent)
            .map_err(DirectoryPublicationError::TemporaryDirectory)?;

        let contents = staging.path().join("contents");

        fs::create_dir(&contents)
            .map_err(|error| DirectoryPublicationError::write(&contents, error))?;

        let work = staging.path().join("work");

        fs::create_dir(&work).map_err(|error| DirectoryPublicationError::write(&work, error))?;

        Ok(Self {
            contents,
            destination: destination.to_path_buf(),
            staging,
            work,
            _lock: lock,
        })
    }

    pub(crate) fn contents(&self) -> &Path {
        &self.contents
    }

    pub(crate) fn work(&self) -> &Path {
        &self.work
    }

    pub(crate) fn publish(self) -> Result<PathBuf, DirectoryPublicationError> {
        let previous = self.staging.path().join("previous");
        let replaces_existing = self.destination.exists();

        if replaces_existing {
            fs::rename(&self.destination, &previous).map_err(|error| {
                DirectoryPublicationError::publish(&self.destination, &previous, error)
            })?;
        }

        if let Err(error) = fs::rename(&self.contents, &self.destination) {
            let publication =
                DirectoryPublicationError::publish(&self.contents, &self.destination, error);

            if replaces_existing && let Err(error) = fs::rename(&previous, &self.destination) {
                let rollback =
                    DirectoryPublicationError::publish(&previous, &self.destination, error);

                let preserved_staging = self.staging.keep();

                return Err(DirectoryPublicationError::Rollback {
                    publication: Box::new(publication),
                    rollback: Box::new(rollback),
                    preserved_staging,
                });
            }

            return Err(publication);
        }

        Ok(self.destination)
    }
}

#[derive(Debug)]
pub(crate) enum DirectoryPublicationError {
    InProgress(PathBuf),
    TemporaryDirectory(std::io::Error),
    Rollback {
        publication: Box<Self>,
        rollback: Box<Self>,
        preserved_staging: PathBuf,
    },
    Io {
        action: &'static str,
        path: PathBuf,
        destination: Option<PathBuf>,
        source: std::io::Error,
    },
}

impl DirectoryPublicationError {
    fn write(path: &Path, source: std::io::Error) -> Self {
        Self::Io {
            action: "write",
            path: path.to_path_buf(),
            destination: None,
            source,
        }
    }

    fn publish(path: &Path, destination: &Path, source: std::io::Error) -> Self {
        Self::Io {
            action: "publish",
            path: path.to_path_buf(),
            destination: Some(destination.to_path_buf()),
            source,
        }
    }
}

impl fmt::Display for DirectoryPublicationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InProgress(path) => {
                write!(
                    formatter,
                    "directory publication is already in progress: {}",
                    path.display()
                )
            }
            Self::TemporaryDirectory(error) => {
                write!(formatter, "could not create staging directory: {error}")
            }
            Self::Rollback {
                publication,
                rollback,
                preserved_staging,
            } => write!(
                formatter,
                "{publication}; restoring the previous directory also failed: {rollback}; the previous directory is preserved under {}",
                preserved_staging.display()
            ),
            Self::Io {
                action,
                path,
                destination,
                source,
            } => match destination {
                Some(destination) => write!(
                    formatter,
                    "failed to {action} {} to {}: {source}",
                    path.display(),
                    destination.display()
                ),
                None => write!(formatter, "failed to {action} {}: {source}", path.display()),
            },
        }
    }
}

struct PublicationLock {
    path: PathBuf,
}

impl PublicationLock {
    fn acquire(destination: &Path) -> Result<Self, DirectoryPublicationError> {
        let file_name = destination.file_name().ok_or_else(|| {
            DirectoryPublicationError::write(
                destination,
                std::io::Error::new(ErrorKind::InvalidInput, "destination has no file name"),
            )
        })?;

        let mut lock_name = OsString::from(file_name);

        lock_name.push(".lock");

        let path = destination.with_file_name(lock_name);

        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(_) => Ok(Self { path }),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                Err(DirectoryPublicationError::InProgress(path))
            }
            Err(error) => Err(DirectoryPublicationError::write(&path, error)),
        }
    }
}

impl Drop for PublicationLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn required_value(
    arguments: &mut impl Iterator<Item = String>,
    option: &'static str,
) -> Result<String, NativeBuildOptionsError> {
    arguments
        .next()
        .ok_or(NativeBuildOptionsError::MissingValue(option))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use bray_target::NativeTarget;

    use super::{DirectoryPublication, NativeBuildOptionsBuilder, NativeBuildOptionsError};

    #[test]
    fn native_build_options_require_an_output() {
        let result = NativeBuildOptionsBuilder::default().finish();

        assert!(matches!(
            result,
            Err(NativeBuildOptionsError::MissingOutput)
        ));
    }

    #[test]
    fn native_build_options_accept_an_optional_target() {
        let mut options = NativeBuildOptionsBuilder::default();

        let mut arguments = [
            "output".to_owned(),
            "--target".to_owned(),
            "x86_64-pc-windows-msvc".to_owned(),
        ]
        .into_iter();

        let output = arguments
            .next()
            .unwrap_or_else(|| panic!("output option value must exist"));

        assert!(
            options
                .parse_option("--output", &mut std::iter::once(output))
                .unwrap_or_else(|error| panic!("output option must parse: {error}"))
        );

        let target_option = arguments
            .next()
            .unwrap_or_else(|| panic!("target option must exist"));

        assert!(
            options
                .parse_option(&target_option, &mut arguments)
                .unwrap_or_else(|error| panic!("target option must parse: {error}"))
        );

        let options = options
            .finish()
            .unwrap_or_else(|error| panic!("native build options must be complete: {error}"));

        assert_eq!(options.output(), PathBuf::from("output"));
        assert_eq!(options.target(), Some(NativeTarget::X86_64WindowsMsvc));

        assert_eq!(
            options.target_output(NativeTarget::X86_64WindowsMsvc),
            PathBuf::from("output").join("x86_64-pc-windows-msvc")
        );
    }

    #[test]
    fn directory_publication_replaces_complete_existing_contents() {
        let directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("temporary root must exist: {error}"));

        let output = directory.path().join("bundle");

        fs::create_dir(&output)
            .unwrap_or_else(|error| panic!("existing output must be created: {error}"));

        fs::write(output.join("previous"), b"previous")
            .unwrap_or_else(|error| panic!("existing artifact must be written: {error}"));

        let publication = DirectoryPublication::begin(&output, "bray-publication-test-")
            .unwrap_or_else(|error| panic!("publication must begin: {error}"));

        fs::write(publication.contents().join("current"), b"current")
            .unwrap_or_else(|error| panic!("staged artifact must be written: {error}"));

        publication
            .publish()
            .unwrap_or_else(|error| panic!("publication must replace existing output: {error}"));

        assert!(!output.join("previous").exists());

        assert_eq!(
            fs::read(output.join("current"))
                .unwrap_or_else(|error| panic!("published artifact must be readable: {error}")),
            b"current"
        );
    }
}
