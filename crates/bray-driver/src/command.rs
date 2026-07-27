use std::path::PathBuf;

use bray_compilation::{CompilationOptions, WorkerBudget};
use bray_source::TextSize;

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
    pub fn compilation_options(self) -> CompilationOptions {
        CompilationOptions::new(
            self.worker_budget,
            bray_symbols::ProductKind::Library,
            bray_compilation::SelectedTarget::baseline(),
        )
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
    /// Inspects lexed source tokens.
    InspectTokens,
    /// Inspects parsed source syntax trees.
    InspectSyntax,
    /// Inspects discovered declarations.
    InspectDeclarations,
    /// Inspects the compilation-wide symbol graph.
    InspectSymbols,
    /// Inspects one source-selected bound semantic unit.
    InspectBound,
}

/// Selects the innermost bound semantic unit covering one source position.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BoundInspectionTarget {
    source_id: u32,
    position: TextSize,
}

impl BoundInspectionTarget {
    /// Creates a bound-unit inspection target from a source ID and UTF-8 byte offset.
    pub const fn new(source_id: u32, position: TextSize) -> Self {
        Self {
            source_id,
            position,
        }
    }

    /// Returns the raw loaded-source identity.
    pub const fn source_id(self) -> u32 {
        self.source_id
    }

    /// Returns the selected UTF-8 byte offset.
    pub const fn position(self) -> TextSize {
        self.position
    }
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
    /// Inspects lexed source tokens.
    InspectTokens {
        /// Source files to inspect.
        files: Vec<PathBuf>,
    },
    /// Inspects parsed source syntax trees.
    InspectSyntax {
        /// Source files to inspect.
        files: Vec<PathBuf>,
    },
    /// Inspects discovered declarations.
    InspectDeclarations {
        /// Source files to inspect.
        files: Vec<PathBuf>,
    },
    /// Inspects the compilation-wide symbol graph.
    InspectSymbols {
        /// Source files to inspect.
        files: Vec<PathBuf>,
    },
    /// Inspects one source-selected bound semantic unit.
    InspectBound {
        /// Source position used to select the innermost semantic unit.
        target: BoundInspectionTarget,
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

    /// Creates an inspect-tokens command.
    pub fn inspect_tokens(files: Vec<PathBuf>) -> Self {
        Self::InspectTokens { files }
    }

    /// Creates an inspect-syntax command.
    pub fn inspect_syntax(files: Vec<PathBuf>) -> Self {
        Self::InspectSyntax { files }
    }

    /// Creates an inspect-declarations command.
    pub fn inspect_declarations(files: Vec<PathBuf>) -> Self {
        Self::InspectDeclarations { files }
    }

    /// Creates an inspect-symbols command.
    pub fn inspect_symbols(files: Vec<PathBuf>) -> Self {
        Self::InspectSymbols { files }
    }

    /// Creates an inspect-bound command.
    pub fn inspect_bound(target: BoundInspectionTarget, files: Vec<PathBuf>) -> Self {
        Self::InspectBound { target, files }
    }

    /// Returns this command's stable category.
    pub const fn kind(&self) -> DriverCommandKind {
        match self {
            Self::Check { .. } => DriverCommandKind::Check,
            Self::InspectSource { .. } => DriverCommandKind::InspectSource,
            Self::InspectTokens { .. } => DriverCommandKind::InspectTokens,
            Self::InspectSyntax { .. } => DriverCommandKind::InspectSyntax,
            Self::InspectDeclarations { .. } => DriverCommandKind::InspectDeclarations,
            Self::InspectSymbols { .. } => DriverCommandKind::InspectSymbols,
            Self::InspectBound { .. } => DriverCommandKind::InspectBound,
        }
    }

    /// Returns the command's source file paths.
    pub fn files(&self) -> &[PathBuf] {
        match self {
            Self::Check { files }
            | Self::InspectSource { files }
            | Self::InspectTokens { files }
            | Self::InspectSyntax { files }
            | Self::InspectDeclarations { files }
            | Self::InspectSymbols { files }
            | Self::InspectBound { files, .. } => files,
        }
    }

    /// Returns the source position selected for bound inspection.
    pub const fn bound_inspection_target(&self) -> Option<BoundInspectionTarget> {
        match self {
            Self::InspectBound { target, .. } => Some(*target),
            _ => None,
        }
    }

    pub(crate) fn into_files(self) -> Vec<PathBuf> {
        match self {
            Self::Check { files }
            | Self::InspectSource { files }
            | Self::InspectTokens { files }
            | Self::InspectSyntax { files }
            | Self::InspectDeclarations { files }
            | Self::InspectSymbols { files }
            | Self::InspectBound { files, .. } => files,
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
}
