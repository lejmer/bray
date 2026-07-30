use std::process::ExitCode;

use bray_compilation::Compilation;
use bray_diagnostics::{Diagnostic, DiagnosticBag, DiagnosticKind};

use crate::DriverOutputFormat;

/// Compilation-aware diagnostic collection produced by Bray Tack.
#[derive(Debug, Default)]
pub struct TackDiagnostics {
    groups: Vec<TackDiagnosticGroup>,
}

impl TackDiagnostics {
    /// Returns whether no command or compilation produced diagnostics.
    pub fn is_empty(&self) -> bool {
        self.groups
            .iter()
            .all(|group| group.diagnostics.is_empty())
    }

    /// Returns whether any command or compilation produced an error.
    pub fn has_errors(&self) -> bool {
        self.groups
            .iter()
            .any(|group| group.diagnostics.has_errors())
    }

    /// Iterates diagnostics matching one stable category without merging compilation identities.
    pub fn by_kind(
        &self,
        kind: DiagnosticKind,
    ) -> impl Iterator<Item = &Diagnostic> {
        self.groups
            .iter()
            .flat_map(move |group| group.diagnostics.by_kind(kind))
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

        self.groups.push(TackDiagnosticGroup {
            diagnostics,
            compilation: None,
        });
    }

    pub(crate) fn push_compilation(
        &mut self,
        compilation: Compilation,
        diagnostics: DiagnosticBag,
    ) {
        if diagnostics.is_empty() {
            return;
        }

        self.groups.push(TackDiagnosticGroup {
            diagnostics,
            compilation: Some(compilation),
        });
    }

    pub(crate) fn groups(
        &self,
    ) -> impl Iterator<
        Item = (&DiagnosticBag, Option<&bray_source::SourceStore>),
    > {
        self.groups.iter().map(|group| {
            (
                &group.diagnostics,
                group.compilation.as_ref().map(Compilation::sources),
            )
        })
    }
}

#[derive(Debug)]
struct TackDiagnosticGroup {
    diagnostics: DiagnosticBag,
    compilation: Option<Compilation>,
}

/// Structured result from running Bray Tack.
#[derive(Debug)]
pub struct TackRunResult {
    exit_code: ExitCode,
    diagnostics: TackDiagnostics,
    output_format: DriverOutputFormat,
    stdout: String,
    stderr: String,
}

impl TackRunResult {
    pub(crate) fn new(
        exit_code: ExitCode,
        diagnostics: DiagnosticBag,
        output_format: DriverOutputFormat,
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
        output_format: DriverOutputFormat,
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

    pub(crate) fn add_unscoped_diagnostics(&mut self, diagnostics: DiagnosticBag) {
        self.diagnostics.push_unscoped(diagnostics);
    }

    pub(crate) fn add_compilation_diagnostics(
        &mut self,
        compilation: Compilation,
        diagnostics: DiagnosticBag,
    ) {
        self.diagnostics
            .push_compilation(compilation, diagnostics);
    }

    pub(crate) fn select_diagnostic_exit_code(&mut self) {
        self.exit_code = if self.diagnostics.has_errors() {
            ExitCode::FAILURE
        } else {
            ExitCode::SUCCESS
        };
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
    pub const fn output_format(&self) -> DriverOutputFormat {
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
