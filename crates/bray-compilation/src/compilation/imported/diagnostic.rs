use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticArtifactDigest, DiagnosticArtifactDigestAlgorithm,
    DiagnosticBag, DiagnosticId, DiagnosticIoErrorKind, DiagnosticKind, DiagnosticLabel,
    DiagnosticLabelKind, DiagnosticNote, DiagnosticNoteKind, DiagnosticRuntimeAbiVersion,
    DiagnosticStandardLibraryManifestProblem, DiagnosticStandardLibraryOptimizationMetadataProblem,
    SeverityKind,
};
use bray_package_interface::InterfaceValidationError;
use bray_standard_library::{
    StandardLibraryArtifactDigest, StandardLibraryLoadError, StandardLibraryManifestError,
    StandardLibraryOptimizationMetadataProblem,
};

use crate::request::DependencyInterfaceInput;

pub(super) fn validation_diagnostics(
    error: InterfaceValidationError,
    input: &DependencyInterfaceInput,
) -> DiagnosticBag {
    DiagnosticBag::single(with_dependency_context(
        error.into_diagnostic(DiagnosticId::new(0)),
        input,
    ))
}

pub(super) fn contextual_interface_diagnostic(
    diagnostic: Diagnostic,
    input: &DependencyInterfaceInput,
) -> DiagnosticBag {
    DiagnosticBag::single(with_dependency_context(diagnostic, input))
}

pub(super) fn unlocated_interface_diagnostics(kind: DiagnosticKind) -> DiagnosticBag {
    DiagnosticBag::single(Diagnostic::new(
        DiagnosticId::new(0),
        kind,
        SeverityKind::Error,
    ))
}

pub(super) fn standard_library_diagnostics(
    error: StandardLibraryLoadError,
    input: &DependencyInterfaceInput,
) -> DiagnosticBag {
    let (diagnostic, artifact_path) = match error {
        StandardLibraryLoadError::Read { path, kind } => {
            let diagnostic = Diagnostic::new(
                DiagnosticId::new(0),
                DiagnosticKind::StandardLibraryArtifactReadFailed,
                SeverityKind::Error,
            )
            // The diagnostic argument and dependency context independently own the path.
            .with_arg(DiagnosticArg::file_path(path.clone()))
            .with_arg(DiagnosticArg::io_error_kind(DiagnosticIoErrorKind::from(
                kind,
            )));

            (diagnostic, Some(path))
        }
        StandardLibraryLoadError::Manifest { path, error } => {
            let diagnostic = Diagnostic::new(
                DiagnosticId::new(0),
                DiagnosticKind::StandardLibraryManifestInvalid,
                SeverityKind::Error,
            )
            .with_arg(DiagnosticArg::file_path(path.clone()))
            .with_arg(DiagnosticArg::standard_library_manifest_problem(
                manifest_problem(error),
            ));

            (diagnostic, Some(path))
        }
        StandardLibraryLoadError::ArtifactLengthMismatch {
            path,
            expected,
            actual,
        } => {
            let diagnostic = Diagnostic::new(
                DiagnosticId::new(0),
                DiagnosticKind::StandardLibraryArtifactLengthMismatch,
                SeverityKind::Error,
            )
            // The diagnostic argument and dependency context independently own the path.
            .with_arg(DiagnosticArg::file_path(path.clone()))
            .with_arg(DiagnosticArg::expected_byte_count(expected))
            .with_arg(DiagnosticArg::actual_byte_count(actual));

            (diagnostic, Some(path))
        }
        StandardLibraryLoadError::ArtifactDigestMismatch {
            path,
            expected,
            actual,
        } => {
            let diagnostic = Diagnostic::new(
                DiagnosticId::new(0),
                DiagnosticKind::StandardLibraryArtifactDigestMismatch,
                SeverityKind::Error,
            )
            // The diagnostic argument and dependency context independently own the path.
            .with_arg(DiagnosticArg::file_path(path.clone()))
            .with_arg(DiagnosticArg::expected_artifact_digest(diagnostic_digest(
                expected,
            )))
            .with_arg(DiagnosticArg::actual_artifact_digest(diagnostic_digest(
                actual,
            )));

            (diagnostic, Some(path))
        }
        StandardLibraryLoadError::TargetUnavailable(target) => (
            Diagnostic::new(
                DiagnosticId::new(0),
                DiagnosticKind::StandardLibraryTargetUnavailable,
                SeverityKind::Error,
            )
            .with_arg(DiagnosticArg::target_triple(target.as_str())),
            None,
        ),
        StandardLibraryLoadError::RuntimeAbiMismatch {
            target,
            expected,
            actual,
        } => (
            Diagnostic::new(
                DiagnosticId::new(0),
                DiagnosticKind::StandardLibraryRuntimeAbiMismatch,
                SeverityKind::Error,
            )
            .with_arg(DiagnosticArg::target_triple(target.as_str()))
            .with_arg(DiagnosticArg::expected_runtime_abi(diagnostic_runtime_abi(
                expected,
            )))
            .with_arg(DiagnosticArg::actual_runtime_abi(diagnostic_runtime_abi(
                actual,
            ))),
            None,
        ),
        StandardLibraryLoadError::OptimizationUnavailable { target } => (
            Diagnostic::new(
                DiagnosticId::new(0),
                DiagnosticKind::StandardLibraryOptimizationUnavailable,
                SeverityKind::Error,
            )
            .with_arg(DiagnosticArg::target_triple(target.as_str())),
            None,
        ),
        StandardLibraryLoadError::Infrastructure => (
            Diagnostic::new(
                DiagnosticId::new(0),
                DiagnosticKind::StandardLibraryInfrastructureFailure,
                SeverityKind::Error,
            )
            .with_arg(DiagnosticArg::artifact_path(input.artifact_path()))
            .with_note(DiagnosticNote::new(
                DiagnosticNoteKind::ReportCompilerDefect,
            )),
            None,
        ),
    };

    DiagnosticBag::single(with_dependency_context_path(
        diagnostic,
        input,
        artifact_path
            .as_deref()
            .unwrap_or_else(|| input.artifact_path()),
    ))
}

