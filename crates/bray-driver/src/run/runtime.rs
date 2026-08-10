use std::env;
use std::path::PathBuf;

use bray_compilation::SelectedTarget;
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticArtifactDigest, DiagnosticArtifactDigestAlgorithm,
    DiagnosticBag, DiagnosticId, DiagnosticIoErrorKind, DiagnosticKind,
    DiagnosticRuntimeAbiVersion, DiagnosticRuntimeArtifactProblem, SeverityKind,
};
use bray_runtime_interface::{
    RuntimeArtifact, RuntimeArtifactBuildError, RuntimeArtifactMetadataDecodeError,
    RuntimeArtifactSelectionError,
};
use bray_tooling::{RuntimeArtifactLoadError, load_runtime_artifact};

use crate::command::{DriverRuntimeProfile, DriverRuntimeSelection};

pub(super) fn resolve_runtime(
    selection: Option<&DriverRuntimeSelection>,
    target: &SelectedTarget,
) -> Result<Option<RuntimeArtifact>, DiagnosticBag> {
    let Some(selection) = selection else {
        return Ok(None);
    };

    let metadata = match selection {
        DriverRuntimeSelection::Artifact(path) => path.clone(),
        DriverRuntimeSelection::Profile(profile) => {
            let Some(path) = runtime_profile_metadata(target, profile) else {
                return Err(DiagnosticBag::single(unsupported_product_diagnostic()));
            };

            path
        }
    };

    load_runtime_artifact(
        &metadata,
        target.profile().identity(),
        target.runtime_abi(),
    )
    .map(Some)
    .map_err(runtime_load_diagnostics)
}

pub(super) fn runtime_selection_diagnostics(
    error: &RuntimeArtifactSelectionError,
) -> Option<DiagnosticBag> {
    let diagnostic = match error {
        RuntimeArtifactSelectionError::UnreadableArchive { path, kind, .. } => Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::RuntimeArtifactArchiveReadFailed,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::artifact_path(path))
        .with_arg(DiagnosticArg::io_error_kind(DiagnosticIoErrorKind::from(
            *kind,
        ))),
        RuntimeArtifactSelectionError::InvalidArchive { path, .. } => Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::RuntimeArtifactArchiveInvalid,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::artifact_path(path)),
        RuntimeArtifactSelectionError::ArchiveDigestMismatch {
            path,
            expected,
            actual,
            ..
        } => Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::RuntimeArtifactArchiveDigestMismatch,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::artifact_path(path))
        .with_arg(DiagnosticArg::expected_artifact_digest(runtime_digest(
            *expected,
        )))
        .with_arg(DiagnosticArg::actual_artifact_digest(runtime_digest(
            *actual,
        ))),
        _ => return None,
    };

    Some(DiagnosticBag::single(diagnostic))
}

fn runtime_load_diagnostics(error: RuntimeArtifactLoadError) -> DiagnosticBag {
    let diagnostic = match error {
        RuntimeArtifactLoadError::MetadataRead { path, kind } => Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::RuntimeArtifactMetadataReadFailed,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::artifact_path(path))
        .with_arg(DiagnosticArg::io_error_kind(DiagnosticIoErrorKind::from(
            kind,
        ))),
        RuntimeArtifactLoadError::InvalidMetadata { path, source } => Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::RuntimeArtifactMetadataInvalid,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::artifact_path(path))
        .with_arg(DiagnosticArg::runtime_artifact_problem(
            metadata_problem(&source),
        )),
        RuntimeArtifactLoadError::InvalidArtifact { path, source } => Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::RuntimeArtifactMetadataInvalid,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::artifact_path(path))
        .with_arg(DiagnosticArg::runtime_artifact_problem(build_problem(
            source,
        ))),
        RuntimeArtifactLoadError::IncompatibleTarget {
            path,
            expected,
            actual,
        } => Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::RuntimeArtifactTargetMismatch,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::artifact_path(path))
        .with_arg(DiagnosticArg::expected_target_identity(expected.as_str()))
        .with_arg(DiagnosticArg::actual_target_identity(actual.as_str())),
        RuntimeArtifactLoadError::IncompatibleRuntimeAbi {
            path,
            expected,
            actual,
        } => Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::RuntimeArtifactAbiMismatch,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::artifact_path(path))
        .with_arg(DiagnosticArg::expected_runtime_abi(runtime_abi(expected)))
        .with_arg(DiagnosticArg::actual_runtime_abi(runtime_abi(actual))),
    };

    DiagnosticBag::single(diagnostic)
}

