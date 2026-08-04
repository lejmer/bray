use std::fmt;
use std::path::{Path, PathBuf};

use bray_emitter::ArtifactKind;
use bray_target::TargetIdentity;

#[derive(Debug)]
pub(in crate::standard_library) enum BuildError {
    Usage,
    UnexpectedArgument(String),
    MissingValue(&'static str),
    MissingOutput,
    Workspace(String),
    OutputExists(PathBuf),
    PublicationInProgress(PathBuf),
    Project(String),
    Source(String),
    TemporaryDirectory(std::io::Error),
    UnsupportedTarget(TargetIdentity),
    CompilerUnavailable,
    LinkerUnavailable(TargetIdentity),
    CompilationFailed {
        target: TargetIdentity,
        detail: String,
    },
    EmissionRequest(String),
    Emission(String),
    MissingEmittedArtifact(ArtifactKind),
    TargetDependentInterface(TargetIdentity),
    InvalidArtifactPath(PathBuf),
    InvalidTarget(String),
    InvalidIdentity,
    MissingProduct,
    MissingInterface,
    NonReproducibleManifest,
    NonReproducibleArtifact(String),
    Conformance {
        check: &'static str,
        detail: String,
    },
    Manifest(String),
    Io {
        action: &'static str,
        path: PathBuf,
        destination: Option<PathBuf>,
        source: std::io::Error,
    },
}

impl BuildError {
    pub(in crate::standard_library) fn conformance(
        check: &'static str,
        detail: impl Into<String>,
    ) -> Self {
        Self::Conformance {
            check,
            detail: detail.into(),
        }
    }

    pub(in crate::standard_library) fn read(path: &Path, source: std::io::Error) -> Self {
        Self::Io {
            action: "read",
            path: path.to_path_buf(),
            destination: None,
            source,
        }
    }

    pub(in crate::standard_library) fn write(path: &Path, source: std::io::Error) -> Self {
        Self::Io {
            action: "write",
            path: path.to_path_buf(),
            destination: None,
            source,
        }
    }

    pub(super) fn publish(path: &Path, destination: &Path, source: std::io::Error) -> Self {
        Self::Io {
            action: "publish",
            path: path.to_path_buf(),
            destination: Some(destination.to_path_buf()),
            source,
        }
    }
}

impl fmt::Display for BuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage => formatter.write_str("invalid standard-library command"),
            Self::UnexpectedArgument(argument) => {
                write!(formatter, "unexpected argument: {argument}")
            }
            Self::MissingValue(option) => write!(formatter, "missing value for {option}"),
            Self::MissingOutput => formatter.write_str("missing --output"),
            Self::Workspace(error) => formatter.write_str(error),
            Self::OutputExists(path) => {
                write!(formatter, "output already exists: {}", path.display())
            }
            Self::PublicationInProgress(path) => {
                write!(
                    formatter,
                    "standard library publication is already in progress: {}",
                    path.display()
                )
            }
            Self::Project(error) => {
                write!(formatter, "standard library project is invalid: {error}")
            }
            Self::Source(error) => {
                write!(
                    formatter,
                    "standard library source could not be read: {error}"
                )
            }
            Self::TemporaryDirectory(error) => {
                write!(formatter, "could not create staging directory: {error}")
            }
            Self::UnsupportedTarget(target) => {
                write!(
                    formatter,
                    "unsupported standard library target: {}",
                    target.as_str()
                )
            }
            Self::CompilerUnavailable => {
                formatter.write_str("LLVM compiler backend is unavailable")
            }
            Self::LinkerUnavailable(target) => {
                write!(formatter, "archiver is unavailable for {}", target.as_str())
            }
            Self::CompilationFailed { target, detail } => {
                write!(
                    formatter,
                    "standard library compilation failed for {}: {detail}",
                    target.as_str(),
                )
            }
            Self::EmissionRequest(error) => {
                write!(
                    formatter,
                    "standard library emission request is invalid: {error}"
                )
            }
            Self::Emission(error) => {
                write!(formatter, "standard library emission failed: {error}")
            }
            Self::MissingEmittedArtifact(kind) => {
                write!(formatter, "standard library emission omitted {kind:?}")
            }
            Self::TargetDependentInterface(target) => {
                write!(
                    formatter,
                    "package interface differs for target {}",
                    target.as_str()
                )
            }
            Self::InvalidArtifactPath(path) => {
                write!(formatter, "artifact path is invalid: {}", path.display())
            }
            Self::InvalidTarget(target) => {
                write!(formatter, "unsupported standard library target: {target}")
            }
            Self::InvalidIdentity => {
                formatter.write_str("standard library identity contract is invalid")
            }
            Self::MissingProduct => {
                formatter.write_str("standard library product std:library is missing")
            }
            Self::MissingInterface => formatter
                .write_str("standard library has no target from which to build its interface"),
            Self::NonReproducibleManifest => {
                formatter.write_str("repeated standard library builds produced different manifests")
            }
            Self::NonReproducibleArtifact(path) => {
                write!(
                    formatter,
                    "repeated standard library builds produced different bytes for {path}"
                )
            }
            Self::Conformance { check, detail } => {
                write!(
                    formatter,
                    "standard library {check} conformance failed: {detail}"
                )
            }
            Self::Manifest(error) => {
                write!(formatter, "standard library manifest is invalid: {error}")
            }
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
