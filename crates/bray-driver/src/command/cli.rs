use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

use bray_compilation::WorkerBudget;
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticKind, SeverityKind,
};
use bray_standard_library::StandardLibraryRoot;
use bray_tooling::{
    InspectionTarget, OutputFormat, clap_styles, exit_code_from_diagnostics, render_styled_text,
};
use clap::error::ErrorKind as ClapErrorKind;
use clap::{Args, Parser, Subcommand};

use crate::command::{
    CliBuildCommand, CliCompilationOptions, DriverCommand, DriverInvocation, DriverOptions,
};

/// Error returned when parsing driver command-line arguments.
#[derive(Debug)]
pub struct DriverCliError {
    kind: DriverCliErrorKind,
}

#[derive(Debug)]
enum DriverCliErrorKind {
    Clap(clap::Error),
    Diagnostics {
        diagnostics: DiagnosticBag,
        output_format: OutputFormat,
    },
}

impl DriverCliError {
    /// Returns the process exit code for this CLI parse error.
    pub fn exit_code(&self) -> ExitCode {
        match &self.kind {
            DriverCliErrorKind::Clap(error) => match error.kind() {
                ClapErrorKind::DisplayHelp
                | ClapErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
                | ClapErrorKind::DisplayVersion => ExitCode::SUCCESS,
                _ => ExitCode::FAILURE,
            },
            DriverCliErrorKind::Diagnostics { diagnostics, .. } => {
                exit_code_from_diagnostics(diagnostics)
            }
        }
    }

    /// Returns the output format selected before this parse failure, when known.
    pub const fn output_format(&self) -> OutputFormat {
        match &self.kind {
            DriverCliErrorKind::Clap(_) => OutputFormat::Text,
            DriverCliErrorKind::Diagnostics { output_format, .. } => *output_format,
        }
    }

    /// Converts this parse failure into structured diagnostics when available.
    pub fn into_diagnostics(self) -> DiagnosticBag {
        match self.kind {
            DriverCliErrorKind::Clap(_) => DiagnosticBag::new(),
            DriverCliErrorKind::Diagnostics { diagnostics, .. } => diagnostics,
        }
    }

    pub(crate) fn into_diagnostics_and_output(self) -> (DiagnosticBag, String, String) {
        match self.kind {
            DriverCliErrorKind::Clap(error) => {
                let (stdout, stderr) = render_clap_error(error);

                (DiagnosticBag::new(), stdout, stderr)
            }
            DriverCliErrorKind::Diagnostics { diagnostics, .. } => {
                (diagnostics, String::new(), String::new())
            }
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
#[command(
    name = "brayc",
    version = env!("CARGO_PKG_VERSION"),
    about = "The Bray compiler",
    styles = clap_styles(),
    arg_required_else_help = true
)]
struct Cli {
    #[command(flatten)]
    options: CliOptions,
    #[command(subcommand)]
    command: CliCommand,
}

fn render_clap_error(error: clap::Error) -> (String, String) {
    let use_stdout = clap_error_uses_stdout(&error);
    let output = render_styled_text(&error.render());

    if use_stdout {
        (output, String::new())
    } else {
        (String::new(), output)
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

impl Cli {
    fn into_driver_invocation(self) -> Result<DriverInvocation, DriverCliError> {
        let output_format = self.options.output_format();

        let options = self
            .options
            .into_driver_options()
            .map_err(|diagnostics| DriverCliError {
                kind: DriverCliErrorKind::Diagnostics {
                    diagnostics,
                    output_format,
                },
            })?;

        let (command, output_file) = self.command.into_driver_parts();

        Ok(DriverInvocation::new(options, command, output_file))
    }
}

#[derive(Args, Debug)]
struct CliOptions {
    #[arg(long = "cpu-count", global = true, value_name = "N")]
    cpu_count: Option<usize>,
    #[arg(long = "format", global = true, value_enum, default_value = "text")]
    format: OutputFormat,
    #[arg(
        long = "standard-library-root",
        global = true,
        value_name = "DIRECTORY"
    )]
    standard_library_root: Option<PathBuf>,
    #[command(flatten)]
    compilation: CliCompilationOptions,
}

impl CliOptions {
    const fn output_format(&self) -> OutputFormat {
        self.format
    }

