use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

use bray_compilation::WorkerBudget;
use bray_diagnostics::DiagnosticBag;
use bray_tooling::{
    OutputFormat, clap_styles, exit_code_from_diagnostics,
    render_styled_text,
};
use clap::error::ErrorKind as ClapErrorKind;
use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::tack::model::{
    TackCommand, TackInspection, TackInvocation, TackSelection,
};

/// Error returned when parsing Bray Tack command-line arguments.
#[derive(Debug)]
pub struct TackCliError {
    kind: TackCliErrorKind,
}

#[derive(Debug)]
enum TackCliErrorKind {
    Clap(clap::Error),
    Diagnostics {
        diagnostics: DiagnosticBag,
        output_format: OutputFormat,
    },
}

impl TackCliError {
    /// Returns the process exit code for this command-line parse error.
    pub fn exit_code(&self) -> ExitCode {
        match &self.kind {
            TackCliErrorKind::Clap(error) => match error.kind() {
                ClapErrorKind::DisplayHelp
                | ClapErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
                | ClapErrorKind::DisplayVersion => ExitCode::SUCCESS,
                _ => ExitCode::FAILURE,
            },
            TackCliErrorKind::Diagnostics { diagnostics, .. } => {
                exit_code_from_diagnostics(diagnostics)
            }
        }
    }

    /// Returns the output format selected before this parse failure.
    pub const fn output_format(&self) -> OutputFormat {
        match &self.kind {
            TackCliErrorKind::Clap(_) => OutputFormat::Text,
            TackCliErrorKind::Diagnostics { output_format, .. } => *output_format,
        }
    }

    /// Converts this parse failure into structured diagnostics when available.
    pub fn into_diagnostics(self) -> DiagnosticBag {
        match self.kind {
            TackCliErrorKind::Clap(_) => DiagnosticBag::new(),
            TackCliErrorKind::Diagnostics { diagnostics, .. } => diagnostics,
        }
    }

    pub(crate) fn into_output(self) -> (DiagnosticBag, String, String) {
        match self.kind {
            TackCliErrorKind::Clap(error) => {
                let output = render_styled_text(&error.render());

                if clap_error_uses_stdout(&error) {
                    (DiagnosticBag::new(), output, String::new())
                } else {
                    (DiagnosticBag::new(), String::new(), output)
                }
            }
            TackCliErrorKind::Diagnostics { diagnostics, .. } => {
                (diagnostics, String::new(), String::new())
            }
        }
    }
}

impl From<clap::Error> for TackCliError {
    fn from(error: clap::Error) -> Self {
        Self {
            kind: TackCliErrorKind::Clap(error),
        }
    }
}

impl TackInvocation {
    /// Parses process arguments into a typed Bray Tack invocation.
    pub fn try_from_arguments<I, T>(arguments: I) -> Result<Self, TackCliError>
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString> + Clone,
    {
        let cli = Cli::try_parse_from(arguments).map_err(TackCliError::from)?;

        cli.into_invocation()
    }
}

fn clap_error_uses_stdout(error: &clap::Error) -> bool {
    matches!(
        error.kind(),
        ClapErrorKind::DisplayHelp
            | ClapErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
            | ClapErrorKind::DisplayVersion
    )
}

#[derive(Debug, Parser)]
#[command(
    name = "bray",
    version = env!("CARGO_PKG_VERSION"),
    about = "Bray Tack project and workspace driver",
    styles = clap_styles(),
    arg_required_else_help = true
)]
struct Cli {
    #[arg(long, global = true, value_name = "DIRECTORY", default_value = ".")]
    workspace: PathBuf,
    #[arg(long = "cpu-count", global = true, value_name = "N")]
    cpu_count: Option<usize>,
    #[arg(long = "format", global = true, value_enum, default_value = "text")]
    output_format: CliOutputFormat,
    #[command(subcommand)]
    command: CliCommand,
}

impl Cli {
    fn into_invocation(self) -> Result<TackInvocation, TackCliError> {
        let output_format = OutputFormat::from(self.output_format);

        let worker_budget = match self.cpu_count {
            Some(cpu_count) => WorkerBudget::new(cpu_count).map_err(|error| TackCliError {
                kind: TackCliErrorKind::Diagnostics {
                    diagnostics: error.into_diagnostic_bag(),
                    output_format,
                },
            })?,
            None => WorkerBudget::default(),
        };

        Ok(TackInvocation::new(
            self.workspace,
            worker_budget,
            output_format,
            self.command.into_command(),
        ))
    }
}

#[derive(Debug, Subcommand)]
enum CliCommand {
    Check(CliSelection),
    Build(CliSelection),
    Run(CliExecution),
    Test(CliExecution),
    #[command(name = "fmt")]
    Format(CliFormat),
    Inspect(CliInspect),
    #[command(name = "language-server")]
    LanguageServer(CliLanguageServer),
    Vendor(CliVendor),
}

