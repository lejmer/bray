use std::io::{self, Write};
use std::path::PathBuf;

use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticIoErrorKind, DiagnosticKind,
    SeverityKind,
};
use bray_messages::CompilerProfileMessageRenderer;
use bray_tooling::{OutputFormat, write_diagnostics};

use super::DriverRunResult;

pub(crate) enum DriverOutputError {
    InspectionReport { path: PathBuf, kind: io::ErrorKind },
    CompilerProfile { path: PathBuf, kind: io::ErrorKind },
    Terminal,
}

pub(crate) fn write_driver_output(
    result: &DriverRunResult,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> Result<(), DriverOutputError> {
    if let Some(path) = result.report_file() {
        std::fs::write(path, result.stdout().as_bytes()).map_err(|error| {
            DriverOutputError::InspectionReport {
                path: path.to_path_buf(),
                kind: error.kind(),
            }
        })?;
    }

    if let (Some(profile), Some(path)) = (result.profile(), result.profile_output()) {
        let mut bytes = serde_json::to_vec(profile).map_err(|_| DriverOutputError::Terminal)?;

        bytes.push(b'\n');

        std::fs::write(path, bytes).map_err(|error| DriverOutputError::CompilerProfile {
            path: path.to_path_buf(),
            kind: error.kind(),
        })?;
    }

    stdout
        .write_all(result.stdout().as_bytes())
        .map_err(|_| DriverOutputError::Terminal)?;

    stderr
        .write_all(result.stderr().as_bytes())
        .map_err(|_| DriverOutputError::Terminal)?;

    if let Some(profile) = result.profile() {
        write_profile_summary(profile, stderr)?;
    }

    if result.has_terminal_output() {
        return Ok(());
    }

    write_diagnostics(
        result.diagnostics(),
        result.sources(),
        result.output_format(),
        stdout,
        stderr,
    )
    .map_err(|_| DriverOutputError::Terminal)
}

pub(crate) fn write_driver_output_error(
    error: DriverOutputError,
    output_format: OutputFormat,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> io::Result<()> {
    let (diagnostic_kind, path, kind) = match error {
        DriverOutputError::InspectionReport { path, kind } => {
            (DiagnosticKind::InspectionReportWriteFailed, path, kind)
        }
        DriverOutputError::CompilerProfile { path, kind } => {
            (DiagnosticKind::CompilerProfileWriteFailed, path, kind)
        }
        DriverOutputError::Terminal => return Ok(()),
    };

    let diagnostic = Diagnostic::new(
        DiagnosticId::new(0),
        diagnostic_kind,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::file_path(path))
    .with_arg(DiagnosticArg::io_error_kind(DiagnosticIoErrorKind::from(
        kind,
    )));

    write_diagnostics(
        &DiagnosticBag::single(diagnostic),
        None,
        output_format,
        stdout,
        stderr,
    )
}

fn write_profile_summary(
    profile: &bray_compilation::CompilationProfileReport,
    stderr: &mut impl Write,
) -> Result<(), DriverOutputError> {
    let renderer = CompilerProfileMessageRenderer::english();

    write!(stderr, "{}", renderer.summary(profile)).map_err(|_| DriverOutputError::Terminal)
}
