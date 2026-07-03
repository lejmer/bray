use std::ffi::OsString;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::process::ExitCode;

use bray_compilation::WorkerBudget;
use clap::error::ErrorKind as ClapErrorKind;
use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::command::{DriverCommand, DriverInvocation, DriverOptions, DriverOutputFormat};

/// Error returned when parsing driver command-line arguments.
#[derive(Debug)]
pub struct DriverCliError {
    error: clap::Error,
}

impl DriverCliError {
    /// Returns the process exit code for this CLI parse error.
    pub fn exit_code(&self) -> ExitCode {
        match self.error.kind() {
            ClapErrorKind::DisplayHelp | ClapErrorKind::DisplayVersion => ExitCode::SUCCESS,
            _ => ExitCode::FAILURE,
        }
    }
}

impl From<clap::Error> for DriverCliError {
    fn from(error: clap::Error) -> Self {
        Self { error }
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

        Ok(cli.into_driver_invocation())
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
    fn into_driver_invocation(self) -> DriverInvocation {
        DriverInvocation::new(
            self.options.into_driver_options(),
            self.command.into_driver_command(),
        )
    }
}

#[derive(Args, Debug)]
struct CliOptions {
    #[arg(long = "cpu-count", global = true, value_name = "N")]
    cpu_count: Option<NonZeroUsize>,
    #[arg(long = "format", global = true, value_enum, default_value = "text")]
    format: CliOutputFormat,
}

impl CliOptions {
    fn into_driver_options(self) -> DriverOptions {
        let worker_budget = match self.cpu_count {
            Some(cpu_count) => WorkerBudget::from_nonzero(cpu_count),
            None => WorkerBudget::default(),
        };

        DriverOptions::new(worker_budget, self.format.into())
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
    #[arg(value_name = "FILE", required = true, num_args = 1..)]
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
        let result = DriverInvocation::try_from_arguments([
            "brayc",
            "--cpu-count",
            "0",
            "check",
            "main.bray",
        ]);

        assert!(result.is_err());
    }

    #[test]
    fn requires_source_files() {
        let result = DriverInvocation::try_from_arguments(["brayc", "inspect", "source"]);

        assert!(result.is_err());
    }
}
