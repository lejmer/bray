use std::ffi::OsString;
use std::path::{Path, PathBuf};

use bray_test_protocol::{TestCapturePolicy, TestDuration, TestTimeoutPolicy};
use bray_tooling::OutputFormat;

/// Stable category for one Bray Tack command.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TackCommandKind {
    /// Initializes a Bray workspace and root package.
    Init,
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

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TackBuildConfiguration {
    #[default]
    Debug,
    Release,
}

impl TackBuildConfiguration {
    pub(crate) const fn directory_name(self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Release => "release",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TackTestOptions {
    pub(crate) filters: Vec<String>,
    pub(crate) maximum_concurrency: usize,
    pub(crate) timeout: TestTimeoutPolicy,
    pub(crate) capture: TestCapturePolicy,
    pub(crate) show_output: bool,
}

impl TackTestOptions {
    pub(crate) fn new(
        filters: Vec<String>,
        maximum_concurrency: usize,
        timeout_milliseconds: Option<u64>,
        capture: TestCapturePolicy,
        show_output: bool,
    ) -> Self {
        let timeout = timeout_milliseconds.map_or(TestTimeoutPolicy::Unlimited, |milliseconds| {
            TestTimeoutPolicy::Limit(TestDuration::from_nanoseconds(
                milliseconds.saturating_mul(1_000_000),
            ))
        });

        Self {
            filters,
            maximum_concurrency,
            timeout,
            capture,
            show_output,
        }
    }
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
    Init {
        directory: Option<PathBuf>,
        package: Option<String>,
    },
    Check(TackSelection),
    Build {
        selection: TackSelection,
        configuration: TackBuildConfiguration,
    },
    Run {
        selection: TackSelection,
        configuration: TackBuildConfiguration,
        arguments: Vec<OsString>,
    },
    Test {
        selection: TackSelection,
        configuration: TackBuildConfiguration,
        options: TackTestOptions,
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
            Self::Init { .. } => TackCommandKind::Init,
            Self::Check(_) => TackCommandKind::Check,
            Self::Build { .. } => TackCommandKind::Build,
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
    toolchain_root: Option<PathBuf>,
    worker_count: usize,
    output_format: OutputFormat,
    verbose: bool,
    command: TackCommand,
}

impl TackInvocation {
    pub(crate) const fn new(
        workspace_root: PathBuf,
        toolchain_root: Option<PathBuf>,
        worker_count: usize,
        output_format: OutputFormat,
        verbose: bool,
        command: TackCommand,
    ) -> Self {
        Self {
            workspace_root,
            toolchain_root,
            worker_count,
            output_format,
            verbose,
            command,
        }
    }

    /// Returns the exact workspace root selected by this invocation.
    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    /// Returns the explicitly selected toolchain root, when supplied.
    pub fn toolchain_root(&self) -> Option<&Path> {
        self.toolchain_root.as_deref()
    }

    /// Returns the maximum compiler worker count supplied to child tools.
    pub const fn worker_count(&self) -> usize {
        self.worker_count
    }

    /// Returns the selected output format.
    pub const fn output_format(&self) -> OutputFormat {
        self.output_format
    }

    /// Returns whether detailed workflow activity was requested.
    pub const fn verbose(&self) -> bool {
        self.verbose
    }

    /// Returns the stable command category.
    pub const fn command_kind(&self) -> TackCommandKind {
        self.command.kind()
    }

    pub(crate) fn into_parts(self) -> (PathBuf, Option<PathBuf>, usize, OutputFormat, TackCommand) {
        (
            self.workspace_root,
            self.toolchain_root,
            self.worker_count,
            self.output_format,
            self.command,
        )
    }
}