const fn manifest_problem(
    error: StandardLibraryManifestError,
) -> DiagnosticStandardLibraryManifestProblem {
    match error {
        StandardLibraryManifestError::Malformed => {
            DiagnosticStandardLibraryManifestProblem::Malformed
        }
        StandardLibraryManifestError::NonCanonicalEncoding => {
            DiagnosticStandardLibraryManifestProblem::NonCanonicalEncoding
        }
        StandardLibraryManifestError::InvalidDigest => {
            DiagnosticStandardLibraryManifestProblem::InvalidDigest
        }
        StandardLibraryManifestError::InvalidInterfaceArtifact => {
            DiagnosticStandardLibraryManifestProblem::InvalidInterfaceArtifact
        }
        StandardLibraryManifestError::InvalidImplementationArtifact => {
            DiagnosticStandardLibraryManifestProblem::InvalidImplementationArtifact
        }
        StandardLibraryManifestError::InvalidTargetArtifact => {
            DiagnosticStandardLibraryManifestProblem::InvalidTargetArtifact
        }
        StandardLibraryManifestError::InvalidArtifactPath => {
            DiagnosticStandardLibraryManifestProblem::InvalidArtifactPath
        }
        StandardLibraryManifestError::MissingArtifact => {
            DiagnosticStandardLibraryManifestProblem::MissingArtifact
        }
        StandardLibraryManifestError::DuplicateArtifact => {
            DiagnosticStandardLibraryManifestProblem::DuplicateArtifact
        }
        StandardLibraryManifestError::DuplicateArtifactPath => {
            DiagnosticStandardLibraryManifestProblem::DuplicateArtifactPath
        }
        StandardLibraryManifestError::MissingTarget => {
            DiagnosticStandardLibraryManifestProblem::MissingTarget
        }
        StandardLibraryManifestError::DuplicateTarget => {
            DiagnosticStandardLibraryManifestProblem::DuplicateTarget
        }
        StandardLibraryManifestError::InvalidIdentity => {
            DiagnosticStandardLibraryManifestProblem::InvalidIdentity
        }
        StandardLibraryManifestError::InvalidNativeLink => {
            DiagnosticStandardLibraryManifestProblem::InvalidNativeLink
        }
        StandardLibraryManifestError::InvalidPlatformServices => {
            DiagnosticStandardLibraryManifestProblem::InvalidPlatformServices
        }
        StandardLibraryManifestError::DuplicatePlatformService => {
            DiagnosticStandardLibraryManifestProblem::DuplicatePlatformService
        }
        StandardLibraryManifestError::InvalidOptimizationMetadata(problem) => {
            DiagnosticStandardLibraryManifestProblem::InvalidOptimizationMetadata(
                optimization_metadata_problem(problem),
            )
        }
        StandardLibraryManifestError::InvalidOptimizationFallback => {
            DiagnosticStandardLibraryManifestProblem::InvalidOptimizationFallback
        }
        StandardLibraryManifestError::BundleDigestMismatch => {
            DiagnosticStandardLibraryManifestProblem::BundleDigestMismatch
        }
        StandardLibraryManifestError::LengthExceeded => {
            DiagnosticStandardLibraryManifestProblem::LengthExceeded
        }
    }
}

