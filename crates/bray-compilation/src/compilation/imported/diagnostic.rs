use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticArtifactDigest, DiagnosticArtifactDigestAlgorithm,
    DiagnosticBag, DiagnosticId, DiagnosticKind, DiagnosticLabel, DiagnosticLabelKind,
    DiagnosticNote, DiagnosticNoteKind, DiagnosticRuntimeAbiVersion,
    DiagnosticStandardLibraryManifestProblem, SeverityKind,
};
use bray_package_interface::InterfaceValidationError;
use bray_standard_library::{
    StandardLibraryArtifactDigest, StandardLibraryLoadError, StandardLibraryManifestError,
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

pub(in crate::compilation) fn native_artifact_diagnostics(
    error: bray_package_interface::PackageNativeArtifactError,
    input: &DependencyInterfaceInput,
) -> DiagnosticBag {
    let diagnostic = Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::InterfaceValidationFailed,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::interface_validation_failure(
        error.into_diagnostic_failure(),
    ));

    DiagnosticBag::single(with_dependency_context_path(
        diagnostic,
        input,
        input
            .implementation_artifact_path()
            .unwrap_or_else(|| input.artifact_path()),
    ))
}

pub(in crate::compilation) fn implementation_validation_diagnostics(
    error: InterfaceValidationError,
    input: &DependencyInterfaceInput,
) -> DiagnosticBag {
    let diagnostic = with_dependency_context_path(
        error.into_diagnostic(DiagnosticId::new(0)),
        input,
        input
            .implementation_artifact_path()
            .unwrap_or_else(|| input.artifact_path()),
    );

    DiagnosticBag::single(diagnostic)
}

pub(super) fn contextual_interface_diagnostic(
    diagnostic: Diagnostic,
    input: &DependencyInterfaceInput,
) -> DiagnosticBag {
    DiagnosticBag::single(with_dependency_context(diagnostic, input))
}

pub(super) fn dependency_artifact_diagnostics(
    error: bray_package_interface::PackageArtifactLoadError,
    input: &DependencyInterfaceInput,
    path: &std::path::Path,
) -> DiagnosticBag {
    DiagnosticBag::single(with_dependency_context_path(
        error
            .into_diagnostic(DiagnosticId::new(0))
            .with_arg(DiagnosticArg::file_path(path)),
        input,
        path,
    ))
}

pub(in crate::compilation) fn standard_library_diagnostics(
    error: StandardLibraryLoadError,
    manifest_path: &std::path::Path,
) -> DiagnosticBag {
    let (diagnostic, artifact_path) = standard_library_failure_diagnostic(error);

    let artifact_path = artifact_path.as_deref().unwrap_or(manifest_path);

    DiagnosticBag::single(diagnostic.with_note(interface_dependency_context_note(
        bray_standard_library::PUBLIC_STANDARD_LIBRARY_PACKAGE_IDENTITY,
        bray_standard_library::PUBLIC_STANDARD_LIBRARY_PRODUCT_IDENTITY,
        artifact_path,
    )))
}

