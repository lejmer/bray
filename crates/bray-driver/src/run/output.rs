use std::io::{self, Write};
use std::path::PathBuf;

use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticDocumentParseKind, DiagnosticId,
    DiagnosticIoErrorKind, DiagnosticKind, DiagnosticProjectCommandFailure,
    DiagnosticProjectOperation, SeverityKind,
};
use bray_messages::CompilerProfileMessageRenderer;
use bray_tooling::{OutputFormat, write_diagnostics};

use super::DriverRunResult;

pub(crate) enum DriverOutputError {
    InspectionReport { path: PathBuf, kind: io::ErrorKind },
    CompilerProfile { path: PathBuf, kind: io::ErrorKind },
    ProfileSerialization,
    StandardOutput(io::ErrorKind),
    StandardError(io::ErrorKind),
    ProfileSummary(io::ErrorKind),
    DiagnosticOutput(io::ErrorKind),
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
        let mut bytes =
            serde_json::to_vec(profile).map_err(|_| DriverOutputError::ProfileSerialization)?;

        bytes.push(b'\n');

        std::fs::write(path, bytes).map_err(|error| DriverOutputError::CompilerProfile {
            path: path.to_path_buf(),
            kind: error.kind(),
        })?;
    }

    stdout
        .write_all(result.stdout().as_bytes())
        .map_err(|error| DriverOutputError::StandardOutput(error.kind()))?;

    stderr
        .write_all(result.stderr().as_bytes())
        .map_err(|error| DriverOutputError::StandardError(error.kind()))?;

    if let Some(profile) = result.profile() {
        write_profile_summary(profile, stderr)?;
    }

    if result.has_terminal_output() {
        return Ok(());
    }

    if result.output_format() == OutputFormat::Json && !result.published_artifacts().is_empty() {
        return write_build_json(result, stdout, stderr);
    }

    write_diagnostics(
        result.diagnostics(),
        result.sources(),
        result.output_format(),
        stdout,
        stderr,
    )
    .map_err(|error| DriverOutputError::DiagnosticOutput(error.kind()))
}

