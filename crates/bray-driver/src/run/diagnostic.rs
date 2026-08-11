use std::io;
use std::path::Path;

use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticId, DiagnosticIoErrorKind, DiagnosticKind, SeverityKind,
};

pub(super) fn artifact_write_failure(
    id: DiagnosticId,
    path: &Path,
    kind: io::ErrorKind,
) -> Diagnostic {
    Diagnostic::new(
        id,
        DiagnosticKind::EmissionArtifactWriteFailed,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::file_path(path))
    .with_arg(DiagnosticArg::io_error_kind(DiagnosticIoErrorKind::from(
        kind,
    )))
}
