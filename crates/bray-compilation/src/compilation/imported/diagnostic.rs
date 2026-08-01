use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticArtifactDigest, DiagnosticArtifactDigestAlgorithm,
    DiagnosticBag, DiagnosticId, DiagnosticIoErrorKind, DiagnosticKind, DiagnosticNote,
    DiagnosticNoteKind, SeverityKind,
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
    let diagnostic = match error {
        StandardLibraryLoadError::Read { path, kind } => Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::StandardLibraryArtifactReadFailed,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::file_path(path))
        .with_arg(DiagnosticArg::io_error_kind(DiagnosticIoErrorKind::from(
            kind,
        ))),
        StandardLibraryLoadError::Manifest(_) => Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::StandardLibraryManifestInvalid,
            SeverityKind::Error,
        ),
        StandardLibraryLoadError::ArtifactLengthMismatch {
            path,
            expected,
            actual,
        } => Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::StandardLibraryArtifactLengthMismatch,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::file_path(path))
        .with_arg(DiagnosticArg::expected_byte_count(expected))
        .with_arg(DiagnosticArg::actual_byte_count(actual)),
        StandardLibraryLoadError::ArtifactDigestMismatch {
            path,
            expected,
            actual,
        } => Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::StandardLibraryArtifactDigestMismatch,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::file_path(path))
        .with_arg(DiagnosticArg::expected_artifact_digest(diagnostic_digest(
            expected,
        )))
        .with_arg(DiagnosticArg::actual_artifact_digest(diagnostic_digest(
            actual,
        ))),
        StandardLibraryLoadError::TargetUnavailable(target) => Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::StandardLibraryTargetUnavailable,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::referenced_name(target.as_str())),
        StandardLibraryLoadError::RuntimeAbiMismatch { target, .. } => Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::StandardLibraryRuntimeAbiMismatch,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::referenced_name(target.as_str())),
        StandardLibraryLoadError::Infrastructure => Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::StandardLibraryManifestInvalid,
            SeverityKind::Error,
        ),
    };

    DiagnosticBag::single(with_dependency_context(diagnostic, input))
}

const fn diagnostic_digest(digest: StandardLibraryArtifactDigest) -> DiagnosticArtifactDigest {
    DiagnosticArtifactDigest::new(DiagnosticArtifactDigestAlgorithm::Blake3, digest.bytes())
}

pub(super) fn implementation_body_diagnostics(input: &DependencyInterfaceInput) -> DiagnosticBag {
    let diagnostic = Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::InterfaceConstantCallableBodyUnavailable,
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