    fn into_driver_options(self) -> Result<DriverOptions, DiagnosticBag> {
        let worker_budget = match self.cpu_count {
            Some(cpu_count) => match WorkerBudget::new(cpu_count) {
                Ok(worker_budget) => worker_budget,
                Err(error) => return Err(error.into_diagnostic_bag()),
            },
            None => WorkerBudget::default(),
        };

        let compilation = self.compilation.into_configuration()?;

        let standard_library_root = self
            .standard_library_root
            .map(|root| {
                let root =
                    std::path::absolute(root).map_err(|_| invalid_standard_library_root())?;

                StandardLibraryRoot::try_new(root).ok_or_else(invalid_standard_library_root)
            })
            .transpose()?;

        Ok(DriverOptions::new(
            worker_budget,
            self.format,
            compilation,
            standard_library_root,
        ))
    }
}

fn invalid_standard_library_root() -> DiagnosticBag {
    DiagnosticBag::single(
        Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::ProjectCommandSelectionInvalid,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::referenced_name("standard_library_root")),
    )
}

#[derive(Debug, Subcommand)]
enum CliCommand {
    Build(CliBuildCommand),
    Check(CliCheckCommand),
    Inspect(CliInspectCommand),
}

impl CliCommand {
    fn into_driver_parts(self) -> (DriverCommand, Option<PathBuf>) {
        match self {
            Self::Build(command) => (command.into_driver_command(), None),
            Self::Check(check) => (
                DriverCommand::check(check.files, check.emit_interface),
                None,
            ),
            Self::Inspect(command) => command.into_driver_parts(),
        }
    }
}

#[derive(Args, Debug)]
struct CliInspectCommand {
    #[arg(long, global = true, value_name = "PATH")]
    output_file: Option<PathBuf>,
    #[command(subcommand)]
    command: CliInspectSubcommand,
}

impl CliInspectCommand {
    fn into_driver_parts(self) -> (DriverCommand, Option<PathBuf>) {
        let command = match self.command {
            CliInspectSubcommand::Source(files) => DriverCommand::inspect_source(files.files),
            CliInspectSubcommand::Tokens(files) => DriverCommand::inspect_tokens(files.files),
            CliInspectSubcommand::Syntax(files) => DriverCommand::inspect_syntax(files.files),
            CliInspectSubcommand::Declarations(files) => {
                DriverCommand::inspect_declarations(files.files)
            }
            CliInspectSubcommand::Symbols(files) => DriverCommand::inspect_symbols(files.files),
            CliInspectSubcommand::Bound(request) => {
                let target = request.target();

                DriverCommand::inspect_bound(target, request.files)
            }
            CliInspectSubcommand::Lowered(request) => {
                let target = request.target();

                DriverCommand::inspect_lowered(target, request.files)
            }
            CliInspectSubcommand::Mir(request) => {
                let target = request.target();

                DriverCommand::inspect_mir(target, request.files)
            }
        };

        (command, self.output_file)
    }
}

#[derive(Debug, Subcommand)]
enum CliInspectSubcommand {
    Source(CliSourceFiles),
    Tokens(CliSourceFiles),
    Syntax(CliSourceFiles),
    Declarations(CliSourceFiles),
    Symbols(CliSourceFiles),
    Bound(CliUnitInspection),
    Lowered(CliUnitInspection),
    Mir(CliUnitInspection),
}

#[derive(Args, Debug)]
struct CliUnitInspection {
    #[arg(long, default_value_t = 0)]
    source_id: u32,
    #[arg(long)]
    offset: Option<u32>,
    #[arg(value_name = "FILE", num_args = 0..)]
    files: Vec<PathBuf>,
}

impl CliUnitInspection {
    fn target(&self) -> InspectionTarget {
        self.offset.map_or_else(
            || InspectionTarget::source(self.source_id),
            |offset| InspectionTarget::at(self.source_id, offset.into()),
        )
    }
}

#[derive(Args, Debug)]
struct CliSourceFiles {
    #[arg(value_name = "FILE", num_args = 0..)]
    files: Vec<PathBuf>,
}

#[derive(Args, Debug)]
struct CliCheckCommand {
    #[arg(long = "emit-interface", value_name = "PATH")]
    emit_interface: Option<PathBuf>,
    #[arg(value_name = "FILE", num_args = 0..)]
    files: Vec<PathBuf>,
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::PathBuf;

