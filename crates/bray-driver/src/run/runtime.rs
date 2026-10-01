use std::env;
use std::path::PathBuf;

use bray_compilation::SelectedTarget;
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticIoErrorKind, DiagnosticKind,
    DiagnosticNote, DiagnosticNoteKind, DiagnosticRuntimeAbiVersion,
    DiagnosticRuntimeArtifactProblem, DiagnosticRuntimeArtifactPurpose, SeverityKind,
};
use bray_runtime_interface::{
    RuntimeArtifact, RuntimeArtifactBuildError, RuntimeArtifactMetadataBuildError,
    RuntimeArtifactMetadataDecodeError, RuntimeArtifactPurpose, RuntimeContractBuildError,
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

    load_runtime_artifact(&metadata, target.profile().identity(), target.runtime_abi())
        .map(Some)
        .map_err(runtime_load_diagnostics)
}

fn runtime_load_diagnostics(error: RuntimeArtifactLoadError) -> DiagnosticBag {
    let diagnostic = match error {
        RuntimeArtifactLoadError::MetadataRead { path, kind } => {
            { runtime_diagnostic(DiagnosticKind::RuntimeArtifactMetadataReadFailed) }
                .with_arg(DiagnosticArg::artifact_path(path))
                .with_arg(DiagnosticArg::io_error_kind(DiagnosticIoErrorKind::from(
                    kind,
                )))
        }
        RuntimeArtifactLoadError::InvalidMetadata { path, source } => {
            { runtime_diagnostic(DiagnosticKind::RuntimeArtifactMetadataInvalid) }
                .with_arg(DiagnosticArg::artifact_path(path))
                .with_arg(DiagnosticArg::runtime_artifact_problem(metadata_problem(
                    &source,
                )))
        }
        RuntimeArtifactLoadError::InvalidArtifact { path, source } => {
            { runtime_diagnostic(DiagnosticKind::RuntimeArtifactMetadataInvalid) }
                .with_arg(DiagnosticArg::artifact_path(path))
                .with_arg(DiagnosticArg::runtime_artifact_problem(build_problem(
                    source,
                )))
        }
        RuntimeArtifactLoadError::NativeIndex { path, source } => {
            runtime_diagnostic(DiagnosticKind::RuntimeArtifactMetadataInvalid)
                .with_arg(DiagnosticArg::artifact_path(path))
                .with_arg(DiagnosticArg::runtime_artifact_problem(
                    DiagnosticRuntimeArtifactProblem::InvalidNativeArtifact(
                        bray_compilation::diagnostic_native_artifact_cause(&source),
                    ),
                ))
        }
        RuntimeArtifactLoadError::IncompatibleTarget {
            path,
            expected,
            actual,
        } => runtime_diagnostic(DiagnosticKind::RuntimeArtifactTargetMismatch)
            .with_arg(DiagnosticArg::artifact_path(path))
            .with_arg(DiagnosticArg::expected_target_identity(expected.as_str()))
            .with_arg(DiagnosticArg::actual_target_identity(actual.as_str())),
        RuntimeArtifactLoadError::IncompatibleRuntimeAbi {
            path,
            expected,
            actual,
        } => runtime_diagnostic(DiagnosticKind::RuntimeArtifactAbiMismatch)
            .with_arg(DiagnosticArg::artifact_path(path))
            .with_arg(DiagnosticArg::expected_runtime_abi(runtime_abi(expected)))
            .with_arg(DiagnosticArg::actual_runtime_abi(runtime_abi(actual))),
    };

    DiagnosticBag::single(diagnostic)
}

fn runtime_diagnostic(kind: DiagnosticKind) -> Diagnostic {
    Diagnostic::new(DiagnosticId::new(0), kind, SeverityKind::Error).with_note(DiagnosticNote::new(
        DiagnosticNoteKind::RuntimeArtifactMustBeUsable,
    ))
}

fn metadata_problem(
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
        RuntimeArtifactMetadataDecodeError::UnknownPlatformService => {
            DiagnosticRuntimeArtifactProblem::UnknownPlatformService
        }
        RuntimeArtifactMetadataDecodeError::InvalidRoleSymbol => {
            DiagnosticRuntimeArtifactProblem::InvalidRoleSymbol
        }
        RuntimeArtifactMetadataDecodeError::UnknownRoleImplementation => {
            DiagnosticRuntimeArtifactProblem::UnknownRoleImplementation
        }
        RuntimeArtifactMetadataDecodeError::UnknownComponentPurpose => {
            DiagnosticRuntimeArtifactProblem::UnknownComponentPurpose
        }
        RuntimeArtifactMetadataDecodeError::InvalidComponentIdentity => {
            DiagnosticRuntimeArtifactProblem::InvalidComponentIdentity
        }
        RuntimeArtifactMetadataDecodeError::InvalidNativeIndexDigest => {
            DiagnosticRuntimeArtifactProblem::InvalidNativeIndexDigest
        }
        RuntimeArtifactMetadataDecodeError::InvalidContract(error) => contract_problem(*error),
        RuntimeArtifactMetadataDecodeError::InvalidMetadata(error) => catalog_problem(error),
    }
}

