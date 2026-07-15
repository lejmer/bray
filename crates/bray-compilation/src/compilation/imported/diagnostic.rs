use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticKind, DiagnosticNote,
    DiagnosticNoteKind, SeverityKind,
};
use bray_package_interface::InterfaceValidationError;

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

fn with_dependency_context(
    mut diagnostic: Diagnostic,
    input: &DependencyInterfaceInput,
) -> Diagnostic {
    let note = DiagnosticNote::new(DiagnosticNoteKind::InterfaceDependencyContext)
        .with_arg(DiagnosticArg::expected_package_identity(
            input.package().as_str(),
        ))
        .with_arg(DiagnosticArg::expected_product_identity(
            input.product().as_str(),
        ))
        .with_arg(DiagnosticArg::artifact_path(input.artifact_path()));

    if let Some(span) = input.dependency_span() {
        diagnostic = diagnostic.with_primary_span(span);
    }

    diagnostic.with_note(note)
}
