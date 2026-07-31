use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticKind, SeverityKind,
};

pub(crate) fn selection_diagnostics(selection: impl Into<String>) -> DiagnosticBag {
    diagnostic(DiagnosticKind::ProjectCommandSelectionInvalid, selection)
}

pub(crate) fn operation_diagnostics(operation: impl Into<String>) -> DiagnosticBag {
    diagnostic(DiagnosticKind::ProjectCommandFailed, operation)
}

fn diagnostic(kind: DiagnosticKind, value: impl Into<String>) -> DiagnosticBag {
    DiagnosticBag::single(
        Diagnostic::new(DiagnosticId::new(0), kind, SeverityKind::Error)
            .with_arg(DiagnosticArg::referenced_name(value)),
    )
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticKind;

    use super::{operation_diagnostics, selection_diagnostics};

    #[test]
    fn project_command_failures_keep_structured_operation_categories() {
        let cases = [
            (
                selection_diagnostics("package"),
                DiagnosticKind::ProjectCommandSelectionInvalid,
            ),
            (
                operation_diagnostics("git_clone"),
                DiagnosticKind::ProjectCommandFailed,
            ),
        ];

        for (diagnostics, kind) in cases {
            assert_eq!(diagnostics.by_kind(kind).count(), 1);
        }
    }
}