fn contract_problem(error: RuntimeContractBuildError) -> DiagnosticRuntimeArtifactProblem {
    match error {
        RuntimeContractBuildError::DuplicateRole(role) => {
            DiagnosticRuntimeArtifactProblem::DuplicateContractRole(role.as_str().to_owned())
        }
        RuntimeContractBuildError::CompilerOwnedRole(role) => {
            DiagnosticRuntimeArtifactProblem::CompilerOwnedRole(role.as_str().to_owned())
        }
        RuntimeContractBuildError::MissingCooperativeExecution => {
            DiagnosticRuntimeArtifactProblem::MissingCooperativeExecution
        }
    }
}

fn catalog_problem(error: &RuntimeArtifactMetadataBuildError) -> DiagnosticRuntimeArtifactProblem {
    match error {
        RuntimeArtifactMetadataBuildError::InvalidNativeIndexFileName => {
            DiagnosticRuntimeArtifactProblem::InvalidNativeIndexFileName
        }
        RuntimeArtifactMetadataBuildError::InvalidNativeIndexes => {
            DiagnosticRuntimeArtifactProblem::InvalidNativeIndexes
        }
        RuntimeArtifactMetadataBuildError::DuplicateComponent(component) => {
            DiagnosticRuntimeArtifactProblem::DuplicateComponent(component.as_str().to_owned())
        }
        RuntimeArtifactMetadataBuildError::UnknownComponentRole(role) => {
            DiagnosticRuntimeArtifactProblem::UnknownComponentRole(role.as_str().to_owned())
        }
        RuntimeArtifactMetadataBuildError::UnknownComponentCapability(capability) => {
            DiagnosticRuntimeArtifactProblem::UnknownComponentCapability(
                capability.as_str().to_owned(),
            )
        }
        RuntimeArtifactMetadataBuildError::TestRoleInProductComponent(component) => {
            DiagnosticRuntimeArtifactProblem::TestRoleInProductComponent(
                component.as_str().to_owned(),
            )
        }
        RuntimeArtifactMetadataBuildError::MissingRoleOwner { purpose, role } => {
            DiagnosticRuntimeArtifactProblem::MissingRoleOwner {
                purpose: diagnostic_purpose(*purpose),
                role: role.as_str().to_owned(),
            }
        }
        RuntimeArtifactMetadataBuildError::DuplicateRoleOwner { purpose, role } => {
            DiagnosticRuntimeArtifactProblem::DuplicateRoleOwner {
                purpose: diagnostic_purpose(*purpose),
                role: role.as_str().to_owned(),
            }
        }
        RuntimeArtifactMetadataBuildError::MissingCapabilityOwner {
            purpose,
            capability,
        } => DiagnosticRuntimeArtifactProblem::MissingCapabilityOwner {
            purpose: diagnostic_purpose(*purpose),
            capability: capability.as_str().to_owned(),
        },
        RuntimeArtifactMetadataBuildError::DuplicateCapabilityOwner {
            purpose,
            capability,
        } => DiagnosticRuntimeArtifactProblem::DuplicateCapabilityOwner {
            purpose: diagnostic_purpose(*purpose),
            capability: capability.as_str().to_owned(),
        },
        RuntimeArtifactMetadataBuildError::DuplicatePlatformServiceOwner { purpose, .. } => {
            DiagnosticRuntimeArtifactProblem::DuplicatePlatformServiceOwner {
                purpose: diagnostic_purpose(*purpose),
            }
        }
    }
}

const fn diagnostic_purpose(purpose: RuntimeArtifactPurpose) -> DiagnosticRuntimeArtifactPurpose {
    match purpose {
        RuntimeArtifactPurpose::Product => DiagnosticRuntimeArtifactPurpose::Product,
        RuntimeArtifactPurpose::TestRunner => DiagnosticRuntimeArtifactPurpose::TestRunner,
    }
}