const fn optimization_metadata_problem(
    problem: StandardLibraryOptimizationMetadataProblem,
) -> DiagnosticStandardLibraryOptimizationMetadataProblem {
    match problem {
        StandardLibraryOptimizationMetadataProblem::MissingForArchive => {
            DiagnosticStandardLibraryOptimizationMetadataProblem::MissingForArchive
        }
        StandardLibraryOptimizationMetadataProblem::AttachedToUnsupportedArtifact => {
            DiagnosticStandardLibraryOptimizationMetadataProblem::AttachedToUnsupportedArtifact
        }
        StandardLibraryOptimizationMetadataProblem::UnsupportedSemantics => {
            DiagnosticStandardLibraryOptimizationMetadataProblem::UnsupportedSemantics
        }
        StandardLibraryOptimizationMetadataProblem::UnsupportedProducerKind => {
            DiagnosticStandardLibraryOptimizationMetadataProblem::UnsupportedProducerKind
        }
        StandardLibraryOptimizationMetadataProblem::MissingProducerImplementation => {
            DiagnosticStandardLibraryOptimizationMetadataProblem::MissingProducerImplementation
        }
        StandardLibraryOptimizationMetadataProblem::MissingProducerImplementationRevision => {
            DiagnosticStandardLibraryOptimizationMetadataProblem::MissingProducerImplementationRevision
        }
        StandardLibraryOptimizationMetadataProblem::MissingToolchain => {
            DiagnosticStandardLibraryOptimizationMetadataProblem::MissingToolchain
        }
        StandardLibraryOptimizationMetadataProblem::MissingToolchainRevision => {
            DiagnosticStandardLibraryOptimizationMetadataProblem::MissingToolchainRevision
        }
        StandardLibraryOptimizationMetadataProblem::MissingTargetTriple => {
            DiagnosticStandardLibraryOptimizationMetadataProblem::MissingTargetTriple
        }
        StandardLibraryOptimizationMetadataProblem::MissingDataLayout => {
            DiagnosticStandardLibraryOptimizationMetadataProblem::MissingDataLayout
        }
        StandardLibraryOptimizationMetadataProblem::UnsupportedRelocationModel => {
            DiagnosticStandardLibraryOptimizationMetadataProblem::UnsupportedRelocationModel
        }
        StandardLibraryOptimizationMetadataProblem::UnsupportedCodeModel => {
            DiagnosticStandardLibraryOptimizationMetadataProblem::UnsupportedCodeModel
        }
        StandardLibraryOptimizationMetadataProblem::NonCanonicalFallbackPath => {
            DiagnosticStandardLibraryOptimizationMetadataProblem::NonCanonicalFallbackPath
        }
        StandardLibraryOptimizationMetadataProblem::ZeroModuleCount => {
            DiagnosticStandardLibraryOptimizationMetadataProblem::ZeroModuleCount
        }
        StandardLibraryOptimizationMetadataProblem::InvalidPreservationRoot => {
            DiagnosticStandardLibraryOptimizationMetadataProblem::InvalidPreservationRoot
        }
        StandardLibraryOptimizationMetadataProblem::UnsupportedLifecycleRoot => {
            DiagnosticStandardLibraryOptimizationMetadataProblem::UnsupportedLifecycleRoot
        }
        StandardLibraryOptimizationMetadataProblem::UnknownPlatformService => {
            DiagnosticStandardLibraryOptimizationMetadataProblem::UnknownPlatformService
        }
        StandardLibraryOptimizationMetadataProblem::NonCanonicalDependencyPath => {
            DiagnosticStandardLibraryOptimizationMetadataProblem::NonCanonicalDependencyPath
        }
        StandardLibraryOptimizationMetadataProblem::InvalidPartition => {
            DiagnosticStandardLibraryOptimizationMetadataProblem::InvalidPartition
        }
        StandardLibraryOptimizationMetadataProblem::DuplicatePartition => {
            DiagnosticStandardLibraryOptimizationMetadataProblem::DuplicatePartition
        }
        StandardLibraryOptimizationMetadataProblem::RuntimeAbiMismatch => {
            DiagnosticStandardLibraryOptimizationMetadataProblem::RuntimeAbiMismatch
        }
        StandardLibraryOptimizationMetadataProblem::TargetMismatch => {
            DiagnosticStandardLibraryOptimizationMetadataProblem::TargetMismatch
        }
        StandardLibraryOptimizationMetadataProblem::CompatibilityMismatch => {
            DiagnosticStandardLibraryOptimizationMetadataProblem::CompatibilityMismatch
        }
        StandardLibraryOptimizationMetadataProblem::ToolchainMismatch => {
            DiagnosticStandardLibraryOptimizationMetadataProblem::ToolchainMismatch
        }
        StandardLibraryOptimizationMetadataProblem::MissingDependencyArtifact => {
            DiagnosticStandardLibraryOptimizationMetadataProblem::MissingDependencyArtifact
        }
        StandardLibraryOptimizationMetadataProblem::MissingBrayPartition => {
            DiagnosticStandardLibraryOptimizationMetadataProblem::MissingBrayPartition
        }
    }
}

