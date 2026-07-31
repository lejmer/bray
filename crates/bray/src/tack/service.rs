use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::io::{Read, Write};

use bray_compilation::WorkerBudget;
use bray_diagnostics::DiagnosticBag;
use bray_project::ProjectGraph;
use bray_target::TargetIdentity;

/// Formatter write policy selected by `bray fmt`.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TackFormatMode {
    /// Check formatting without modifying files.
    Check,
    /// Publish formatting changes to selected files.
    Write,
}

/// Exact formatter input selected by Bray Tack.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TackFormatInput {
    /// Project-owned or explicitly supplied source files.
    Files(Vec<PathBuf>),
    /// Bytes read from standard input.
    StandardInput(Vec<u8>),
}

/// Narrow project-facing formatter request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TackFormatRequest {
    workspace_root: PathBuf,
    mode: TackFormatMode,
    input: TackFormatInput,
}

impl TackFormatRequest {
    pub(crate) const fn new(
        workspace_root: PathBuf,
        mode: TackFormatMode,
        input: TackFormatInput,
    ) -> Self {
        Self {
            workspace_root,
            mode,
            input,
        }
    }

    /// Returns the selected workspace root.
    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    /// Returns whether the formatter checks or writes source.
    pub const fn mode(&self) -> TackFormatMode {
        self.mode
    }

    /// Returns the exact selected formatter input.
    pub const fn input(&self) -> &TackFormatInput {
        &self.input
    }
}

/// Request supplied to the language-server integration boundary.
#[derive(Clone, Debug)]
pub struct TackLanguageServerRequest {
    workspace_root: PathBuf,
    graph: Arc<ProjectGraph>,
    target: TargetIdentity,
    worker_budget: WorkerBudget,
}

impl TackLanguageServerRequest {
    pub(crate) const fn new(
        workspace_root: PathBuf,
        graph: Arc<ProjectGraph>,
        target: TargetIdentity,
        worker_budget: WorkerBudget,
    ) -> Self {
        Self {
            workspace_root,
            graph,
            target,
            worker_budget,
        }
    }

    /// Returns the selected workspace root.
    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    /// Returns the already validated immutable project graph.
    pub fn graph(&self) -> &ProjectGraph {
        &self.graph
    }

    /// Returns the compiler-owned CPU worker budget.
    pub const fn worker_budget(&self) -> WorkerBudget {
        self.worker_budget
    }

    /// Returns the exact project-selected compilation target.
    pub const fn target(&self) -> &TargetIdentity {
        &self.target
    }

    pub(crate) fn into_parts(
        self,
    ) -> (PathBuf, Arc<ProjectGraph>, TargetIdentity, WorkerBudget) {
        (
            self.workspace_root,
            self.graph,
            self.target,
            self.worker_budget,
        )
    }
}

/// Structured result returned by a linked Bray Tack service.
#[derive(Debug)]
pub struct TackServiceResult {
    exit_code: ExitCode,
    diagnostics: DiagnosticBag,
    stdout: String,
    stderr: String,
}

impl TackServiceResult {
    /// Creates a service result from its complete observable output.
    pub const fn new(
        exit_code: ExitCode,
        diagnostics: DiagnosticBag,
        stdout: String,
        stderr: String,
    ) -> Self {
        Self {
            exit_code,
            diagnostics,
            stdout,
            stderr,
        }
    }

    pub(crate) fn into_parts(self) -> (ExitCode, DiagnosticBag, String, String) {
        (
            self.exit_code,
            self.diagnostics,
            self.stdout,
            self.stderr,
        )
    }
}

/// Formatter implementation linked into Bray Tack.
pub trait TackFormatService {
    /// Formats the exact project or standard-input selection.
    fn format(&self, request: TackFormatRequest) -> TackServiceResult;
}

/// Language-server implementation linked into Bray Tack.
pub trait TackLanguageServerService {
    /// Runs language tooling over the supplied immutable project graph.
    fn run(
        &self,
        request: TackLanguageServerRequest,
        input: Box<dyn Read + Send>,
        output: &mut dyn Write,
    ) -> TackServiceResult;
}

/// Optional tool implementations linked into the Bray Tack executable.
#[derive(Clone, Copy, Default)]
pub struct TackServices<'service> {
    formatter: Option<&'service dyn TackFormatService>,
    language_server: Option<&'service dyn TackLanguageServerService>,
}

impl<'service> TackServices<'service> {
    /// Creates an empty service set.
    pub const fn new() -> Self {
        Self {
            formatter: None,
            language_server: None,
        }
    }

    /// Returns a service set using the supplied formatter.
    pub const fn with_formatter(
        mut self,
        formatter: &'service dyn TackFormatService,
    ) -> Self {
        self.formatter = Some(formatter);

        self
    }

    /// Returns a service set using the supplied language server.
    pub const fn with_language_server(
        mut self,
        language_server: &'service dyn TackLanguageServerService,
    ) -> Self {
        self.language_server = Some(language_server);

        self
    }

    pub(crate) const fn formatter(self) -> Option<&'service dyn TackFormatService> {
        self.formatter
    }

    pub(crate) const fn language_server(
        self,
    ) -> Option<&'service dyn TackLanguageServerService> {
        self.language_server
    }
}