    use bray_compilation::WorkerBudget;
    use bray_diagnostics::DiagnosticKind;
    use bray_runtime_interface::RuntimeCapability;
    use bray_symbols::ProductKind;
    use bray_target::{NativeTarget, TargetOutputKind};
    use bray_tooling::OutputFormat;

    use crate::command::{
        DriverBackend, DriverCommandKind, DriverInspectionArtifact, DriverInvocation,
        DriverRuntimeSelection,
    };

    #[test]
    fn accepts_an_explicit_standard_library_bundle_root() {
        let root = std::env::current_dir()
            .unwrap_or_else(|error| panic!("test directory should be available: {error:?}"))
            .join("standard-library");

        let invocation = DriverInvocation::try_from_arguments([
            OsString::from("brayc"),
            OsString::from("--standard-library-root"),
            root.as_os_str().to_os_string(),
            OsString::from("check"),
        ])
        .unwrap_or_else(|error| panic!("standard-library root should parse: {error:?}"));

        assert_eq!(
            invocation
                .options()
                .standard_library_root()
                .map(bray_standard_library::StandardLibraryRoot::path),
            Some(root.as_path())
        );
    }

    #[test]
    fn parses_typed_build_product_configuration() {
        let invocation = DriverInvocation::try_from_arguments([
            "brayc",
            "build",
            "--product-kind",
            "executable",
            "--target",
            "x86_64-unknown-linux-gnu",
            "--backend",
            "llvm",
            "--release",
            "--runtime-profile",
            "native",
            "--require-capability",
            "main-thread-lane",
            "--require-capability",
            "cooperative-execution",
            "--inspect",
            "backend-ir",
            "--artifact",
            "executable",
            "--output",
            "out",
            "main.bray",
        ])
        .unwrap_or_else(|error| panic!("build invocation should parse: {error:?}"));

        assert_eq!(invocation.command().kind(), DriverCommandKind::Build);
        assert_eq!(invocation.command().files(), [PathBuf::from("main.bray")]);

        let configuration = invocation
            .command()
            .product_configuration()
            .unwrap_or_else(|| panic!("build command must retain product configuration"));

        assert_eq!(
            invocation.options().compilation().product_kind(),
            ProductKind::Executable
        );

        assert_eq!(
            invocation.options().compilation().target(),
            NativeTarget::X86_64LinuxGnu
        );

        assert_eq!(configuration.backend(), DriverBackend::Llvm);

        assert_eq!(
            configuration.build(),
            bray_compilation::BuildConfiguration::Release
        );

        assert_eq!(configuration.artifacts(), &[TargetOutputKind::Executable]);

        assert_eq!(
            configuration.runtime().and_then(|runtime| match runtime {
                DriverRuntimeSelection::Profile(profile) => Some(profile.as_str()),
                DriverRuntimeSelection::Artifact(_) => None,
            }),
            Some("native")
        );

        assert_eq!(
            configuration.required_capabilities(),
            &[
                RuntimeCapability::CooperativeExecution,
                RuntimeCapability::MainThreadLane,
            ]
        );

        assert_eq!(
            configuration.inspections(),
            &[DriverInspectionArtifact::BackendIr]
        );

        assert_eq!(configuration.output(), std::path::Path::new("out"));
    }

    #[test]
    fn parses_every_supported_native_target() {
        for target in NativeTarget::ALL {
            let invocation = DriverInvocation::try_from_arguments([
                "brayc",
                "build",
                "--target",
                target.as_str(),
                "--output",
                "out",
                "main.bray",
            ])
            .unwrap_or_else(|error| {
                panic!("native target invocation should parse for {target:?}: {error:?}")
            });

            assert_eq!(invocation.options().compilation().target(), target);
        }
    }

    #[test]
    fn parses_dependency_identities_and_paths_without_combining_path_text() {
        let invocation = DriverInvocation::try_from_arguments([
            OsString::from("brayc"),
            OsString::from("--dependency-product"),
            OsString::from("example.math/math"),
            OsString::from("--dependency-interface"),
            PathBuf::from("interfaces/math.brayi").into_os_string(),
            OsString::from("check"),
            OsString::from("main.bray"),
        ])
        .unwrap_or_else(|error| panic!("dependency invocation should parse: {error:?}"));

        let dependencies = invocation.options().compilation().dependencies();

        let [dependency] = dependencies else {
            panic!("one dependency interface should be retained: {dependencies:#?}");
        };

        assert_eq!(dependency.package().as_str(), "example.math");
        assert_eq!(dependency.product().as_str(), "math");

        assert_eq!(
            dependency.path(),
            std::path::Path::new("interfaces/math.brayi")
        );
    }