impl CliCommand {
    fn into_command(self) -> TackCommand {
        match self {
            Self::Check(selection) => TackCommand::Check(selection.into()),
            Self::Build(selection) => TackCommand::Build(selection.into()),
            Self::Run(execution) => TackCommand::Run {
                selection: execution.selection.into(),
                arguments: execution.arguments,
            },
            Self::Test(execution) => TackCommand::Test {
                selection: execution.selection.into(),
                arguments: execution.arguments,
            },
            Self::Format(format) => TackCommand::Format {
                check: format.check,
                files: format.files,
            },
            Self::Inspect(inspect) => TackCommand::Inspect {
                selection: inspect.selection.into(),
                inspection: inspect.inspection.into(),
                source_id: inspect.source_id,
                position: inspect.offset.map(Into::into),
            },
            Self::LanguageServer(server) => TackCommand::LanguageServer {
                target: server.target,
            },
            Self::Vendor(vendor) => vendor.into_command(),
        }
    }
}

#[derive(Args, Debug)]
struct CliLanguageServer {
    #[arg(long, value_name = "NAME")]
    target: Option<String>,
}

#[derive(Args, Debug)]
struct CliSelection {
    #[arg(long, value_name = "IDENTITY")]
    package: Option<String>,
    #[arg(long, value_name = "NAME")]
    product: Option<String>,
    #[arg(long, value_name = "NAME")]
    target: Option<String>,
}

impl From<CliSelection> for TackSelection {
    fn from(selection: CliSelection) -> Self {
        Self {
            package: selection.package,
            product: selection.product,
            target: selection.target,
        }
    }
}

#[derive(Args, Debug)]
#[command(trailing_var_arg = true)]
struct CliExecution {
    #[command(flatten)]
    selection: CliSelection,
    #[arg(value_name = "ARG")]
    arguments: Vec<OsString>,
}

#[derive(Args, Debug)]
struct CliFormat {
    #[arg(long)]
    check: bool,
    #[arg(value_name = "FILE")]
    files: Vec<PathBuf>,
}

#[derive(Args, Debug)]
struct CliInspect {
    #[command(flatten)]
    selection: CliSelection,
    #[arg(value_enum)]
    inspection: CliInspection,
    #[arg(long, default_value_t = 0)]
    source_id: u32,
    #[arg(long)]
    offset: Option<u32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum CliInspection {
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

impl From<CliInspection> for TackInspection {
    fn from(inspection: CliInspection) -> Self {
        match inspection {
            CliInspection::Project => Self::Project,
            CliInspection::Source => Self::Source,
            CliInspection::Tokens => Self::Tokens,
            CliInspection::Syntax => Self::Syntax,
            CliInspection::Declarations => Self::Declarations,
            CliInspection::Symbols => Self::Symbols,
            CliInspection::Bound => Self::Bound,
            CliInspection::Lowered => Self::Lowered,
            CliInspection::Mir => Self::Mir,
        }
    }
}

#[derive(Args, Debug)]
struct CliVendor {
    #[command(subcommand)]
    command: CliVendorCommand,
}

impl CliVendor {
    fn into_command(self) -> TackCommand {
        match self.command {
            CliVendorCommand::Install(install) => TackCommand::VendorInstall {
                name: install.name,
                repository: install.repository,
            },
        }
    }
}

#[derive(Debug, Subcommand)]
enum CliVendorCommand {
    Install(CliVendorInstall),
}

#[derive(Args, Debug)]
struct CliVendorInstall {
    #[arg(value_name = "NAME")]
    name: String,
    #[arg(value_name = "GIT_REPOSITORY")]
    repository: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum CliOutputFormat {
    Text,
    Json,
}

impl From<CliOutputFormat> for OutputFormat {
    fn from(format: CliOutputFormat) -> Self {
        match format {
            CliOutputFormat::Text => Self::Text,
            CliOutputFormat::Json => Self::Json,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::tack::{TackCommandKind, TackInvocation};

    #[test]
    fn routes_every_project_command() {
        let cases = [
            (vec!["check"], TackCommandKind::Check),
            (vec!["build"], TackCommandKind::Build),
            (vec!["run"], TackCommandKind::Run),
            (vec!["test"], TackCommandKind::Test),
            (vec!["fmt"], TackCommandKind::Format),
            (vec!["inspect", "project"], TackCommandKind::Inspect),
            (
                vec!["language-server"],
                TackCommandKind::LanguageServer,
            ),
            (
                vec!["vendor", "install", "math", "https://example.invalid/math.git"],
                TackCommandKind::VendorInstall,
            ),
        ];

        for (arguments, expected) in cases {
            let invocation = TackInvocation::try_from_arguments(
                std::iter::once("bray").chain(arguments),
            )
            .unwrap_or_else(|error| panic!("command should parse: {error:?}"));

            assert_eq!(invocation.command_kind(), expected);
        }
    }

    #[test]
    fn keeps_workspace_selection_explicit() {
        let invocation = TackInvocation::try_from_arguments([
            "bray",
            "--workspace",
            "project",
            "check",
            "--package",
            "example.application",
            "--product",
            "app",
            "--target",
            "native",
        ])
        .unwrap_or_else(|error| panic!("selection should parse: {error:?}"));

        assert_eq!(invocation.workspace_root(), Path::new("project"));
        assert_eq!(invocation.command_kind(), TackCommandKind::Check);
    }

    #[test]
    fn accepts_formatter_file_and_standard_input_forms() {
        for arguments in [
            vec!["bray", "fmt", "--check", "src/main.bray"],
            vec!["bray", "fmt", "-"],
        ] {
            let invocation = TackInvocation::try_from_arguments(arguments)
                .unwrap_or_else(|error| panic!("format input should parse: {error:?}"));

            assert_eq!(invocation.command_kind(), TackCommandKind::Format);
        }
    }
}