const fn diagnostic_digest(digest: StandardLibraryArtifactDigest) -> DiagnosticArtifactDigest {
    DiagnosticArtifactDigest::new(DiagnosticArtifactDigestAlgorithm::Blake3, digest.bytes())
}

const fn diagnostic_runtime_abi(
    version: bray_runtime_interface::RuntimeAbiVersion,
) -> DiagnosticRuntimeAbiVersion {
    DiagnosticRuntimeAbiVersion::new(version.major(), version.minor())
}

pub(super) fn implementation_body_diagnostics(input: &DependencyInterfaceInput) -> DiagnosticBag {
    implementation_artiquery_diagnostics(
        input,
        DiagnosticKind::InterfaceConstantCallableBodyUnavailable,
    )
}

pub(super) fn executable_template_diagnostics(input: &DependencyInterfaceInput) -> DiagnosticBag {
    implementation_artiquery_diagnostics(
        input,
        DiagnosticKind::InterfaceExecutableTemplateUnavailable,
    )
}

pub(super) fn executable_template_decode_diagnostics(
    input: &DependencyInterfaceInput,
    error: bray_package_interface::ExecutableTemplateDecodeError,
) -> DiagnosticBag {
    let bray_package_interface::ExecutableTemplateDecodeError::Validation(error) = error else {
        return executable_template_diagnostics(input);
    };

    let diagnostic = with_dependency_context_path(
        error.into_diagnostic(DiagnosticId::new(0)),
        input,
        input
            .implementation_artifact_path()
            .unwrap_or_else(|| input.artifact_path()),
    );

    DiagnosticBag::single(diagnostic)
}

fn implementation_artiquery_diagnostics(
    input: &DependencyInterfaceInput,
    kind: DiagnosticKind,
) -> DiagnosticBag {
    let diagnostic = Diagnostic::new(DiagnosticId::new(0), kind, SeverityKind::Error);

    DiagnosticBag::single(with_dependency_context_path(
        diagnostic,
        input,
        input
            .implementation_artifact_path()
            .unwrap_or_else(|| input.artifact_path()),
    ))
}

fn with_dependency_context(diagnostic: Diagnostic, input: &DependencyInterfaceInput) -> Diagnostic {
    with_dependency_context_path(diagnostic, input, input.artifact_path())
}