fn standard_library_failure_diagnostic(
    error: StandardLibraryLoadError,
) -> (Diagnostic, Option<std::path::PathBuf>) {
    match error {
        StandardLibraryLoadError::Read { path, kind } => {
            let diagnostic = bray_package_interface::PackageArtifactLoadError::Read(kind)
                .into_diagnostic(DiagnosticId::new(0))
                // The diagnostic argument and dependency context independently own the path.
                .with_arg(DiagnosticArg::file_path(path.clone()));

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
        StandardLibraryLoadError::Implementation { path, cause } => {
            (cause.into_diagnostic(DiagnosticId::new(0)), Some(path))
        }
        StandardLibraryLoadError::Infrastructure { path } => {
            let diagnostic = Diagnostic::new(
                DiagnosticId::new(0),
                DiagnosticKind::StandardLibraryInfrastructureFailure,
                SeverityKind::Error,
            )
            .with_arg(DiagnosticArg::artifact_path(path.clone()))
            .with_note(DiagnosticNote::new(
                DiagnosticNoteKind::ReportCompilerDefect,
            ));

            (diagnostic, Some(path))
        }
    }
}

/// Maps the shared native-index error to its locale-neutral diagnostic cause.
pub fn diagnostic_native_artifact_cause(
    error: &bray_native_artifact::NativeIndexError,
) -> bray_diagnostics::DiagnosticNativeArtifactCause {
    use bray_diagnostics::DiagnosticNativeArtifactCause as Cause;
    use bray_native_artifact::{NativeIndexError as Error, WireError};

    let cause = match error {
        Error::SizeLimitExceeded => Cause::IndexSizeLimitExceeded,
        Error::Malformed => Cause::IndexMalformed,
        Error::Wire(WireError::UnsupportedSchema) => Cause::IndexUnsupportedSchema,
        Error::Wire(WireError::InvalidTarget) => Cause::IndexInvalidTarget,
        Error::Wire(WireError::InvalidDigest) => Cause::IndexInvalidDigest,
        Error::Wire(WireError::InvalidSymbol) => Cause::IndexInvalidSymbol,
        Error::Wire(WireError::InvalidLink) => Cause::IndexInvalidLink,
        Error::IndexDigestMismatch { .. } => Cause::IndexDigestMismatch,
        Error::PayloadDigestMismatch { .. } => Cause::PayloadDigestMismatch,
        Error::WrongTarget { .. } => Cause::WrongTarget,
        Error::WrongProducer { .. } => Cause::WrongProducer,
        Error::Read { .. } => Cause::ReadFailure,
        Error::DuplicateUnit(_) => Cause::DuplicateUnit,
        Error::InvalidSummary(_) => Cause::InvalidSummary,
        Error::DuplicateDefinition(_) => Cause::DuplicateDefinition,
        Error::InvalidAssociation(_) => Cause::InvalidAssociation,
        Error::NoncanonicalSummary(_) => Cause::NoncanonicalSummary,
        Error::MissingCoRetentionMember(_) => Cause::MissingCoRetentionMember,
        Error::DuplicateCoRetentionGroup => Cause::DuplicateCoRetentionGroup,
        Error::InvalidCoRetentionGroup => Cause::InvalidCoRetentionGroup,
    };

    cause
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
        StandardLibraryManifestError::BundleDigestMismatch => {
            DiagnosticStandardLibraryManifestProblem::BundleDigestMismatch
        }
        StandardLibraryManifestError::LengthExceeded => {
            DiagnosticStandardLibraryManifestProblem::LengthExceeded
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
    let note = interface_dependency_context_note(
        input.package().as_str(),
        input.product().as_str(),
        artifact_path,
    );

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

fn interface_dependency_context_note(
    package: &str,
    product: &str,
    artifact_path: &std::path::Path,
) -> DiagnosticNote {
    DiagnosticNote::new(DiagnosticNoteKind::InterfaceDependencyContext)
        .with_arg(DiagnosticArg::expected_package_identity(package))
        .with_arg(DiagnosticArg::expected_product_identity(product))
        .with_arg(DiagnosticArg::artifact_path(artifact_path))
}

#[cfg(test)]
mod tests {
    use std::io::ErrorKind;

    use bray_diagnostics::{DiagnosticArg, DiagnosticKind, DiagnosticRuntimeAbiVersion};
    use bray_package_interface::{
        InterfaceArtifactHash, InterfaceFormatRevision, InterfaceLanguageRevision, InterfaceLimit,
        InterfaceMalformedCause, InterfaceProductIdentity, InterfaceSectionHash,
        InterfaceSectionTag, InterfaceValidationContext, InterfaceValidationError,
        InterfaceValidationField, InterfaceValidationPolicy,
    };
    use bray_runtime_interface::RuntimeAbiVersion;
    use bray_standard_library::{StandardLibraryArtifactDigest, StandardLibraryLoadError};
    use bray_symbols::PackageIdentity;
    use bray_target::TargetIdentity;

    use super::{
        dependency_artifact_diagnostics, standard_library_diagnostics, validation_diagnostics,
    };
    use crate::request::DependencyInterfaceInput;

    #[test]
    fn package_artifact_read_failure_preserves_path_and_rendering_contract() {
        let input = dependency_input();
        let path = std::path::Path::new("interfaces/library.brayimpl");

        let diagnostics = dependency_artifact_diagnostics(
            bray_package_interface::PackageArtifactLoadError::Read(ErrorKind::NotFound),
            &input,
            path,
        );

        let diagnostic = bray_testing::single_diagnostic(&diagnostics);

        assert_eq!(diagnostic.args()[1], DiagnosticArg::file_path(path));

        assert_eq!(
            diagnostic.notes()[0].args()[2],
            DiagnosticArg::artifact_path(path)
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &diagnostics,
            DiagnosticKind::PackageArtifactReadFailed,
        );
    }

    #[test]
    fn standard_library_diagnostics_preserve_selected_artifacts_and_exact_causes() {
        let manifest_path = std::path::Path::new("standard-library/manifest.json");
        let artifact_path = std::path::PathBuf::from("targets/test/1.0/libstd.a");

        let read_bag = standard_library_diagnostics(
            StandardLibraryLoadError::Read {
                path: artifact_path.clone(),
                kind: ErrorKind::NotFound,
            },
            manifest_path,
        );

        let diagnostic = bray_testing::single_diagnostic(&read_bag);

        assert_eq!(
            diagnostic.args()[1],
            DiagnosticArg::file_path(artifact_path.clone())
        );

        assert_eq!(
            diagnostic.notes()[0].args()[2],
            DiagnosticArg::artifact_path(artifact_path)
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &read_bag,
            DiagnosticKind::PackageArtifactReadFailed,
        );

        let length_bag = standard_library_diagnostics(
            StandardLibraryLoadError::ArtifactLengthMismatch {
                path: "targets/test/1.0/libstd-length.a".into(),
                expected: 16,
                actual: 12,
            },
            manifest_path,
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
            manifest_path,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &digest_bag,
            DiagnosticKind::StandardLibraryArtifactDigestMismatch,
        );

        let target = TargetIdentity::try_new("test-target")
            .unwrap_or_else(|| panic!("test target identity must be valid"));

        let target_bag = standard_library_diagnostics(
            StandardLibraryLoadError::TargetUnavailable(target.clone()),
            manifest_path,
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
            manifest_path,
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

        let infrastructure_path = std::path::PathBuf::from("interfaces/std-cache.brayi");

        let infrastructure_bag = standard_library_diagnostics(
            StandardLibraryLoadError::Infrastructure {
                path: infrastructure_path.clone(),
            },
            manifest_path,
        );

        let diagnostic = bray_testing::single_diagnostic(&infrastructure_bag);

        assert_eq!(
            diagnostic.args(),
            &[DiagnosticArg::artifact_path(infrastructure_path)]
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &infrastructure_bag,
            DiagnosticKind::StandardLibraryInfrastructureFailure,
        );
    }

    #[test]
    fn interface_validation_diagnostics_preserve_context_for_every_failure_category() {
        let input = dependency_input();

        let invalid_magic = validation_diagnostics(
            InterfaceValidationError::InvalidMagic { actual: [0; 8] },
            &input,
        );

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

        let encoding = validation_diagnostics(
            InterfaceValidationError::UnsupportedByteOrder {
                expected: 1,
                actual: 2,
            },
            &input,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &encoding,
            DiagnosticKind::InterfaceUnsupportedEncoding,
        );

        let truncated = validation_diagnostics(
            InterfaceValidationError::Truncated {
                context: InterfaceValidationContext::Header,
                field: InterfaceValidationField::Magic,
                offset: 0,
                expected_length: 8,
                actual_length: 7,
            },
            &input,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &truncated,
            DiagnosticKind::InterfaceValidationFailed,
        );

        let malformed = validation_diagnostics(
            InterfaceValidationError::Malformed {
                context: InterfaceValidationContext::Header,
                cause: InterfaceMalformedCause::InvalidValue {
                    field: InterfaceValidationField::RequiredFlags,
                },
            },
            &input,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &malformed,
            DiagnosticKind::InterfaceValidationFailed,
        );

        let hash = validation_diagnostics(
            InterfaceValidationError::ArtifactHashMismatch {
                expected: InterfaceArtifactHash::from_bytes([0; 32]),
                actual: InterfaceArtifactHash::from_bytes([1; 32]),
            },
            &input,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &hash,
            DiagnosticKind::InterfaceHashMismatch,
        );

        let checksum = validation_diagnostics(
            InterfaceValidationError::SectionChecksumMismatch {
                section: InterfaceSectionTag::DeclarationSemantics,
                expected: InterfaceSectionHash::from_bytes([0; 32]),
                actual: InterfaceSectionHash::from_bytes([1; 32]),
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