    #[test]
    fn parses_package_semantic_versions() {
        let invocation = DriverInvocation::try_from_arguments([
            "brayc",
            "--package-version",
            "2.3.4-beta.1+build.5",
            "check",
            "main.bray",
        ])
        .unwrap_or_else(|error| panic!("package version should parse: {error:?}"));

        assert_eq!(
            invocation
                .options()
                .compilation()
                .package_version()
                .to_string(),
            "2.3.4-beta.1+build.5"
        );
    }

    #[test]
    fn rejects_unpaired_dependency_inputs() {
        assert!(
            DriverInvocation::try_from_arguments([
                "brayc",
                "--dependency-product",
                "example.math/math",
                "check",
                "main.bray",
            ])
            .is_err()
        );
    }

    #[test]
    fn rejects_multiple_runtime_selection_forms() {
        assert!(
            DriverInvocation::try_from_arguments([
                "brayc",
                "build",
                "--runtime-artifact",
                "runtime.json",
                "--runtime-profile",
                "native",
                "--output",
                "out",
                "main.bray",
            ])
            .is_err()
        );
    }

    #[test]
    fn rejects_empty_runtime_profile() {
        assert!(
            DriverInvocation::try_from_arguments([
                "brayc",
                "build",
                "--runtime-profile",
                "",
                "--output",
                "out",
                "main.bray",
            ])
            .is_err()
        );
    }

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
        assert_eq!(invocation.options().output_format(), OutputFormat::Json);
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
        assert_eq!(invocation.options().output_format(), OutputFormat::Text);

        assert_eq!(
            invocation.command().kind(),
            DriverCommandKind::InspectSource
        );

        let files = [PathBuf::from("main.bray"), PathBuf::from("lib.bray")];

