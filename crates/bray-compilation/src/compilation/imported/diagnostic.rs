use bray_diagnostics::{Diagnostic, DiagnosticBag, DiagnosticId, DiagnosticKind, SeverityKind};
use bray_package_interface::InterfaceValidationError;

pub(super) fn validation_diagnostics(error: InterfaceValidationError) -> DiagnosticBag {
    DiagnosticBag::single(error.into_diagnostic(DiagnosticId::new(0)))
}

pub(super) fn interface_diagnostics(kind: DiagnosticKind) -> DiagnosticBag {
    DiagnosticBag::single(Diagnostic::new(
        DiagnosticId::new(0),
        kind,
        SeverityKind::Error,
    ))
}
