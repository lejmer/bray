use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticArtifactDigest, DiagnosticArtifactDigestAlgorithm,
    DiagnosticBag, DiagnosticId, DiagnosticIoErrorKind, DiagnosticKind, DiagnosticNote,
    DiagnosticNoteKind, DiagnosticRuntimeAbiVersion, SeverityKind,
};
use bray_package_interface::InterfaceValidationError;
use bray_standard_library::{StandardLibraryArtifactDigest, StandardLibraryLoadError};

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

pub(super) fn interface_diagnostics(
    kind: DiagnosticKind,
    input: &DependencyInterfaceInput,
) -> DiagnosticBag {
    DiagnosticBag::single(with_dependency_context(
        Diagnostic::new(DiagnosticId::new(0), kind, SeverityKind::Error),
        input,
    ))
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
        StandardLibraryLoadError::Manifest(_) => (
            Diagnostic::new(
                DiagnosticId::new(0),
                DiagnosticKind::StandardLibraryManifestInvalid,
                SeverityKind::Error,
            ),
            None,
        ),
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
            .with_arg(DiagnosticArg::referenced_name(target.as_str())),
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
            .with_arg(DiagnosticArg::referenced_name(target.as_str()))
            .with_arg(DiagnosticArg::expected_runtime_abi(diagnostic_runtime_abi(
                expected,
            )))
            .with_arg(DiagnosticArg::actual_runtime_abi(diagnostic_runtime_abi(
                actual,
            ))),
            None,
        ),
        StandardLibraryLoadError::Infrastructure => (
            Diagnostic::new(
                DiagnosticId::new(0),
                DiagnosticKind::StandardLibraryManifestInvalid,
                SeverityKind::Error,
            ),
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

const fn diagnostic_digest(digest: StandardLibraryArtifactDigest) -> DiagnosticArtifactDigest {
    DiagnosticArtifactDigest::new(DiagnosticArtifactDigestAlgorithm::Blake3, digest.bytes())
}

const fn diagnostic_runtime_abi(
    version: bray_runtime_interface::RuntimeAbiVersion,
) -> DiagnosticRuntimeAbiVersion {
    DiagnosticRuntimeAbiVersion::new(version.major(), version.minor())
}

pub(super) fn implementation_body_diagnostics(input: &DependencyInterfaceInput) -> DiagnosticBag {
    implementation_artifact_diagnostics(
        input,
        DiagnosticKind::InterfaceConstantCallableBodyUnavailable,
    )
}

pub(super) fn executable_template_diagnostics(
    input: &DependencyInterfaceInput,
) -> DiagnosticBag {
    implementation_artifact_diagnostics(
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

fn implementation_artifact_diagnostics(
    input: &DependencyInterfaceInput,
    kind: DiagnosticKind,
) -> DiagnosticBag {
    let diagnostic = Diagnostic::new(
        DiagnosticId::new(0),
        kind,
        SeverityKind::Error,
    );

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
        diagnostic = diagnostic.with_primary_span(span);
    }

    diagnostic.with_note(note)
}

#[cfg(test)]
mod tests {
    use std::io::ErrorKind;

    use bray_diagnostics::{DiagnosticArg, DiagnosticRuntimeAbiVersion};
    use bray_package_interface::{
        InterfaceLanguageRevision, InterfaceProductIdentity, InterfaceValidationPolicy,
    };
    use bray_runtime_interface::RuntimeAbiVersion;
    use bray_standard_library::StandardLibraryLoadError;
    use bray_symbols::PackageIdentity;
    use bray_target::TargetIdentity;

    use super::standard_library_diagnostics;
    use crate::request::DependencyInterfaceInput;

    #[test]
    fn standard_library_diagnostics_preserve_artifact_paths_and_abi_versions() {
        let input = dependency_input();
        let artifact_path = std::path::PathBuf::from("targets/test/1.0/libstd.a");

        let bag = standard_library_diagnostics(
            StandardLibraryLoadError::Read {
                path: artifact_path.clone(),
                kind: ErrorKind::NotFound,
            },
            &input,
        );

        let [diagnostic] = bag.diagnostics() else {
            panic!("artifact failure must produce one diagnostic");
        };

        assert_eq!(
            diagnostic.args()[0],
            DiagnosticArg::file_path(artifact_path.clone())
        );

        assert_eq!(
            diagnostic.notes()[0].args()[2],
            DiagnosticArg::artifact_path(artifact_path)
        );

        let target = TargetIdentity::try_new("test-target")
            .unwrap_or_else(|| panic!("test target identity must be valid"));

        let bag = standard_library_diagnostics(
            StandardLibraryLoadError::RuntimeAbiMismatch {
                target,
                expected: RuntimeAbiVersion::new(2, 1),
                actual: RuntimeAbiVersion::new(1, 4),
            },
            &input,
        );

        let [diagnostic] = bag.diagnostics() else {
            panic!("ABI mismatch must produce one diagnostic");
        };

        assert_eq!(
            diagnostic.args()[1],
            DiagnosticArg::expected_runtime_abi(DiagnosticRuntimeAbiVersion::new(2, 1))
        );

        assert_eq!(
            diagnostic.args()[2],
            DiagnosticArg::actual_runtime_abi(DiagnosticRuntimeAbiVersion::new(1, 4))
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
