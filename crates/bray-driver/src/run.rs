use std::ffi::OsString;
use std::io;
use std::process::ExitCode;

use bray_compilation::Compilation;
use bray_diagnostics::DiagnosticBag;

use crate::command::DriverInvocation;
use crate::command::DriverOutputFormat;
use crate::diagnostic_output::write_driver_output;
use crate::exit_status::exit_code_from_diagnostics;

/// Structured result from running the Bray compiler driver.
#[derive(Debug)]
pub struct DriverRunResult {
    exit_code: ExitCode,
    diagnostics: DiagnosticBag,
    output_format: DriverOutputFormat,
    stdout: String,
    stderr: String,
}

impl DriverRunResult {
    fn new(
        exit_code: ExitCode,
        diagnostics: DiagnosticBag,
        output_format: DriverOutputFormat,
    ) -> Self {
        Self::with_output(
            exit_code,
            diagnostics,
            output_format,
            String::new(),
            String::new(),
        )
    }

    fn with_output(
        exit_code: ExitCode,
        diagnostics: DiagnosticBag,
        output_format: DriverOutputFormat,
        stdout: String,
        stderr: String,
    ) -> Self {
        Self {
            exit_code,
            diagnostics,
            output_format,
            stdout,
            stderr,
        }
    }

    /// Returns the process exit code selected by the driver.
    pub const fn exit_code(&self) -> ExitCode {
        self.exit_code
    }

    /// Returns the final diagnostics produced by the driver operation.
    pub const fn diagnostics(&self) -> &DiagnosticBag {
        &self.diagnostics
    }

    /// Returns the output format selected for driver-produced output.
    pub const fn output_format(&self) -> DriverOutputFormat {
        self.output_format
    }

    /// Returns driver-owned stdout text, such as help or version output.
    pub fn stdout(&self) -> &str {
        &self.stdout
    }

    /// Returns driver-owned stderr text, such as command-line parser errors.
    pub fn stderr(&self) -> &str {
        &self.stderr
    }

    /// Returns whether this result carries driver-owned terminal output.
    pub fn has_terminal_output(&self) -> bool {
        !self.stdout.is_empty() || !self.stderr.is_empty()
    }
}

/// Runs the Bray compiler driver for the provided process arguments.
pub fn run(arguments: impl IntoIterator<Item = OsString>) -> ExitCode {
    let mut stdout = io::stdout().lock();
    let mut stderr = io::stderr().lock();

    run_with_writers(arguments, &mut stdout, &mut stderr)
}

/// Runs the Bray compiler driver and returns its structured outcome.
pub fn run_result(arguments: impl IntoIterator<Item = OsString>) -> DriverRunResult {
    let invocation = match DriverInvocation::try_from_arguments(arguments) {
        Ok(invocation) => invocation,
        Err(error) => {
            let exit_code = error.exit_code();
            let output_format = error.output_format();

            let (diagnostics, stdout, stderr) = error.into_diagnostics_and_output();

            return DriverRunResult::with_output(
                exit_code,
                diagnostics,
                output_format,
                stdout,
                stderr,
            );
        }
    };

    let output_format = invocation.options().output_format();

    let request = match invocation.into_compilation_request() {
        Ok(request) => request,
        Err(diagnostics) => {
            let exit_code = exit_code_from_diagnostics(&diagnostics);

            return DriverRunResult::new(exit_code, diagnostics, output_format);
        }
    };

    let compilation = match Compilation::build(request) {
        Ok(compilation) => compilation,
        Err(_) => {
            return DriverRunResult::new(ExitCode::FAILURE, DiagnosticBag::new(), output_format);
        }
    };

    let diagnostics = compilation.into_diagnostics();
    let exit_code = exit_code_from_diagnostics(&diagnostics);

    DriverRunResult::new(exit_code, diagnostics, output_format)
}