        assert_eq!(invocation.command().files(), files.as_slice());
    }

    #[test]
    fn parses_inspect_tokens_command_with_subcommand_options() {
        let invocation = match DriverInvocation::try_from_arguments([
            "brayc",
            "inspect",
            "tokens",
            "--format",
            "json",
            "--cpu-count",
            "1",
            "main.bray",
            "lib.bray",
        ]) {
            Ok(invocation) => invocation,
            Err(error) => panic!("inspect tokens invocation should parse: {error:?}"),
        };

        assert_eq!(invocation.options().worker_budget(), WorkerBudget::serial());

        assert_eq!(invocation.options().output_format(), OutputFormat::Json);

        assert_eq!(
            invocation.command().kind(),
            DriverCommandKind::InspectTokens
        );

        let files = [PathBuf::from("main.bray"), PathBuf::from("lib.bray")];

        assert_eq!(invocation.command().files(), files.as_slice());
    }

    #[test]
    fn parses_inspect_syntax_command_with_subcommand_options() {
        let invocation = match DriverInvocation::try_from_arguments([
            "brayc",
            "inspect",
            "syntax",
            "--format",
            "json",
            "--cpu-count",
            "1",
            "main.bray",
            "lib.bray",
        ]) {
            Ok(invocation) => invocation,
            Err(error) => panic!("inspect syntax invocation should parse: {error:?}"),
        };

        assert_eq!(invocation.options().worker_budget(), WorkerBudget::serial());
        assert_eq!(invocation.options().output_format(), OutputFormat::Json);

        assert_eq!(
            invocation.command().kind(),
            DriverCommandKind::InspectSyntax
        );

        let files = [PathBuf::from("main.bray"), PathBuf::from("lib.bray")];

        assert_eq!(invocation.command().files(), files.as_slice());
    }

    #[test]
    fn parses_inspect_declarations_command_with_subcommand_options() {
        let invocation = match DriverInvocation::try_from_arguments([
            "brayc",
            "inspect",
            "declarations",
            "--format",
            "json",
            "--cpu-count",
            "1",
            "main.bray",
            "lib.bray",
        ]) {
            Ok(invocation) => invocation,
            Err(error) => panic!("inspect declarations invocation should parse: {error:?}"),
        };

        assert_eq!(invocation.options().worker_budget(), WorkerBudget::serial());
        assert_eq!(invocation.options().output_format(), OutputFormat::Json);

        assert_eq!(
            invocation.command().kind(),
            DriverCommandKind::InspectDeclarations
        );

        let files = [PathBuf::from("main.bray"), PathBuf::from("lib.bray")];

        assert_eq!(invocation.command().files(), files.as_slice());
    }

    #[test]
    fn parses_inspect_symbols_command_with_subcommand_options() {
        let invocation = match DriverInvocation::try_from_arguments([
            "brayc",
            "inspect",
            "symbols",
            "--format",
            "json",
            "--cpu-count",
            "1",
            "main.bray",
            "lib.bray",
        ]) {
            Ok(invocation) => invocation,
            Err(error) => panic!("inspect symbols invocation should parse: {error:?}"),
        };

        assert_eq!(invocation.options().worker_budget(), WorkerBudget::serial());
        assert_eq!(invocation.options().output_format(), OutputFormat::Json);

        assert_eq!(
            invocation.command().kind(),
            DriverCommandKind::InspectSymbols
        );

        let files = [PathBuf::from("main.bray"), PathBuf::from("lib.bray")];

        assert_eq!(invocation.command().files(), files.as_slice());
    }

    #[test]
    fn parses_inspect_bound_command_with_source_target() {
        let invocation = match DriverInvocation::try_from_arguments([
            "brayc",
            "inspect",
            "bound",
            "--source-id",
            "2",
            "--offset",
            "31",
            "--format",
            "json",
            "main.bray",
        ]) {
            Ok(invocation) => invocation,
            Err(error) => panic!("inspect bound invocation should parse: {error:?}"),
        };

        assert_eq!(invocation.command().kind(), DriverCommandKind::InspectBound);

        let target = invocation
            .command()
            .unit_inspection_target()
            .unwrap_or_else(|| panic!("inspect bound command must retain its target"));

        assert_eq!(target.source_id(), 2);
        assert_eq!(target.position(), Some(31.into()));
        assert_eq!(invocation.command().files(), [PathBuf::from("main.bray")]);
    }

    #[test]
    fn parses_inspect_bound_command_without_a_position_filter() {
        let invocation = match DriverInvocation::try_from_arguments([
            "brayc",
            "inspect",
            "bound",
            "main.bray",
        ]) {
            Ok(invocation) => invocation,
            Err(error) => panic!("inspect bound invocation should parse: {error:?}"),
        };

        let target = invocation
            .command()
            .unit_inspection_target()
            .unwrap_or_else(|| panic!("inspect bound command must retain its source target"));

        assert_eq!(target.source_id(), 0);
        assert_eq!(target.position(), None);
        assert_eq!(invocation.command().files(), [PathBuf::from("main.bray")]);
    }

    #[test]
    fn parses_lowered_and_mir_inspection_commands() {
        let cases = [
            ("lowered", DriverCommandKind::InspectLowered),
            ("mir", DriverCommandKind::InspectMir),
        ];

        for (command, expected_kind) in cases {
            let invocation = DriverInvocation::try_from_arguments([
                "brayc",
                "inspect",
                command,
                "--source-id",
                "3",
                "--offset",
                "12",
                "main.bray",
            ])
            .unwrap_or_else(|error| panic!("inspect {command} should parse: {error:?}"));

            assert_eq!(invocation.command().kind(), expected_kind);

            let target = invocation
                .command()
                .unit_inspection_target()
                .unwrap_or_else(|| panic!("inspect {command} must retain its target"));

            assert_eq!(target.source_id(), 3);
            assert_eq!(target.position(), Some(12.into()));
        }
    }

    #[test]
    fn parses_report_file_for_every_inspection_command() {
        let commands = [
            "source",
            "tokens",
            "syntax",
            "declarations",
            "symbols",
            "bound",
            "lowered",
            "mir",
        ];

        for command in commands {
            let invocation = DriverInvocation::try_from_arguments([
                "brayc",
                "inspect",
                command,
                "--output-file",
                "report.txt",
                "main.bray",
            ])
            .unwrap_or_else(|error| {
                panic!("inspect {command} should accept a report file: {error:?}")
            });

            assert_eq!(
                invocation.output_file(),
                Some(std::path::Path::new("report.txt"))
            );
        }
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