fn with_dependency_context_path(
    mut diagnostic: Diagnostic,
    input: &DependencyInterfaceInput,
    artifact_path: &std::path::Path,
) -> Diagnostic {
    let note = DiagnosticNote::new(DiagnosticNoteKind::InterfaceDependencyContext)
        .with_arg(DiagnosticArg::expected_package_identity(
            input.package().as_str(),
        ))
        .with_arg(DiagnosticArg::expected_product_identity(
            input.product().as_str(),
        ))
        .with_arg(DiagnosticArg::artifact_path(artifact_path));

    if let Some(span) = input.dependency_span() {
        diagnostic = diagnostic
            .with_primary_span(span)
            .with_label(DiagnosticLabel::primary(
                DiagnosticLabelKind::DependencySelection,
                span,
            ));
    }

    diagnostic.with_note(note)
}

#[cfg(test)]
mod tests {
    use std::io::ErrorKind;

    use bray_diagnostics::{
        DiagnosticArg, DiagnosticKind, DiagnosticRuntimeAbiVersion,
        DiagnosticStandardLibraryManifestProblem,
        DiagnosticStandardLibraryOptimizationMetadataProblem,
    };
    use bray_package_interface::{
        InterfaceFormatRevision, InterfaceLanguageRevision, InterfaceLimit,
        InterfaceProductIdentity, InterfaceSectionTag, InterfaceValidationError,
        InterfaceValidationPolicy,
    };
    use bray_runtime_interface::RuntimeAbiVersion;
    use bray_standard_library::{
        StandardLibraryArtifactDigest, StandardLibraryLoadError, StandardLibraryManifestError,
        StandardLibraryOptimizationMetadataProblem,
    };
    use bray_symbols::PackageIdentity;
    use bray_target::TargetIdentity;

    use super::{manifest_problem, standard_library_diagnostics, validation_diagnostics};
    use crate::request::DependencyInterfaceInput;

    #[test]
    fn standard_library_diagnostics_preserve_selected_artifacts_and_exact_causes() {
        let input = dependency_input();
        let artifact_path = std::path::PathBuf::from("targets/test/1.0/libstd.a");

        let read_bag = standard_library_diagnostics(
            StandardLibraryLoadError::Read {
                path: artifact_path.clone(),
                kind: ErrorKind::NotFound,
            },
            &input,
        );

        let diagnostic = bray_testing::single_diagnostic(&read_bag);

        assert_eq!(
            diagnostic.args()[0],
            DiagnosticArg::file_path(artifact_path.clone())
        );

        assert_eq!(
            diagnostic.notes()[0].args()[2],
            DiagnosticArg::artifact_path(artifact_path)
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &read_bag,
            DiagnosticKind::StandardLibraryArtifactReadFailed,
        );

        let length_bag = standard_library_diagnostics(
            StandardLibraryLoadError::ArtifactLengthMismatch {
                path: "targets/test/1.0/libstd-length.a".into(),
                expected: 16,
                actual: 12,
            },
            &input,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &length_bag,
            DiagnosticKind::StandardLibraryArtifactLengthMismatch,
        );

        let digest_bag = standard_library_diagnostics(
            StandardLibraryLoadError::ArtifactDigestMismatch {
                path: "targets/test/1.0/libstd-digest.a".into(),
                expected: StandardLibraryArtifactDigest::new([1; 32]),
                actual: StandardLibraryArtifactDigest::new([2; 32]),
            },
            &input,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &digest_bag,
            DiagnosticKind::StandardLibraryArtifactDigestMismatch,
        );

        let target = TargetIdentity::try_new("test-target")
            .unwrap_or_else(|| panic!("test target identity must be valid"));

        let target_bag = standard_library_diagnostics(
            StandardLibraryLoadError::TargetUnavailable(target.clone()),
            &input,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &target_bag,
            DiagnosticKind::StandardLibraryTargetUnavailable,
        );

        let abi_bag = standard_library_diagnostics(
            StandardLibraryLoadError::RuntimeAbiMismatch {
                target,
                expected: RuntimeAbiVersion::new(2, 1),
                actual: RuntimeAbiVersion::new(1, 4),
            },
            &input,
        );

        let diagnostic = bray_testing::single_diagnostic(&abi_bag);

        assert_eq!(
            diagnostic.args()[1],
            DiagnosticArg::expected_runtime_abi(DiagnosticRuntimeAbiVersion::new(2, 1))
        );

        assert_eq!(
            diagnostic.args()[2],
            DiagnosticArg::actual_runtime_abi(DiagnosticRuntimeAbiVersion::new(1, 4))
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &abi_bag,
            DiagnosticKind::StandardLibraryRuntimeAbiMismatch,
        );

        let infrastructure_bag =
            standard_library_diagnostics(StandardLibraryLoadError::Infrastructure, &input);

        bray_testing::assert_goal_state_diagnostic_kind(
            &infrastructure_bag,
            DiagnosticKind::StandardLibraryInfrastructureFailure,
        );
    }