fn run_with_writers(
    arguments: impl IntoIterator<Item = OsString>,
    stdout: &mut impl io::Write,
    stderr: &mut impl io::Write,
) -> ExitCode {
    let result = run_result(arguments);

    match write_driver_output(&result, stdout, stderr) {
        Ok(()) => result.exit_code(),
        Err(_) => ExitCode::FAILURE,
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::process::ExitCode;

    use bray_diagnostics::DiagnosticKind;

    use super::{run_result, run_with_writers};
    use crate::test_support::{TemporaryFile, unique_temporary_directory};

    #[test]
    fn run_fails_when_final_compilation_diagnostics_have_errors() {
        let file = TemporaryFile::write("bad.bray", &[0xff]);

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        assert_eq!(
            run_with_writers(
                [
                    OsString::from("brayc"),
                    OsString::from("check"),
                    file.path().as_os_str().to_os_string(),
                ],
                &mut stdout,
                &mut stderr,
            ),
            ExitCode::FAILURE
        );
    }

    #[test]
    fn run_result_carries_final_compilation_diagnostics() {
        let file = TemporaryFile::write("bad.bray", &[0xff]);

        let result = run_result([
            OsString::from("brayc"),
            OsString::from("check"),
            file.path().as_os_str().to_os_string(),
        ]);

        assert_eq!(result.exit_code(), ExitCode::FAILURE);

        assert_eq!(
            result
                .diagnostics()
                .by_kind(DiagnosticKind::SourceInvalidUtf8)
                .count(),
            1
        );
    }

    #[test]
    fn run_result_carries_invalid_worker_budget_diagnostics() {
        let result = run_result([
            OsString::from("brayc"),
            OsString::from("--format"),
            OsString::from("json"),
            OsString::from("--cpu-count"),
            OsString::from("0"),
            OsString::from("check"),
            OsString::from("main.bray"),
        ]);

        assert_eq!(result.exit_code(), ExitCode::FAILURE);
        assert_eq!(result.output_format(), crate::DriverOutputFormat::Json);

        assert_eq!(
            result
                .diagnostics()
                .by_kind(DiagnosticKind::RequestInvalidWorkerBudget)
                .count(),
            1
        );
    }

    #[test]
    fn run_result_carries_missing_source_input_diagnostics() {
        let result = run_result([OsString::from("brayc"), OsString::from("check")]);

        assert_eq!(result.exit_code(), ExitCode::FAILURE);

        assert_eq!(
            result
                .diagnostics()
                .by_kind(DiagnosticKind::RequestMissingSourceInput)
                .count(),
            1
        );
    }

    #[test]
    fn run_result_carries_file_read_diagnostics() {
        let missing_path = unique_temporary_directory().join("missing.bray");

        let result = run_result([
            OsString::from("brayc"),
            OsString::from("check"),
            missing_path.as_os_str().to_os_string(),
        ]);

        assert_eq!(result.exit_code(), ExitCode::FAILURE);

        assert_eq!(
            result
                .diagnostics()
                .by_kind(DiagnosticKind::SourceFileReadFailed)
                .count(),
            1
        );
    }

    #[test]
    fn run_writes_help_to_stdout() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let exit_code = run_with_writers(
            [OsString::from("brayc"), OsString::from("--help")],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(exit_code, ExitCode::SUCCESS);
        assert!(stderr.is_empty());

        let stdout = match String::from_utf8(stdout) {
            Ok(stdout) => stdout,
            Err(error) => panic!("stdout should be UTF-8: {error:?}"),
        };

        assert!(stdout.contains("The Bray compiler"));
        assert!(stdout.contains("\x1b[36mUsage:\x1b[0m"));
        assert!(stdout.contains("Usage:"));
        assert!(stdout.contains("check"));
        assert!(stdout.contains("--version"));
    }

    #[test]
    fn run_writes_subcommand_help_to_stdout() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let exit_code = run_with_writers(
            [
                OsString::from("brayc"),
                OsString::from("check"),
                OsString::from("--help"),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(exit_code, ExitCode::SUCCESS);
        assert!(stderr.is_empty());

        let stdout = match String::from_utf8(stdout) {
            Ok(stdout) => stdout,
            Err(error) => panic!("stdout should be UTF-8: {error:?}"),
        };

        assert!(stdout.contains("Usage:"));
        assert!(stdout.contains("FILE"));
    }

    #[test]
    fn run_writes_help_to_stdout_when_no_arguments_are_provided() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let exit_code = run_with_writers([OsString::from("brayc")], &mut stdout, &mut stderr);

        assert_eq!(exit_code, ExitCode::SUCCESS);
        assert!(stderr.is_empty());
        assert!(!stdout.is_empty());
    }

    #[test]
    fn run_writes_version_to_stdout() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let exit_code = run_with_writers(
            [OsString::from("brayc"), OsString::from("--version")],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(exit_code, ExitCode::SUCCESS);
        assert!(stderr.is_empty());

        let stdout = match String::from_utf8(stdout) {
            Ok(stdout) => stdout,
            Err(error) => panic!("stdout should be UTF-8: {error:?}"),
        };

        assert_eq!(
            stdout.trim(),
            format!("brayc {}", env!("CARGO_PKG_VERSION"))
        );
    }

    #[test]
    fn run_writes_clap_errors_to_stderr() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let exit_code = run_with_writers(
            [OsString::from("brayc"), OsString::from("--unknown")],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(exit_code, ExitCode::FAILURE);
        assert!(stdout.is_empty());

        let stderr = match String::from_utf8(stderr) {
            Ok(stderr) => stderr,
            Err(error) => panic!("stderr should be UTF-8: {error:?}"),
        };

        assert!(stderr.contains("\x1b[31merror:\x1b[0m"));
    }

    #[test]
    fn run_writes_text_diagnostics_to_stderr() {
        let file = TemporaryFile::write("bad.bray", &[0xff]);

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let exit_code = run_with_writers(
            [
                OsString::from("brayc"),
                OsString::from("check"),
                file.path().as_os_str().to_os_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(exit_code, ExitCode::FAILURE);
        assert!(stdout.is_empty());

        let stderr = match String::from_utf8(stderr) {
            Ok(stderr) => stderr,
            Err(error) => panic!("stderr should be UTF-8: {error:?}"),
        };

        assert!(stderr.contains("source_invalid_utf8"));
    }

    #[test]
    fn run_writes_json_diagnostics_to_stdout() {
        let file = TemporaryFile::write("bad.bray", &[0xff]);

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let exit_code = run_with_writers(
            [
                OsString::from("brayc"),
                OsString::from("--format"),
                OsString::from("json"),
                OsString::from("check"),
                file.path().as_os_str().to_os_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(exit_code, ExitCode::FAILURE);
        assert!(stderr.is_empty());

        let stdout = match String::from_utf8(stdout) {
            Ok(stdout) => stdout,
            Err(error) => panic!("stdout should be UTF-8: {error:?}"),
        };

        assert!(stdout.contains("\"kind\": \"source_invalid_utf8\""));
        assert!(!stdout.contains("source input contains invalid UTF-8 at byte offset"));
    }
}
