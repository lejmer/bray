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
    /// Inspects bound semantic units from one source.
    InspectBound,
    /// Inspects lowered units from one source.
    InspectLowered,
    /// Renders lowered units as MIR notation.
    InspectMir,
}

/// Selects semantic units from one source, optionally at one position.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct UnitInspectionTarget {
    source_id: u32,
    position: Option<TextSize>,
}

impl UnitInspectionTarget {
    /// Creates a target covering every independently checked unit in one source.
    pub const fn source(source_id: u32) -> Self {
        Self {
            source_id,
            position: None,
        }
    }

    /// Creates a target for the innermost unit covering one UTF-8 byte offset.
    pub const fn at(source_id: u32, position: TextSize) -> Self {
        Self {
            source_id,
            position: Some(position),
        }
    }

    /// Returns the raw loaded-source identity.
    pub const fn source_id(self) -> u32 {
        self.source_id
    }

    /// Returns the selected UTF-8 byte offset, if inspection is position-filtered.
    pub const fn position(self) -> Option<TextSize> {
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
    /// Inspects bound semantic units from one source.
    InspectBound {
        /// Source and optional position used to select semantic units.
        target: UnitInspectionTarget,
        /// Source files to inspect.
        files: Vec<PathBuf>,
    },
    /// Inspects lowered units from one source.
    InspectLowered {
        /// Source and optional position used to select semantic units.
        target: UnitInspectionTarget,
        /// Source files to inspect.
        files: Vec<PathBuf>,
    },
    /// Renders lowered units as MIR notation.
    InspectMir {
        /// Source and optional position used to select semantic units.
        target: UnitInspectionTarget,
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
    pub fn inspect_bound(target: UnitInspectionTarget, files: Vec<PathBuf>) -> Self {
        Self::InspectBound { target, files }
    }

    /// Creates an inspect-lowered command.
    pub fn inspect_lowered(target: UnitInspectionTarget, files: Vec<PathBuf>) -> Self {
        Self::InspectLowered { target, files }
    }

    /// Creates an inspect-MIR command.
    pub fn inspect_mir(target: UnitInspectionTarget, files: Vec<PathBuf>) -> Self {
        Self::InspectMir { target, files }
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
            Self::InspectLowered { .. } => DriverCommandKind::InspectLowered,
            Self::InspectMir { .. } => DriverCommandKind::InspectMir,
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
            | Self::InspectBound { files, .. }
            | Self::InspectLowered { files, .. }
            | Self::InspectMir { files, .. } => files,
        }
    }

    /// Returns the source and optional position selected for semantic-unit inspection.
    pub const fn unit_inspection_target(&self) -> Option<UnitInspectionTarget> {
        match self {
            Self::InspectBound { target, .. }
            | Self::InspectLowered { target, .. }
            | Self::InspectMir { target, .. } => Some(*target),
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
            | Self::InspectBound { files, .. }
            | Self::InspectLowered { files, .. }
            | Self::InspectMir { files, .. } => files,
        }
    }
}

/// Parsed driver invocation.
#[derive(Debug, Eq, PartialEq)]
pub struct DriverInvocation {
    options: DriverOptions,
    command: DriverCommand,
    output_file: Option<PathBuf>,
}

impl DriverInvocation {
    /// Creates a parsed driver invocation from options and a command.
    pub const fn new(
        options: DriverOptions,
        command: DriverCommand,
        output_file: Option<PathBuf>,
    ) -> Self {
        Self {
            options,
            command,
            output_file,
        }
    }

    /// Returns shared driver options.
    pub const fn options(&self) -> DriverOptions {
        self.options
    }

    /// Returns the selected driver command.
    pub const fn command(&self) -> &DriverCommand {
        &self.command
    }

    /// Returns the inspection report destination, when one was requested.
    pub fn output_file(&self) -> Option<&std::path::Path> {
        self.output_file.as_deref()
    }

    /// Consumes this invocation into options, command, and report destination.
    pub fn into_parts(self) -> (DriverOptions, DriverCommand, Option<PathBuf>) {
        (self.options, self.command, self.output_file)
    }
}