    #[test]
    fn optimization_metadata_diagnostics_preserve_the_failed_contract() {
        let problem = manifest_problem(StandardLibraryManifestError::InvalidOptimizationMetadata(
            StandardLibraryOptimizationMetadataProblem::MissingDependencyArtifact,
        ));

        assert_eq!(
            problem,
            DiagnosticStandardLibraryManifestProblem::InvalidOptimizationMetadata(
                DiagnosticStandardLibraryOptimizationMetadataProblem::MissingDependencyArtifact,
            )
        );
    }

    #[test]
    fn interface_validation_diagnostics_preserve_context_for_every_failure_category() {
        let input = dependency_input();

        let invalid_magic = validation_diagnostics(InterfaceValidationError::InvalidMagic, &input);

        bray_testing::assert_goal_state_diagnostic_kind(
            &invalid_magic,
            DiagnosticKind::InterfaceInvalidMagic,
        );

        let format_revision = validation_diagnostics(
            InterfaceValidationError::UnsupportedFormatRevision {
                actual: InterfaceFormatRevision::new(9),
            },
            &input,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &format_revision,
            DiagnosticKind::InterfaceUnsupportedFormatRevision,
        );

        let language_revision = validation_diagnostics(
            InterfaceValidationError::UnsupportedLanguageRevision {
                expected: InterfaceLanguageRevision::new(1),
                actual: InterfaceLanguageRevision::new(2),
            },
            &input,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &language_revision,
            DiagnosticKind::InterfaceUnsupportedLanguageRevision,
        );

        let encoding =
            validation_diagnostics(InterfaceValidationError::UnsupportedEncoding, &input);

        bray_testing::assert_goal_state_diagnostic_kind(
            &encoding,
            DiagnosticKind::InterfaceUnsupportedEncoding,
        );

        let truncated = validation_diagnostics(InterfaceValidationError::Truncated, &input);

        bray_testing::assert_goal_state_diagnostic_kind(
            &truncated,
            DiagnosticKind::InterfaceTruncated,
        );

        let malformed = validation_diagnostics(InterfaceValidationError::Malformed, &input);

        bray_testing::assert_goal_state_diagnostic_kind(
            &malformed,
            DiagnosticKind::InterfaceMalformed,
        );

        let hash = validation_diagnostics(InterfaceValidationError::HashMismatch, &input);

        bray_testing::assert_goal_state_diagnostic_kind(
            &hash,
            DiagnosticKind::InterfaceHashMismatch,
        );

        let checksum = validation_diagnostics(
            InterfaceValidationError::SectionChecksumMismatch {
                section: InterfaceSectionTag::DeclarationSemantics,
            },
            &input,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &checksum,
            DiagnosticKind::InterfaceSectionChecksumMismatch,
        );

        let limit = validation_diagnostics(
            InterfaceValidationError::ResourceLimitExceeded {
                limit: InterfaceLimit::RecordCount,
                actual: 65,
                maximum: 64,
            },
            &input,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &limit,
            DiagnosticKind::InterfaceResourceLimitExceeded,
        );
    }

    fn dependency_input() -> DependencyInterfaceInput {
        let package = PackageIdentity::try_new("std")
            .unwrap_or_else(|| panic!("package identity must be valid"));

        let product = InterfaceProductIdentity::try_new("library")
            .unwrap_or_else(|| panic!("product identity must be valid"));

        DependencyInterfaceInput::new(
            package,
            product,
            "interfaces/std.brayi",
            [],
            InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
        )
    }
}
