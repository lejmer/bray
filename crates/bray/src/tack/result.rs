use std::process::ExitCode;

use bray_diagnostics::DiagnosticBag;
use bray_tooling::OutputFormat;

/// Structured result from running Bray Tack.
#[derive(Debug)]
pub struct TackRunResult {
    exit_code: ExitCode,
    diagnostics: DiagnosticBag,
    output_format: OutputFormat,
    stdout: String,
    stderr: String,
}

impl TackRunResult {
    pub(crate) fn new(
        exit_code: ExitCode,
        diagnostics: DiagnosticBag,
        output_format: OutputFormat,
    ) -> Self {
        Self {
            exit_code,
            diagnostics,
            output_format,
            stdout: String::new(),
            stderr: String::new(),
        }
    }

    pub(crate) fn with_output(
        exit_code: ExitCode,
        diagnostics: DiagnosticBag,
        output_format: OutputFormat,
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

    pub(crate) fn prepend_stdout(&mut self, output: String) {
        if output.is_empty() {
            return;
        }

        self.stdout.insert_str(0, &output);
    }

    pub(crate) fn replace_stdout(&mut self, output: String) {
        self.stdout = output;
    }

    pub(crate) fn prepend_stderr(&mut self, output: &str) {
        self.stderr.insert_str(0, output);
    }

    pub(crate) fn clear_diagnostics(&mut self) {
        self.diagnostics = DiagnosticBag::new();
    }

    pub(crate) const fn set_exit_code(&mut self, exit_code: ExitCode) {
        self.exit_code = exit_code;
    }

    pub(crate) fn diagnostic_groups(
        &self,
    ) -> impl Iterator<Item = (&DiagnosticBag, Option<&bray_source::SourceStore>)> {
        std::iter::once((&self.diagnostics, None))
    }

    /// Returns the process exit code selected by Bray Tack.
    pub const fn exit_code(&self) -> ExitCode {
        self.exit_code
    }

    /// Returns structured diagnostics produced by the command.
    pub const fn diagnostics(&self) -> &DiagnosticBag {
        &self.diagnostics
    }

    /// Returns the selected diagnostic output format.
    pub const fn output_format(&self) -> OutputFormat {
        self.output_format
    }

    /// Returns command-owned standard output.
    pub fn stdout(&self) -> &str {
        &self.stdout
    }

    /// Returns command-owned standard error.
    pub fn stderr(&self) -> &str {
        &self.stderr
    }
}
