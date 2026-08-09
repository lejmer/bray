use std::ffi::OsString;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::process::ExitCode;

use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticKind, SeverityKind,
};
use bray_test_protocol::{TestCaptureLimits, TestCapturePolicy};
use bray_tooling::{OutputFormat, clap_styles, exit_code_from_diagnostics, render_styled_text};
use clap::error::ErrorKind as ClapErrorKind;
use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::tack::model::{
    TackCommand, TackInspection, TackInvocation, TackProfileConfiguration, TackProfileMode,
    TackSelection,
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

fn invalid_worker_count() -> DiagnosticBag {
    DiagnosticBag::single(
        Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::ProjectCommandSelectionInvalid,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::referenced_name("cpu_count")),
    )
}

#[derive(Debug, Parser)]
#[command(
    name = "bray",
    version = env!("CARGO_PKG_VERSION"),
    about = "Bray Tack, Bray's build tool",
    styles = clap_styles(),
    arg_required_else_help = true
)]
struct Cli {
    #[arg(long, global = true, value_name = "DIRECTORY", default_value = ".")]
    workspace: PathBuf,
    #[arg(long = "toolchain-root", global = true, value_name = "DIRECTORY")]
    toolchain_root: Option<PathBuf>,
    #[arg(long = "standard-library-source", global = true, hide = true)]
    standard_library_source: bool,
    #[arg(long = "cpu-count", global = true, value_name = "N")]
    cpu_count: Option<usize>,
    #[arg(long = "format", global = true, value_enum, default_value = "text")]
    output_format: OutputFormat,
    #[arg(short, long, global = true)]
    verbose: bool,
    #[arg(long, global = true, value_enum, value_name = "MODE")]
    profile: Option<CliProfileMode>,
    #[arg(
        long,
        global = true,
        value_name = "DIRECTORY",
        requires = "profile",
        required_if_eq("profile", "trace")
    )]
    profile_output: Option<PathBuf>,
    #[command(subcommand)]
    command: CliCommand,
}

impl Cli {
    fn into_invocation(self) -> Result<TackInvocation, TackCliError> {
        let output_format = self.output_format;

        let worker_count = match self.cpu_count {
            Some(0) => {
                return Err(TackCliError {
                    kind: TackCliErrorKind::Diagnostics {
                        diagnostics: invalid_worker_count(),
                        output_format,
                    },
                });
            }
            Some(worker_count) => worker_count,
            None => std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get),
        };

        let command = self.command.into_command();

        if self.profile.is_some() && !command.invokes_compiler() {
            return Err(TackCliError {
                kind: TackCliErrorKind::Diagnostics {
                    diagnostics: invalid_profile_command(),
                    output_format,
                },
            });
        }

        Ok(TackInvocation::new(
            self.workspace,
            self.toolchain_root,
            self.standard_library_source,
            worker_count,
            output_format,
            self.verbose,
            self.profile.map(|mode| {
                TackProfileConfiguration::new(mode.into(), self.profile_output)
            }),
            command,
        ))
    }
}

