use std::process::ExitCode;

use bray_diagnostics::DiagnosticBag;

/// Selects a successful exit unless the diagnostics contain an error.
pub fn exit_code_from_diagnostics(
    diagnostics: &DiagnosticBag,
) -> ExitCode {
    if diagnostics.has_errors() {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

#[cfg(test)]
mod tests {
    use std::process::ExitCode;

    use bray_diagnostics::{Diagnostic, DiagnosticBag, DiagnosticId, DiagnosticKind, SeverityKind};

    use super::exit_code_from_diagnostics;

    #[test]
    fn exit_code_comes_from_error_severity() {
        let warning = Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::LexicalInvalidCharacter,
            SeverityKind::Warning,
        );

        let error = Diagnostic::new(
            DiagnosticId::new(1),
            DiagnosticKind::SourceInvalidUtf8,
            SeverityKind::Error,
        );

        assert_eq!(
            exit_code_from_diagnostics(&DiagnosticBag::single(warning)),
            ExitCode::SUCCESS
        );

        assert_eq!(
            exit_code_from_diagnostics(&DiagnosticBag::single(error)),
            ExitCode::FAILURE
        );
    }
}
