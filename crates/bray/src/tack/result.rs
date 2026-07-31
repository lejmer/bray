use std::process::ExitCode;

use bray_diagnostics::{Diagnostic, DiagnosticBag, DiagnosticKind};
use bray_tooling::OutputFormat;

/// Structured project-command diagnostics produced directly by Bray Tack.
#[derive(Debug, Default)]
pub struct TackDiagnostics {
    groups: Vec<DiagnosticBag>,
}

impl TackDiagnostics {
    /// Returns whether no command or compilation produced diagnostics.
    pub fn is_empty(&self) -> bool {
        self.groups
            .iter()
            .all(DiagnosticBag::is_empty)
    }

    /// Returns whether any command or compilation produced an error.
    pub fn has_errors(&self) -> bool {
        self.groups
            .iter()
            .any(DiagnosticBag::has_errors)
    }

    /// Iterates diagnostics matching one stable category without merging compilation identities.
    pub fn by_kind(
        &self,
        kind: DiagnosticKind,
    ) -> impl Iterator<Item = &Diagnostic> {
        self.groups
            .iter()
            .flat_map(move |group| group.by_kind(kind))
    }

    pub(crate) fn from_unscoped(diagnostics: DiagnosticBag) -> Self {
        let mut result = Self::default();

        result.push_unscoped(diagnostics);

        result
    }

    pub(crate) fn push_unscoped(&mut self, diagnostics: DiagnosticBag) {
        if diagnostics.is_empty() {
            return;
        }

        self.groups.push(diagnostics);
    }

    pub(crate) fn groups(
        &self,
    ) -> impl Iterator<
        Item = (&DiagnosticBag, Option<&bray_source::SourceStore>),
    > {
        self.groups.iter().map(|diagnostics| (diagnostics, None))
    }
}

/// Structured result from running Bray Tack.
#[derive(Debug)]
pub struct TackRunResult {
    exit_code: ExitCode,
    diagnostics: TackDiagnostics,
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
            diagnostics: TackDiagnostics::from_unscoped(diagnostics),
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
            diagnostics: TackDiagnostics::from_unscoped(diagnostics),
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

    pub(crate) const fn set_exit_code(&mut self, exit_code: ExitCode) {
        self.exit_code = exit_code;
    }

    pub(crate) fn diagnostic_groups(
        &self,
    ) -> impl Iterator<
        Item = (&DiagnosticBag, Option<&bray_source::SourceStore>),
    > {
        self.diagnostics.groups()
    }

    /// Returns the process exit code selected by Bray Tack.
    pub const fn exit_code(&self) -> ExitCode {
        self.exit_code
    }

    /// Returns structured diagnostics produced by the command.
    pub const fn diagnostics(&self) -> &TackDiagnostics {
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