fn invalid_profile_command() -> DiagnosticBag {
    DiagnosticBag::single(
        Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::ProjectCommandSelectionInvalid,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::referenced_name("profile")),
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum CliProfileMode {
    Summary,
    Trace,
}

impl From<CliProfileMode> for TackProfileMode {
    fn from(mode: CliProfileMode) -> Self {
        match mode {
            CliProfileMode::Summary => Self::Summary,
            CliProfileMode::Trace => Self::Trace,
        }
    }
}

#[derive(Debug, Subcommand)]
enum CliCommand {
    Init(CliInit),
    Check(CliSelection),
    Build(CliBuild),
    Run(CliExecution),
    Test(CliTest),
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
            Self::Init(init) => TackCommand::Init {
                directory: init.directory,
                package: init.package,
            },
            Self::Check(selection) => TackCommand::Check(selection.into()),
            Self::Build(build) => {
                let configuration = build.configuration();

                TackCommand::Build {
                    selection: build.selection.into(),
                    configuration,
                }
            }
            Self::Run(execution) => {
                let configuration = execution.configuration();

                TackCommand::Run {
                    selection: execution.selection.into(),
                    configuration,
                    arguments: execution.arguments,
                }
            }
            Self::Test(test) => {
                let configuration = build_configuration(test.release);

                let maximum_concurrency = if test.sequential {
                    1
                } else {
                    test.jobs.map_or(usize::MAX, NonZeroUsize::get)
                };

                let capture = if test.no_capture {
                    TestCapturePolicy::Inherited
                } else if test.discard_output {
                    TestCapturePolicy::Discarded
                } else {
                    TestCapturePolicy::Captured(TestCaptureLimits::new(
                        test.capture_limit,
                        test.capture_limit.saturating_mul(2),
                    ))
                };

                TackCommand::Test {
                    selection: test.selection.into(),
                    configuration,
                    options: crate::tack::model::TackTestOptions::new(
                        test.filters,
                        maximum_concurrency,
                        test.timeout_ms,
                        capture,
                        test.show_output,
                    ),
                }
            }
            Self::Format(format) => TackCommand::Format {
                check: format.check,
                configuration: format.configuration,
                files: format.files,
            },
            Self::Inspect(inspect) => TackCommand::Inspect {
                selection: inspect.selection.into(),
                inspection: inspect.inspection.into(),
                source_id: inspect.source_id,
                position: inspect.offset,
            },
            Self::LanguageServer(server) => TackCommand::LanguageServer {
                target: server.target,
            },
            Self::Vendor(vendor) => vendor.into_command(),
        }
    }
}

#[derive(Args, Debug)]
struct CliInit {
    #[arg(value_name = "DIRECTORY")]
    directory: Option<PathBuf>,
    #[arg(long, value_name = "IDENTITY")]
    package: Option<String>,
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

#[derive(Args, Debug)]
struct CliBuild {
    #[command(flatten)]
    selection: CliSelection,
    #[arg(long)]
    release: bool,
}

impl CliBuild {
    const fn configuration(&self) -> crate::tack::model::TackBuildConfiguration {
        build_configuration(self.release)
    }
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
    #[arg(long)]
    release: bool,
    #[arg(value_name = "ARG")]
    arguments: Vec<OsString>,
}

impl CliExecution {
    const fn configuration(&self) -> crate::tack::model::TackBuildConfiguration {
        build_configuration(self.release)
    }
}

#[derive(Args, Debug)]
struct CliTest {
    #[command(flatten)]
    selection: CliSelection,
    #[arg(long)]
    release: bool,
    #[arg(long, conflicts_with = "jobs")]
    sequential: bool,
    #[arg(long, value_name = "N")]
    jobs: Option<NonZeroUsize>,
    #[arg(long, value_name = "MILLISECONDS")]
    timeout_ms: Option<u64>,
    #[arg(long, conflicts_with = "discard_output")]
    no_capture: bool,
    #[arg(long, conflicts_with = "no_capture")]
    discard_output: bool,
    #[arg(long, default_value_t = 1_048_576, value_name = "BYTES")]
    capture_limit: u64,
    #[arg(long)]
    show_output: bool,
    #[arg(value_name = "FILTER")]
    filters: Vec<String>,
}

const fn build_configuration(release: bool) -> crate::tack::model::TackBuildConfiguration {
    if release {
        crate::tack::model::TackBuildConfiguration::Release
    } else {
        crate::tack::model::TackBuildConfiguration::Debug
    }
}

#[derive(Args, Debug)]
struct CliFormat {
    #[arg(long)]
    check: bool,
    #[arg(long = "config", value_name = "FILE")]
    configuration: Option<PathBuf>,
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

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use bray_test_protocol::{
        TestCaptureLimits, TestCapturePolicy, TestDuration, TestTimeoutPolicy,
    };

    use crate::tack::model::{TackBuildConfiguration, TackCommand, TackProfileMode};
    use crate::tack::{TackCommandKind, TackInvocation};

