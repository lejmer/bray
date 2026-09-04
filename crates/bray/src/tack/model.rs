use std::ffi::OsString;
use std::path::{Path, PathBuf};

use bray_test_protocol::{TestCapturePolicy, TestDuration, TestTimeoutPolicy};
use bray_tooling::OutputFormat;

/// Stable category for one Bray Tack command.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TackCommandKind {
    /// Reports managed build storage.
    Storage,
    /// Cleans selected managed build storage.
    Clean,
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
    /// Inspects the project graph or one compiler query.
    Inspect,
    /// Displays or compares compiler profile reports.
    Profile,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TackProfileMode {
    Summary,
    Trace,
}

impl TackProfileMode {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Summary => "summary",
            Self::Trace => "trace",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TackProfileConfiguration {
    mode: TackProfileMode,
    output_directory: Option<PathBuf>,
}

impl TackProfileConfiguration {
    pub(crate) const fn new(mode: TackProfileMode, output_directory: Option<PathBuf>) -> Self {
        Self {
            mode,
            output_directory,
        }
    }

    pub(crate) const fn mode(&self) -> TackProfileMode {
        self.mode
    }

    pub(crate) fn output_directory(&self) -> Option<&Path> {
        self.output_directory.as_deref()
    }
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
    Storage {
        options: TackStorageOptions,
        action: TackStorageAction,
    },
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
        no_build: bool,
        batch_request: Option<PathBuf>,
        native_link_inputs: Vec<String>,
        options: TackTestOptions,
    },
    Format {
        check: bool,
        configuration: Option<PathBuf>,
        files: Vec<PathBuf>,
    },
    Inspect {
        selection: TackSelection,
        inspection: TackInspection,
        source_id: u32,
        position: Option<u32>,
        source: bool,
    },
    ProfileShow {
        report: PathBuf,
    },
    ProfileCompare {
        before: PathBuf,
        after: PathBuf,
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
            Self::Storage { action, .. } => match action {
                TackStorageAction::Report => TackCommandKind::Storage,
                TackStorageAction::PreviewClean | TackStorageAction::Clean => {
                    TackCommandKind::Clean
                }
            },
            Self::Init { .. } => TackCommandKind::Init,
            Self::Check(_) => TackCommandKind::Check,
            Self::Build { .. } => TackCommandKind::Build,
            Self::Run { .. } => TackCommandKind::Run,
            Self::Test { .. } => TackCommandKind::Test,
            Self::Format { .. } => TackCommandKind::Format,
            Self::Inspect { .. } => TackCommandKind::Inspect,
            Self::ProfileShow { .. } | Self::ProfileCompare { .. } => TackCommandKind::Profile,
            Self::LanguageServer { .. } => TackCommandKind::LanguageServer,
            Self::VendorInstall { .. } => TackCommandKind::VendorInstall,
        }
    }

    pub(crate) const fn invokes_compiler(&self) -> bool {
        match self {
            Self::Check(_) | Self::Build { .. } | Self::Run { .. } | Self::Test { .. } => true,
            Self::Inspect { inspection, .. } => !matches!(inspection, TackInspection::Project),
            Self::Storage { .. }
            | Self::Init { .. }
            | Self::Format { .. }
            | Self::ProfileShow { .. }
            | Self::ProfileCompare { .. }
            | Self::LanguageServer { .. }
            | Self::VendorInstall { .. } => false,
        }
    }
}

/// Parsed Bray Tack invocation.
#[derive(Debug, Eq, PartialEq)]
pub struct TackInvocation {
    workspace_root: PathBuf,
    toolchain_root: Option<PathBuf>,
    standard_library_source: bool,
    worker_count: usize,
    output_format: OutputFormat,
    verbose: bool,
    profile: Option<TackProfileConfiguration>,
    command: TackCommand,
}

impl TackInvocation {
    pub(crate) const fn new(
        workspace_root: PathBuf,
        toolchain_root: Option<PathBuf>,
        standard_library_source: bool,
        worker_count: usize,
        output_format: OutputFormat,
        verbose: bool,
        profile: Option<TackProfileConfiguration>,
        command: TackCommand,
    ) -> Self {
        Self {
            workspace_root,
            toolchain_root,
            standard_library_source,
            worker_count,
            output_format,
            verbose,
            profile,
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

    pub(crate) fn into_parts(
        self,
    ) -> (
        PathBuf,
        Option<PathBuf>,
        bool,
        usize,
        OutputFormat,
        Option<TackProfileConfiguration>,
        TackCommand,
    ) {
        (
            self.workspace_root,
            self.toolchain_root,
            self.standard_library_source,
            self.worker_count,
            self.output_format,
            self.profile,
            self.command,
        )
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct TackStorageOptions {
    pub(crate) product: Option<String>,
    pub(crate) target: Option<String>,
    pub(crate) profile: Option<String>,
    pub(crate) toolchain: Option<String>,
    pub(crate) kind: Option<bray_emitter::StorageKind>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TackStorageAction {
    Report,
    PreviewClean,
    Clean,
}
