use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

use bray_compilation::WorkerBudget;
use bray_diagnostics::DiagnosticBag;
use clap::error::ErrorKind as ClapErrorKind;
use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::command::{DriverCommand, DriverInvocation, DriverOptions, DriverOutputFormat};

/// Error returned when parsing driver command-line arguments.
#[derive(Debug)]
pub struct DriverCliError {
    kind: DriverCliErrorKind,
}

#[derive(Debug)]
enum DriverCliErrorKind {
    Clap(clap::Error),
    Diagnostics(DiagnosticBag),
}

impl DriverCliError {
    /// Returns the process exit code for this CLI parse error.
    pub fn exit_code(&self) -> ExitCode {
        match &self.kind {
            DriverCliErrorKind::Clap(error) => match error.kind() {
                ClapErrorKind::DisplayHelp | ClapErrorKind::DisplayVersion => ExitCode::SUCCESS,
                _ => ExitCode::FAILURE,
            },
            DriverCliErrorKind::Diagnostics(_) => ExitCode::FAILURE,
        }
    }

    /// Converts this parse failure into structured diagnostics when available.
    pub fn into_diagnostics(self) -> DiagnosticBag {
        match self.kind {
            DriverCliErrorKind::Clap(_) => DiagnosticBag::new(),
            DriverCliErrorKind::Diagnostics(diagnostics) => diagnostics,
        }
    }
}

impl From<clap::Error> for DriverCliError {
    fn from(error: clap::Error) -> Self {
        Self {
            kind: DriverCliErrorKind::Clap(error),
        }
    }
}

impl DriverInvocation {
    /// Parses process arguments into a typed driver invocation.
    pub fn try_from_arguments<I, T>(arguments: I) -> Result<Self, DriverCliError>
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString> + Clone,
    {
        let cli = Cli::try_parse_from(arguments).map_err(DriverCliError::from)?;

        cli.into_driver_invocation()
    }
}

#[derive(Debug, Parser)]
#[command(name = "brayc")]
struct Cli {
    #[command(flatten)]
    options: CliOptions,
    #[command(subcommand)]
    command: CliCommand,
}

impl Cli {
    fn into_driver_invocation(self) -> Result<DriverInvocation, DriverCliError> {
        let options = self
            .options
            .into_driver_options()
            .map_err(|diagnostics| DriverCliError {
                kind: DriverCliErrorKind::Diagnostics(diagnostics),
            })?;

        Ok(DriverInvocation::new(
            options,
            self.command.into_driver_command(),
        ))
    }
}

#[derive(Args, Debug)]
struct CliOptions {
    #[arg(long = "cpu-count", global = true, value_name = "N")]
    cpu_count: Option<usize>,
    #[arg(long = "format", global = true, value_enum, default_value = "text")]
    format: CliOutputFormat,
}

impl CliOptions {
    fn into_driver_options(self) -> Result<DriverOptions, DiagnosticBag> {
        let worker_budget = match self.cpu_count {
            Some(cpu_count) => match WorkerBudget::new(cpu_count) {
                Ok(worker_budget) => worker_budget,
                Err(error) => return Err(error.into_diagnostic_bag()),
            },
            None => WorkerBudget::default(),
        };

        Ok(DriverOptions::new(worker_budget, self.format.into()))
    }
}

#[derive(Debug, Subcommand)]
enum CliCommand {
    Check(CliSourceFiles),
    Inspect(CliInspectCommand),
}

impl CliCommand {
    fn into_driver_command(self) -> DriverCommand {
        match self {
            Self::Check(files) => DriverCommand::check(files.files),
            Self::Inspect(command) => command.into_driver_command(),
        }
    }
}

#[derive(Args, Debug)]
struct CliInspectCommand {
    #[command(subcommand)]
    command: CliInspectSubcommand,
}

impl CliInspectCommand {
    fn into_driver_command(self) -> DriverCommand {
        match self.command {
            CliInspectSubcommand::Source(files) => DriverCommand::inspect_source(files.files),
        }
    }
}

#[derive(Debug, Subcommand)]
enum CliInspectSubcommand {
    Source(CliSourceFiles),
}

#[derive(Args, Debug)]
struct CliSourceFiles {
    #[arg(value_name = "FILE", num_args = 0..)]
    files: Vec<PathBuf>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum CliOutputFormat {
    Text,
    Json,
}

impl From<CliOutputFormat> for DriverOutputFormat {
    fn from(format: CliOutputFormat) -> Self {
        match format {
            CliOutputFormat::Text => Self::Text,
            CliOutputFormat::Json => Self::Json,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use bray_compilation::WorkerBudget;
    use bray_diagnostics::DiagnosticKind;

    use crate::command::{DriverCommandKind, DriverInvocation, DriverOutputFormat};

    #[test]
    fn parses_check_command_with_global_options() {
        let invocation = match DriverInvocation::try_from_arguments([
            "brayc",
            "--cpu-count",
            "1",
            "--format",
            "json",
            "check",
            "main.bray",
        ]) {
            Ok(invocation) => invocation,
            Err(error) => panic!("check invocation should parse: {error:?}"),
        };

        assert_eq!(invocation.options().worker_budget(), WorkerBudget::serial());

        assert_eq!(
            invocation.options().output_format(),
            DriverOutputFormat::Json
        );

        assert_eq!(invocation.command().kind(), DriverCommandKind::Check);

        let files = [PathBuf::from("main.bray")];

        assert_eq!(invocation.command().files(), files.as_slice());
    }

    #[test]
    fn parses_inspect_source_command_with_subcommand_options() {
        let invocation = match DriverInvocation::try_from_arguments([
            "brayc",
            "inspect",
            "--format",
            "text",
            "source",
            "--cpu-count",
            "1",
            "main.bray",
            "lib.bray",
        ]) {
            Ok(invocation) => invocation,
            Err(error) => panic!("inspect source invocation should parse: {error:?}"),
        };

        assert_eq!(invocation.options().worker_budget(), WorkerBudget::serial());

        assert_eq!(
            invocation.options().output_format(),
            DriverOutputFormat::Text
        );

        assert_eq!(
            invocation.command().kind(),
            DriverCommandKind::InspectSource
        );

        let files = [PathBuf::from("main.bray"), PathBuf::from("lib.bray")];

        assert_eq!(invocation.command().files(), files.as_slice());
    }

    #[test]
    fn rejects_zero_cpu_count() {
        let error = match DriverInvocation::try_from_arguments([
            "brayc",
            "--cpu-count",
            "0",
            "check",
            "main.bray",
        ]) {
            Ok(invocation) => panic!("zero worker budget should fail: {invocation:?}"),
            Err(error) => error,
        };

        let diagnostics = error.into_diagnostics();

        assert_eq!(
            diagnostics
                .by_kind(DiagnosticKind::RequestInvalidWorkerBudget)
                .count(),
            1
        );
    }

    #[test]
    fn parses_empty_source_file_lists_for_structured_request_diagnostics() {
        let invocation = match DriverInvocation::try_from_arguments(["brayc", "inspect", "source"])
        {
            Ok(invocation) => invocation,
            Err(error) => panic!("empty source list should parse for diagnostics: {error:?}"),
        };

        assert_eq!(
            invocation.command().kind(),
            DriverCommandKind::InspectSource
        );

        assert!(invocation.command().files().is_empty());
    }
}
