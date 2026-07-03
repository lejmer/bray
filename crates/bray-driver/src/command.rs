use std::path::PathBuf;

use bray_compilation::{CompilationOptions, CompilationRequest, WorkerBudget};

use crate::file_arguments::{DriverSourceInputError, compilation_request_from_file_arguments};

/// Output format selected for driver-produced output.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DriverOutputFormat {
    /// Plain text output.
    #[default]
    Text,
    /// Structured JSON output.
    Json,
}

/// Options shared by all driver commands.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DriverOptions {
    worker_budget: WorkerBudget,
    output_format: DriverOutputFormat,
}

impl DriverOptions {
    /// Creates shared driver options.
    pub const fn new(worker_budget: WorkerBudget, output_format: DriverOutputFormat) -> Self {
        Self {
            worker_budget,
            output_format,
        }
    }

    /// Returns the compiler-owned CPU worker budget.
    pub const fn worker_budget(self) -> WorkerBudget {
        self.worker_budget
    }

    /// Returns the selected driver output format.
    pub const fn output_format(self) -> DriverOutputFormat {
        self.output_format
    }

    /// Returns compilation options derived from driver options.
    pub const fn compilation_options(self) -> CompilationOptions {
        CompilationOptions::new(self.worker_budget)
    }
}

impl Default for DriverOptions {
    fn default() -> Self {
        Self::new(WorkerBudget::default(), DriverOutputFormat::default())
    }
}

/// Stable category for a driver command.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DriverCommandKind {
    /// Checks source files.
    Check,
    /// Inspects loaded source snapshots.
    InspectSource,
}

/// Driver command selected by the CLI.
#[derive(Debug, Eq, PartialEq)]
pub enum DriverCommand {
    /// Checks source files.
    Check {
        /// Source files to check.
        files: Vec<PathBuf>,
    },
    /// Inspects loaded source snapshots.
    InspectSource {
        /// Source files to inspect.
        files: Vec<PathBuf>,
    },
}

impl DriverCommand {
    /// Creates a check command.
    pub fn check(files: Vec<PathBuf>) -> Self {
        Self::Check { files }
    }

    /// Creates an inspect-source command.
    pub fn inspect_source(files: Vec<PathBuf>) -> Self {
        Self::InspectSource { files }
    }

    /// Returns this command's stable category.
    pub const fn kind(&self) -> DriverCommandKind {
        match self {
            Self::Check { .. } => DriverCommandKind::Check,
            Self::InspectSource { .. } => DriverCommandKind::InspectSource,
        }
    }

    /// Returns the command's source file paths.
    pub fn files(&self) -> &[PathBuf] {
        match self {
            Self::Check { files } | Self::InspectSource { files } => files,
        }
    }

    fn into_files(self) -> Vec<PathBuf> {
        match self {
            Self::Check { files } | Self::InspectSource { files } => files,
        }
    }
}

/// Parsed driver invocation.
#[derive(Debug, Eq, PartialEq)]
pub struct DriverInvocation {
    options: DriverOptions,
    command: DriverCommand,
}

impl DriverInvocation {
    /// Creates a parsed driver invocation from options and a command.
    pub const fn new(options: DriverOptions, command: DriverCommand) -> Self {
        Self { options, command }
    }

    /// Returns shared driver options.
    pub const fn options(&self) -> DriverOptions {
        self.options
    }

    /// Returns the selected driver command.
    pub const fn command(&self) -> &DriverCommand {
        &self.command
    }

    /// Consumes this invocation into options and command.
    pub fn into_parts(self) -> (DriverOptions, DriverCommand) {
        (self.options, self.command)
    }

    /// Builds the compilation request for this invocation.
    pub fn into_compilation_request(self) -> Result<CompilationRequest, DriverSourceInputError> {
        let options = self.options.compilation_options();
        let files = self.command.into_files();

        compilation_request_from_file_arguments(files, options)
    }
}
