use std::fmt;
use std::path::{Path, PathBuf};

use bray_emitter::ArtifactKind;
use bray_target::TargetIdentity;
use bray_tooling::{OutputFormat, write_diagnostics};

use crate::bundle::{DirectoryPublicationError, NativeBuildOptionsError};

#[derive(Debug)]
pub(in crate::standard_library) enum BuildError {
    Usage,
    UnexpectedArgument(String),
    MissingValue(&'static str),
    MissingRequiredOption {
        option: &'static str,
        required_by: &'static str,
    },
    InvalidProfileMode(String),
    BuildOptions(NativeBuildOptionsError),
    Workspace(String),
    InputIdentity(String),
    Publication(DirectoryPublicationError),
    Project(String),
    Source(String),
    OsBindings(String),
    NativeArchive(String),
    TemporaryDirectory(std::io::Error),
    UnsupportedTarget(TargetIdentity),
    CompilerUnavailable(bray_tooling::LlvmCompilationLoadError),
    LinkerUnavailable {
        target: TargetIdentity,
        detail: String,
    },
    CompilationFailed {
        target: TargetIdentity,
        detail: String,
    },
    EmissionRequest(String),
    Emission(String),
    MissingEmittedArtifact(ArtifactKind),
    MissingCompilerProfile(TargetIdentity),
    CompilerProfile(String),
    InvalidArtifactPath(PathBuf),
    InvalidIdentity,
    MissingProduct,
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
    pub(in crate::standard_library) fn compilation_failed(
        target: TargetIdentity,
        internal: String,
        diagnostics: &bray_diagnostics::DiagnosticBag,
        sources: &bray_source::SourceStore,
    ) -> Self {
        Self::CompilationFailed {
            target,
            detail: diagnostic_failure_detail(internal, diagnostics, sources),
        }
    }

    pub(in crate::standard_library) fn emission(
        internal: String,
        diagnostics: &bray_diagnostics::DiagnosticBag,
        sources: &bray_source::SourceStore,
    ) -> Self {
        Self::Emission(diagnostic_failure_detail(internal, diagnostics, sources))
    }

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
}

fn diagnostic_failure_detail(
    internal: String,
    diagnostics: &bray_diagnostics::DiagnosticBag,
    sources: &bray_source::SourceStore,
) -> String {
    if diagnostics.is_empty() {
        return internal;
    }

    let mut standard_output = Vec::new();
    let mut standard_error = Vec::new();

    if write_diagnostics(
        diagnostics,
        Some(sources),
        OutputFormat::Text,
        &mut standard_output,
        &mut standard_error,
    )
    .is_err()
    {
        return internal;
    }

    let Ok(rendered) = String::from_utf8(standard_error) else {
        return internal;
    };

    if rendered.is_empty() {
        return internal;
    }

    format!("\n{}", rendered.trim_end())
}

impl fmt::Display for BuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage => formatter.write_str("invalid standard-library command"),
            Self::UnexpectedArgument(argument) => {
                write!(formatter, "unexpected argument: {argument}")
            }
            Self::MissingValue(option) => write!(formatter, "missing value for {option}"),
            Self::MissingRequiredOption {
                option,
                required_by,
            } => write!(formatter, "{option} is required with {required_by}"),
            Self::InvalidProfileMode(mode) => write!(
                formatter,
                "invalid compiler profile mode '{mode}', expected 'summary' or 'trace'"
            ),
            Self::BuildOptions(error) => write!(formatter, "{error}"),
            Self::Workspace(error) => formatter.write_str(error),
            Self::InputIdentity(error) => formatter.write_str(error),
            Self::Publication(error) => write!(formatter, "{error}"),
            Self::Project(error) => {
                write!(formatter, "standard library project is invalid: {error}")
            }
            Self::Source(error) => {
                write!(
                    formatter,
                    "standard library source could not be read: {error}"
                )
            }
            Self::OsBindings(error) => {
                write!(
                    formatter,
                    "standard library OS bindings are invalid: {error}"
                )
            }
            Self::NativeArchive(error) => {
                write!(
                    formatter,
                    "platform ABI archive could not be built: {error}"
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
            Self::CompilerUnavailable(error) => {
                write!(formatter, "LLVM compiler backend is unavailable: {error}")
            }
            Self::LinkerUnavailable { target, detail } => {
                write!(
                    formatter,
                    "archiver is unavailable for {}: {detail}",
                    target.as_str()
                )
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
            Self::MissingCompilerProfile(target) => write!(
                formatter,
                "standard library compilation produced no compiler profile for {}",
                target.as_str()
            ),
            Self::CompilerProfile(error) => {
                write!(
                    formatter,
                    "standard library compiler profile failed: {error}"
                )
            }
            Self::InvalidArtifactPath(path) => {
                write!(formatter, "artifact path is invalid: {}", path.display())
            }
            Self::InvalidIdentity => {
                formatter.write_str("standard library identity contract is invalid")
            }
            Self::MissingProduct => {
                formatter.write_str("standard library product std:library is missing")
            }
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

#[cfg(test)]
mod tests {
    use bray_diagnostics::{
        Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticKind, DiagnosticNote,
        DiagnosticNoteKind, DiagnosticSourceInput, DiagnosticSourceInputOrigin, SeverityKind,
    };
    use bray_source::{SourceInputKind, SourceStore, TextSize};

    use super::diagnostic_failure_detail;

    #[test]
    fn diagnostic_failure_details_render_structured_diagnostics() {
        let diagnostic = Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::SourceInvalidUtf8,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::source_input(DiagnosticSourceInput::new(
            0,
            SourceInputKind::File,
            DiagnosticSourceInputOrigin::File("main.bray".into()),
        )))
        .with_arg(DiagnosticArg::text_offset(TextSize::new(4)))
        .with_note(DiagnosticNote::new(DiagnosticNoteKind::SourceMustBeUtf8));

        let rendered = diagnostic_failure_detail(
            "internal diagnostic bag".to_owned(),
            &DiagnosticBag::single(diagnostic),
            &SourceStore::new(),
        );

        assert!(rendered.contains("error E1002"));
        assert!(!rendered.contains("internal diagnostic bag"));
    }
}