const fn metadata_problem(
    error: &RuntimeArtifactMetadataDecodeError,
) -> DiagnosticRuntimeArtifactProblem {
    match error {
        RuntimeArtifactMetadataDecodeError::SizeLimitExceeded => {
            DiagnosticRuntimeArtifactProblem::MetadataSizeLimitExceeded
        }
        RuntimeArtifactMetadataDecodeError::Malformed => {
            DiagnosticRuntimeArtifactProblem::MalformedMetadata
        }
        RuntimeArtifactMetadataDecodeError::UnsupportedFormat => {
            DiagnosticRuntimeArtifactProblem::UnsupportedFormat
        }
        RuntimeArtifactMetadataDecodeError::InvalidRuntimeIdentity => {
            DiagnosticRuntimeArtifactProblem::InvalidRuntimeIdentity
        }
        RuntimeArtifactMetadataDecodeError::InvalidArtifactIdentity => {
            DiagnosticRuntimeArtifactProblem::InvalidArtifactIdentity
        }
        RuntimeArtifactMetadataDecodeError::InvalidTarget => {
            DiagnosticRuntimeArtifactProblem::InvalidTarget
        }
        RuntimeArtifactMetadataDecodeError::InvalidPanicAbi => {
            DiagnosticRuntimeArtifactProblem::InvalidPanicAbi
        }
        RuntimeArtifactMetadataDecodeError::UnknownCapability => {
            DiagnosticRuntimeArtifactProblem::UnknownCapability
        }
        RuntimeArtifactMetadataDecodeError::UnknownRole => {
            DiagnosticRuntimeArtifactProblem::UnknownRole
        }
        RuntimeArtifactMetadataDecodeError::InvalidRoleSymbol => {
            DiagnosticRuntimeArtifactProblem::InvalidRoleSymbol
        }
        RuntimeArtifactMetadataDecodeError::UnknownRoleImplementation => {
            DiagnosticRuntimeArtifactProblem::UnknownRoleImplementation
        }
        RuntimeArtifactMetadataDecodeError::InvalidNativeLinkName => {
            DiagnosticRuntimeArtifactProblem::InvalidNativeLinkName
        }
        RuntimeArtifactMetadataDecodeError::UnknownNativeLinkKind => {
            DiagnosticRuntimeArtifactProblem::UnknownNativeLinkKind
        }
        RuntimeArtifactMetadataDecodeError::UnknownComponentPurpose => {
            DiagnosticRuntimeArtifactProblem::UnknownComponentPurpose
        }
        RuntimeArtifactMetadataDecodeError::InvalidComponentIdentity => {
            DiagnosticRuntimeArtifactProblem::InvalidComponentIdentity
        }
        RuntimeArtifactMetadataDecodeError::InvalidArchiveDigest => {
            DiagnosticRuntimeArtifactProblem::InvalidArchiveDigest
        }
        RuntimeArtifactMetadataDecodeError::InvalidContract(_) => {
            DiagnosticRuntimeArtifactProblem::InvalidContract
        }
        RuntimeArtifactMetadataDecodeError::InvalidMetadata(_) => {
            DiagnosticRuntimeArtifactProblem::InvalidCatalog
        }
    }
}

const fn build_problem(error: RuntimeArtifactBuildError) -> DiagnosticRuntimeArtifactProblem {
    match error {
        RuntimeArtifactBuildError::MissingComponent => {
            DiagnosticRuntimeArtifactProblem::MissingComponent
        }
        RuntimeArtifactBuildError::UnexpectedComponent => {
            DiagnosticRuntimeArtifactProblem::UnexpectedComponent
        }
        RuntimeArtifactBuildError::ArchiveFileNameMismatch => {
            DiagnosticRuntimeArtifactProblem::ArchiveFileNameMismatch
        }
    }
}

fn runtime_profile_metadata(
    target: &SelectedTarget,
    profile: &DriverRuntimeProfile,
) -> Option<PathBuf> {
    let executable = env::current_exe().ok()?;

    executable
        .ancestors()
        .map(|ancestor| {
            ancestor
                .join("runtimes")
                .join(target.profile().identity().as_str())
                .join(profile.as_str())
                .join("bray-runtime.brayrt")
        })
        .find(|path| path.is_file())
}

fn runtime_abi(version: bray_runtime_interface::RuntimeAbiVersion) -> DiagnosticRuntimeAbiVersion {
    DiagnosticRuntimeAbiVersion::new(version.major(), version.minor())
}

fn runtime_digest(digest: bray_runtime_interface::RuntimeArtifactDigest) -> DiagnosticArtifactDigest {
    DiagnosticArtifactDigest::new(DiagnosticArtifactDigestAlgorithm::Sha256, digest.bytes())
}

