use std::ffi::OsString;
use std::process::ExitCode;

use bray_compilation::Compilation;
use bray_diagnostics::DiagnosticBag;

use crate::command::DriverInvocation;

/// Structured result from running the Bray compiler driver.
#[derive(Debug)]
pub struct DriverRunResult {
    exit_code: ExitCode,
    diagnostics: DiagnosticBag,
}

impl DriverRunResult {
    const fn new(exit_code: ExitCode, diagnostics: DiagnosticBag) -> Self {
        Self {
            exit_code,
            diagnostics,
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
}

/// Runs the Bray compiler driver for the provided process arguments.
pub fn run(arguments: impl IntoIterator<Item = OsString>) -> ExitCode {
    run_result(arguments).exit_code()
}

/// Runs the Bray compiler driver and returns its structured outcome.
pub fn run_result(arguments: impl IntoIterator<Item = OsString>) -> DriverRunResult {
    let invocation = match DriverInvocation::try_from_arguments(arguments) {
        Ok(invocation) => invocation,
        Err(error) => return DriverRunResult::new(error.exit_code(), error.into_diagnostics()),
    };

    let request = match invocation.into_compilation_request() {
        Ok(request) => request,
        Err(diagnostics) => return DriverRunResult::new(ExitCode::FAILURE, diagnostics),
    };

    let compilation = match Compilation::build(request) {
        Ok(compilation) => compilation,
        Err(_) => return DriverRunResult::new(ExitCode::FAILURE, DiagnosticBag::new()),
    };

    let diagnostics = compilation.into_diagnostics();

    let exit_code = if diagnostics.has_errors() {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    };

    DriverRunResult::new(exit_code, diagnostics)
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::process::ExitCode;

    use bray_diagnostics::DiagnosticKind;

    use super::{run, run_result};
    use crate::test_support::{TemporaryFile, unique_temporary_directory};

    #[test]
    fn run_fails_when_final_compilation_diagnostics_have_errors() {
        let file = TemporaryFile::write("bad.bray", &[0xff]);

        assert_eq!(
            run([
                OsString::from("brayc"),
                OsString::from("check"),
                file.path().as_os_str().to_os_string(),
            ]),
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
            OsString::from("--cpu-count"),
            OsString::from("0"),
            OsString::from("check"),
            OsString::from("main.bray"),
        ]);

        assert_eq!(result.exit_code(), ExitCode::FAILURE);

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
}