const fn build_problem(error: RuntimeArtifactBuildError) -> DiagnosticRuntimeArtifactProblem {
    match error {
        RuntimeArtifactBuildError::InvalidNativeTarget => {
            DiagnosticRuntimeArtifactProblem::InvalidNativeTarget
        }
        RuntimeArtifactBuildError::IncompatibleIndexTarget => {
            DiagnosticRuntimeArtifactProblem::IncompatibleIndexTarget
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
        DiagnosticRuntimeArtifactPurpose,
    };
    use bray_runtime_interface::{
        RuntimeAbiRole, RuntimeAbiVersion, RuntimeArtifactBuildError, RuntimeArtifactId,
        RuntimeArtifactMetadataBuildError, RuntimeArtifactMetadataDecodeError,
        RuntimeArtifactPurpose, RuntimeCapability, RuntimeContractBuildError,
    };
    use bray_target::TargetIdentity;
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

        bray_testing::assert_goal_state_diagnostic_kind(
            &diagnostics,
            DiagnosticKind::RuntimeArtifactMetadataReadFailed,
        );
    }

    #[test]
    fn invalid_runtime_metadata_preserves_typed_decode_and_build_failures() {
        let path = std::path::PathBuf::from("runtime.brayrt");

        let component = RuntimeArtifactId::try_new("runtime.scheduler")
            .unwrap_or_else(|| panic!("test component identity must be valid"));

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
                RuntimeArtifactMetadataDecodeError::InvalidNativeIndexDigest,
                DiagnosticRuntimeArtifactProblem::InvalidNativeIndexDigest,
            ),
            (
                RuntimeArtifactMetadataDecodeError::InvalidContract(
                    RuntimeContractBuildError::DuplicateRole(RuntimeAbiRole::Wake),
                ),
                DiagnosticRuntimeArtifactProblem::DuplicateContractRole("wake".to_owned()),
            ),
            (
                RuntimeArtifactMetadataDecodeError::InvalidMetadata(
                    RuntimeArtifactMetadataBuildError::DuplicateComponent(component.clone()),
                ),
                DiagnosticRuntimeArtifactProblem::DuplicateComponent(
                    "runtime.scheduler".to_owned(),
                ),
            ),
            (
                RuntimeArtifactMetadataDecodeError::InvalidMetadata(
                    RuntimeArtifactMetadataBuildError::MissingCapabilityOwner {
                        purpose: RuntimeArtifactPurpose::TestRunner,
                        capability: RuntimeCapability::Reactor,
                    },
                ),
                DiagnosticRuntimeArtifactProblem::MissingCapabilityOwner {
                    purpose: DiagnosticRuntimeArtifactPurpose::TestRunner,
                    capability: "reactor".to_owned(),
                },
            ),
        ];

        for (source, expected) in decode_cases {
            let diagnostics = runtime_load_diagnostics(RuntimeArtifactLoadError::InvalidMetadata {
                path: path.clone(),
                source,
            });

            bray_testing::assert_goal_state_diagnostic_kind(
                &diagnostics,
                DiagnosticKind::RuntimeArtifactMetadataInvalid,
            );

            assert_runtime_problem(&diagnostics, expected);
        }

        let build_cases = [
            (
                RuntimeArtifactBuildError::InvalidNativeTarget,
                DiagnosticRuntimeArtifactProblem::InvalidNativeTarget,
            ),
            (
                RuntimeArtifactBuildError::IncompatibleIndexTarget,
                DiagnosticRuntimeArtifactProblem::IncompatibleIndexTarget,
            ),
        ];

        for (source, expected) in build_cases {
            let diagnostics = runtime_load_diagnostics(RuntimeArtifactLoadError::InvalidArtifact {
                path: path.clone(),
                source,
            });

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

    #[test]
    fn incompatible_runtime_metadata_preserves_target_and_abi_context() {
        let path = std::path::PathBuf::from("runtime.brayrt");

        let expected_target = TargetIdentity::try_new("x86_64-unknown-linux-gnu")
            .unwrap_or_else(|| panic!("test target identity must be valid"));

        let actual_target = TargetIdentity::try_new("aarch64-unknown-linux-gnu")
            .unwrap_or_else(|| panic!("test target identity must be valid"));

        let target_diagnostics =
            runtime_load_diagnostics(RuntimeArtifactLoadError::IncompatibleTarget {
                path: path.clone(),
                expected: expected_target,
                actual: actual_target,
            });

        bray_testing::assert_goal_state_diagnostic_kind(
            &target_diagnostics,
            DiagnosticKind::RuntimeArtifactTargetMismatch,
        );

        let abi_diagnostics =
            runtime_load_diagnostics(RuntimeArtifactLoadError::IncompatibleRuntimeAbi {
                path,
                expected: RuntimeAbiVersion::new(1, 0),
                actual: RuntimeAbiVersion::new(2, 0),
            });

        bray_testing::assert_goal_state_diagnostic_kind(
            &abi_diagnostics,
            DiagnosticKind::RuntimeArtifactAbiMismatch,
        );
    }
}