fn unsupported_product_diagnostic() -> Diagnostic {
    Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::RequestUnsupportedProductEmission,
        SeverityKind::Error,
    )
}

#[cfg(test)]
mod tests {
    use bray_compilation::SelectedTarget;
    use bray_diagnostics::{
        DiagnosticArg, DiagnosticIoErrorKind, DiagnosticKind, DiagnosticRuntimeArtifactProblem,
    };
    use bray_runtime_interface::{RuntimeArtifactBuildError, RuntimeArtifactMetadataDecodeError};
    use bray_tooling::RuntimeArtifactLoadError;

    use super::{resolve_runtime, runtime_load_diagnostics};
    use crate::command::DriverRuntimeSelection;

    #[test]
    fn explicit_missing_runtime_preserves_path_and_io_category() {
        let directory = bray_testing::unique_temporary_directory();
        let metadata = directory.join("missing-runtime.brayrt");
        let selection = DriverRuntimeSelection::Artifact(metadata.clone());

        let diagnostics = match resolve_runtime(Some(&selection), &SelectedTarget::baseline()) {
            Ok(_) => panic!("missing runtime metadata must fail"),
            Err(diagnostics) => diagnostics,
        };

        assert_eq!(
            diagnostics
                .by_kind(DiagnosticKind::RuntimeArtifactMetadataReadFailed)
                .count(),
            1
        );

        assert_eq!(
            diagnostics
                .by_kind(DiagnosticKind::RequestUnsupportedProductEmission)
                .count(),
            0
        );

        let diagnostic = diagnostics
            .by_kind(DiagnosticKind::RuntimeArtifactMetadataReadFailed)
            .next()
            .unwrap_or_else(|| panic!("missing runtime diagnostic must exist"));

        assert!(
            diagnostic
                .args()
                .contains(&DiagnosticArg::artifact_path(metadata))
        );

        assert!(diagnostic.args().contains(&DiagnosticArg::io_error_kind(
            DiagnosticIoErrorKind::NotFound
        )));
    }

    #[test]
    fn invalid_runtime_metadata_preserves_typed_decode_and_build_failures() {
        let path = std::path::PathBuf::from("runtime.brayrt");

        let decode_cases = [
            (
                RuntimeArtifactMetadataDecodeError::Malformed,
                DiagnosticRuntimeArtifactProblem::MalformedMetadata,
            ),
            (
                RuntimeArtifactMetadataDecodeError::UnsupportedFormat,
                DiagnosticRuntimeArtifactProblem::UnsupportedFormat,
            ),
            (
                RuntimeArtifactMetadataDecodeError::UnknownCapability,
                DiagnosticRuntimeArtifactProblem::UnknownCapability,
            ),
            (
                RuntimeArtifactMetadataDecodeError::InvalidArchiveDigest,
                DiagnosticRuntimeArtifactProblem::InvalidArchiveDigest,
            ),
        ];

        for (source, expected) in decode_cases {
            let diagnostics = runtime_load_diagnostics(
                RuntimeArtifactLoadError::InvalidMetadata {
                    path: path.clone(),
                    source,
                },
            );

            assert_runtime_problem(&diagnostics, expected);
        }

        let build_cases = [
            (
                RuntimeArtifactBuildError::MissingComponent,
                DiagnosticRuntimeArtifactProblem::MissingComponent,
            ),
            (
                RuntimeArtifactBuildError::UnexpectedComponent,
                DiagnosticRuntimeArtifactProblem::UnexpectedComponent,
            ),
            (
                RuntimeArtifactBuildError::ArchiveFileNameMismatch,
                DiagnosticRuntimeArtifactProblem::ArchiveFileNameMismatch,
            ),
        ];

        for (source, expected) in build_cases {
            let diagnostics = runtime_load_diagnostics(
                RuntimeArtifactLoadError::InvalidArtifact {
                    path: path.clone(),
                    source,
                },
            );

            assert_runtime_problem(&diagnostics, expected);
        }
    }

    fn assert_runtime_problem(
        diagnostics: &bray_diagnostics::DiagnosticBag,
        expected: DiagnosticRuntimeArtifactProblem,
    ) {
        let diagnostic = diagnostics
            .by_kind(DiagnosticKind::RuntimeArtifactMetadataInvalid)
            .next()
            .unwrap_or_else(|| panic!("invalid runtime metadata diagnostic must exist"));

        assert!(
            diagnostic
                .args()
                .contains(&DiagnosticArg::runtime_artifact_problem(expected))
        );
    }
}