fn write_build_json(
    result: &DriverRunResult,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> Result<(), DriverOutputError> {
    let mut diagnostic_bytes = Vec::new();

    write_diagnostics(
        result.diagnostics(),
        result.sources(),
        OutputFormat::Json,
        &mut diagnostic_bytes,
        stderr,
    )
    .map_err(|error| DriverOutputError::DiagnosticOutput(error.kind()))?;

    let mut report = serde_json::from_slice::<serde_json::Value>(&diagnostic_bytes)
        .map_err(|_| DriverOutputError::DiagnosticOutput(std::io::ErrorKind::InvalidData))?;

    let Some(report) = report.as_object_mut() else {
        return Err(DriverOutputError::DiagnosticOutput(
            std::io::ErrorKind::InvalidData,
        ));
    };

    report.insert(
        String::from("artifacts"),
        serde_json::to_value(result.published_artifacts())
            .map_err(|_| DriverOutputError::DiagnosticOutput(std::io::ErrorKind::InvalidData))?,
    );

    serde_json::to_writer_pretty(&mut *stdout, &report).map_err(|error| {
        DriverOutputError::StandardOutput(
            error.io_error_kind().unwrap_or(std::io::ErrorKind::Other),
        )
    })?;

    stdout
        .write_all(b"\n")
        .map_err(|error| DriverOutputError::StandardOutput(error.kind()))
}

pub(crate) fn write_driver_output_error(
    error: DriverOutputError,
    output_format: OutputFormat,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> io::Result<()> {
    let (diagnostic, failed_stream) = driver_output_error_diagnostic(error, output_format);

    let fallback = DiagnosticBag::single(diagnostic);

    match failed_stream {
        FailedOutputStream::Neither => {
            write_diagnostics(&fallback, None, output_format, stdout, stderr)
        }
        FailedOutputStream::StandardOutput => {
            write_diagnostics(&fallback, None, OutputFormat::Text, &mut io::sink(), stderr)
        }
        FailedOutputStream::StandardError => {
            write_diagnostics(&fallback, None, OutputFormat::Text, &mut io::sink(), stdout)
        }
    }
}

fn driver_output_error_diagnostic(
    error: DriverOutputError,
    output_format: OutputFormat,
) -> (Diagnostic, FailedOutputStream) {
    match error {
        DriverOutputError::InspectionReport { path, kind } => (
            Diagnostic::new(
                DiagnosticId::new(0),
                DiagnosticKind::InspectionReportWriteFailed,
                SeverityKind::Error,
            )
            .with_arg(DiagnosticArg::project_command_failure(
                DiagnosticProjectCommandFailure::Io {
                    operation: DiagnosticProjectOperation::InspectionReportOutput,
                    path,
                    error: DiagnosticIoErrorKind::from(kind),
                },
            )),
            FailedOutputStream::Neither,
        ),
        DriverOutputError::CompilerProfile { path, kind } => (
            Diagnostic::new(
                DiagnosticId::new(0),
                DiagnosticKind::CompilerProfileWriteFailed,
                SeverityKind::Error,
            )
            .with_arg(DiagnosticArg::project_command_failure(
                DiagnosticProjectCommandFailure::Io {
                    operation: DiagnosticProjectOperation::CompilerProfileReportOutput,
                    path,
                    error: DiagnosticIoErrorKind::from(kind),
                },
            )),
            FailedOutputStream::Neither,
        ),
        DriverOutputError::ProfileSerialization => (
            DiagnosticProjectCommandFailure::Document {
                operation: DiagnosticProjectOperation::CompilerProfileSerialization,
                path: None,
                problem: DiagnosticDocumentParseKind::Serialization,
                detail: None,
            }
            .diagnostic(DiagnosticId::new(0)),
            FailedOutputStream::Neither,
        ),
        DriverOutputError::StandardOutput(kind) => (
            stream_failure_diagnostic(DiagnosticProjectOperation::StandardOutput, kind),
            FailedOutputStream::StandardOutput,
        ),
        DriverOutputError::StandardError(kind) => (
            stream_failure_diagnostic(DiagnosticProjectOperation::StandardError, kind),
            FailedOutputStream::StandardError,
        ),
        DriverOutputError::ProfileSummary(kind) => (
            stream_failure_diagnostic(DiagnosticProjectOperation::CompilerProfileSummary, kind),
            FailedOutputStream::StandardError,
        ),
        DriverOutputError::DiagnosticOutput(kind) => (
            stream_failure_diagnostic(DiagnosticProjectOperation::DiagnosticOutput, kind),
            match output_format {
                OutputFormat::Text => FailedOutputStream::StandardError,
                OutputFormat::Json => FailedOutputStream::StandardOutput,
            },
        ),
    }
}

#[derive(Clone, Copy)]
enum FailedOutputStream {
    Neither,
    StandardOutput,
    StandardError,
}

fn stream_failure_diagnostic(
    operation: DiagnosticProjectOperation,
    kind: io::ErrorKind,
) -> Diagnostic {
    DiagnosticProjectCommandFailure::HostIo {
        operation,
        error: DiagnosticIoErrorKind::from(kind),
    }
    .diagnostic(DiagnosticId::new(0))
}

fn write_profile_summary(
    profile: &bray_compilation::CompilationProfileReport,
    stderr: &mut impl Write,
) -> Result<(), DriverOutputError> {
    let renderer = CompilerProfileMessageRenderer::english();

    write!(stderr, "{}", renderer.summary(profile))
        .map_err(|error| DriverOutputError::ProfileSummary(error.kind()))
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::path::PathBuf;
    use std::process::ExitCode;

    use bray_diagnostics::{
        DiagnosticBag, DiagnosticKind, DiagnosticProjectCommandFailure, DiagnosticProjectOperation,
    };
    use bray_tooling::OutputFormat;

    use super::{
        DriverOutputError, driver_output_error_diagnostic, write_driver_output,
        write_driver_output_error,
    };
    use crate::run::DriverRunResult;

    #[test]
    fn build_json_reports_complete_stable_artifact_paths() {
        let result =
            DriverRunResult::new(ExitCode::SUCCESS, DiagnosticBag::new(), OutputFormat::Json)
                .with_published_artifacts(vec![PathBuf::from(
                    "build/native/debug/hello_world/application.exe",
                )]);

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        write_driver_output(&result, &mut stdout, &mut stderr)
            .unwrap_or_else(|_| panic!("build JSON must render"));

        let report: serde_json::Value = serde_json::from_slice(&stdout)
            .unwrap_or_else(|error| panic!("build JSON must parse: {error}"));

        assert_eq!(
            report["artifacts"][0],
            "build/native/debug/hello_world/application.exe"
        );
    }

    #[test]
    fn report_write_failures_preserve_the_operation_path_and_io_category() {
        let inspection_bag = report_write_failure_bag(
            DriverOutputError::InspectionReport {
                path: PathBuf::from("reports/inspection.json"),
                kind: io::ErrorKind::PermissionDenied,
            },
            DiagnosticProjectOperation::InspectionReportOutput,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &inspection_bag,
            DiagnosticKind::InspectionReportWriteFailed,
        );

        let profile_bag = report_write_failure_bag(
            DriverOutputError::CompilerProfile {
                path: PathBuf::from("profiles/compiler.json"),
                kind: io::ErrorKind::WriteZero,
            },
            DiagnosticProjectOperation::CompilerProfileReportOutput,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &profile_bag,
            DiagnosticKind::CompilerProfileWriteFailed,
        );
    }

    fn report_write_failure_bag(
        error: DriverOutputError,
        expected_operation: DiagnosticProjectOperation,
    ) -> bray_diagnostics::DiagnosticBag {
        let (diagnostic, _) = driver_output_error_diagnostic(error, OutputFormat::Json);

        let [argument] = diagnostic.args() else {
            panic!("report-write diagnostics must carry one structured failure");
        };

        let bray_diagnostics::DiagnosticArgValue::ProjectCommandFailure(
            DiagnosticProjectCommandFailure::Io {
                operation,
                path: _,
                error: _,
            },
        ) = argument.value()
        else {
            panic!("report-write diagnostics must preserve an exact file operation");
        };

        assert_eq!(*operation, expected_operation);

        bray_diagnostics::DiagnosticBag::single(diagnostic)
    }

    #[test]
    fn terminal_stream_failures_use_the_surviving_stream_without_recursion() {
        let cases = [
            (
                DriverOutputError::StandardOutput(io::ErrorKind::PermissionDenied),
                true,
                "write standard output",
            ),
            (
                DriverOutputError::StandardError(io::ErrorKind::BrokenPipe),
                false,
                "write standard error",
            ),
            (
                DriverOutputError::DiagnosticOutput(io::ErrorKind::WriteZero),
                true,
                "write compiler diagnostics",
            ),
        ];

        for (error, output_failed, expected) in cases {
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();

            write_driver_output_error(
                error,
                if output_failed {
                    OutputFormat::Json
                } else {
                    OutputFormat::Text
                },
                &mut stdout,
                &mut stderr,
            )
            .unwrap_or_else(|failure| panic!("fallback output should write: {failure:?}"));

            let surviving = if output_failed { &stderr } else { &stdout };
            let text = String::from_utf8_lossy(surviving);

            assert!(text.contains(expected));
        }
    }
}
