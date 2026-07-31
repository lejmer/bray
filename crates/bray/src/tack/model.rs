use std::ffi::OsString;
use std::path::{Path, PathBuf};

use bray_tooling::OutputFormat;

/// Stable category for one Bray Tack command.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TackCommandKind {
    /// Checks selected project products.
    Check,
    /// Builds selected project products.
    Build,
    /// Builds and runs one executable product.
    Run,
    /// Builds and runs selected test products.
    Test,
    /// Formats project or standard-input source.
    Format,
    /// Inspects the project graph or one compiler fact.
    Inspect,
    /// Runs the Bray language-server tool.
    LanguageServer,
    /// Explicitly installs one Git repository in the vendored tree.
    VendorInstall,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct TackSelection {
    pub(crate) package: Option<String>,
    pub(crate) product: Option<String>,
    pub(crate) target: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TackInspection {
    Project,
    Source,
    Tokens,
    Syntax,
    Declarations,
    Symbols,
    Bound,
    Lowered,
    Mir,
}

impl TackInspection {
    pub(crate) const fn command_text(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Source => "source",
            Self::Tokens => "tokens",
            Self::Syntax => "syntax",
            Self::Declarations => "declarations",
            Self::Symbols => "symbols",
            Self::Bound => "bound",
            Self::Lowered => "lowered",
            Self::Mir => "mir",
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum TackCommand {
    Check(TackSelection),
    Build(TackSelection),
    Run {
        selection: TackSelection,
        arguments: Vec<OsString>,
    },
    Test {
        selection: TackSelection,
        arguments: Vec<OsString>,
    },
    Format {
        check: bool,
        files: Vec<PathBuf>,
    },
    Inspect {
        selection: TackSelection,
        inspection: TackInspection,
        source_id: u32,
        position: Option<u32>,
    },
    LanguageServer {
        target: Option<String>,
    },
    VendorInstall {
        name: String,
        repository: String,
    },
}

impl TackCommand {
    pub(crate) const fn kind(&self) -> TackCommandKind {
        match self {
            Self::Check(_) => TackCommandKind::Check,
            Self::Build(_) => TackCommandKind::Build,
            Self::Run { .. } => TackCommandKind::Run,
            Self::Test { .. } => TackCommandKind::Test,
            Self::Format { .. } => TackCommandKind::Format,
            Self::Inspect { .. } => TackCommandKind::Inspect,
            Self::LanguageServer { .. } => TackCommandKind::LanguageServer,
            Self::VendorInstall { .. } => TackCommandKind::VendorInstall,
        }
    }
}

/// Parsed Bray Tack invocation.
#[derive(Debug, Eq, PartialEq)]
pub struct TackInvocation {
    workspace_root: PathBuf,
    worker_count: usize,
    output_format: OutputFormat,
    command: TackCommand,
}

impl TackInvocation {
    pub(crate) const fn new(
        workspace_root: PathBuf,
        worker_count: usize,
        output_format: OutputFormat,
        command: TackCommand,
    ) -> Self {
        Self {
            workspace_root,
            worker_count,
            output_format,
            command,
        }
    }

    /// Returns the exact workspace root selected by this invocation.
    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    /// Returns the maximum compiler worker count supplied to child tools.
    pub const fn worker_count(&self) -> usize {
        self.worker_count
    }

    /// Returns the selected output format.
    pub const fn output_format(&self) -> OutputFormat {
        self.output_format
    }

    /// Returns the stable command category.
    pub const fn command_kind(&self) -> TackCommandKind {
        self.command.kind()
    }

    pub(crate) fn into_parts(self) -> (PathBuf, usize, OutputFormat, TackCommand) {
        (
            self.workspace_root,
            self.worker_count,
            self.output_format,
            self.command,
        )
    }
}