    #[test]
    fn routes_every_project_command() {
        let cases = [
            (vec!["init"], TackCommandKind::Init),
            (vec!["check"], TackCommandKind::Check),
            (vec!["build"], TackCommandKind::Build),
            (vec!["run"], TackCommandKind::Run),
            (vec!["test"], TackCommandKind::Test),
            (vec!["fmt"], TackCommandKind::Format),
            (vec!["inspect", "project"], TackCommandKind::Inspect),
            (vec!["language-server"], TackCommandKind::LanguageServer),
            (
                vec![
                    "vendor",
                    "install",
                    "math",
                    "https://example.invalid/math.git",
                ],
                TackCommandKind::VendorInstall,
            ),
        ];

        for (arguments, expected) in cases {
            let invocation =
                TackInvocation::try_from_arguments(std::iter::once("bray").chain(arguments))
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
    fn keeps_explicit_toolchain_selection() {
        let invocation =
            TackInvocation::try_from_arguments(["bray", "--toolchain-root", "toolchain", "check"])
                .unwrap_or_else(|error| panic!("toolchain selection should parse: {error:?}"));

        assert_eq!(invocation.toolchain_root(), Some(Path::new("toolchain")));
    }

    #[test]
    fn keeps_verbose_workflow_selection() {
        let invocation = TackInvocation::try_from_arguments(["bray", "--verbose", "build"])
            .unwrap_or_else(|error| panic!("verbose build should parse: {error:?}"));

        assert!(invocation.verbose());
    }

    #[test]
    fn release_applies_to_every_command_that_builds_products() {
        for command in ["build", "run", "test"] {
            let invocation = TackInvocation::try_from_arguments(["bray", command, "--release"])
                .unwrap_or_else(|error| panic!("release {command} should parse: {error:?}"));

            let (_, _, _, _, _, _, command) = invocation.into_parts();

            let configuration = match command {
                TackCommand::Build { configuration, .. }
                | TackCommand::Run { configuration, .. }
                | TackCommand::Test { configuration, .. } => configuration,
                command => panic!("expected product-building command, got {command:?}"),
            };

            assert_eq!(configuration, TackBuildConfiguration::Release);
        }
    }

    #[test]
    fn test_options_retain_selection_and_bounded_execution_policy() {
        let invocation = TackInvocation::try_from_arguments([
            "bray",
            "test",
            "parser",
            "recovery",
            "--jobs",
            "3",
            "--timeout-ms",
            "250",
            "--capture-limit",
            "2048",
            "--show-output",
        ])
        .unwrap_or_else(|error| panic!("test options should parse: {error:?}"));

        let (_, _, _, _, _, _, command) = invocation.into_parts();

        let TackCommand::Test { options, .. } = command else {
            panic!("expected test command");
        };

        assert_eq!(options.filters, ["parser", "recovery"]);
        assert_eq!(options.maximum_concurrency, 3);

        assert_eq!(
            options.timeout,
            TestTimeoutPolicy::Limit(TestDuration::from_nanoseconds(250_000_000))
        );

        assert_eq!(
            options.capture,
            TestCapturePolicy::Captured(TestCaptureLimits::new(2048, 4096))
        );

        assert!(options.show_output);
    }

    #[test]
    fn test_jobs_rejects_zero() {
        let result = TackInvocation::try_from_arguments(["bray", "test", "--jobs", "0"]);

        assert!(result.is_err());
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

    #[test]
    fn formatter_configuration_override_is_retained() {
        let invocation = TackInvocation::try_from_arguments([
            "bray",
            "fmt",
            "--config",
            "configuration/brayfmt.json",
            "src/main.bray",
        ])
        .unwrap_or_else(|error| panic!("formatter configuration should parse: {error:?}"));

        let (_, _, _, _, _, _, command) = invocation.into_parts();

        let TackCommand::Format { configuration, .. } = command else {
            panic!("expected formatter command");
        };

        assert_eq!(
            configuration,
            Some(PathBuf::from("configuration/brayfmt.json"))
        );
    }

    #[test]
    fn profile_selection_is_forwarded_with_a_distinct_report_directory() {
        let invocation = TackInvocation::try_from_arguments([
            "bray",
            "--profile=trace",
            "--profile-output",
            "profiles",
            "build",
        ])
        .unwrap_or_else(|error| panic!("profile selection should parse: {error:?}"));

        let (_, _, _, _, _, profile, _) = invocation.into_parts();

        let profile = profile.unwrap_or_else(|| panic!("profile selection must be retained"));

        assert_eq!(profile.mode(), TackProfileMode::Trace);
        assert_eq!(profile.output_directory(), Some(Path::new("profiles")));
    }

    #[test]
    fn trace_requires_a_machine_report_directory() {
        let result = TackInvocation::try_from_arguments(["bray", "--profile=trace", "build"]);

        assert!(result.is_err());
    }
}
