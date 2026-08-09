use std::ffi::OsString;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use bray_base::{FileReplacementMode, StagedFile};
use bray_compilation::{Compilation, CompilationProfileReport, CompilationRequest};
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticIoErrorKind, DiagnosticKind,
    SeverityKind,
};
use bray_tooling::{
    InspectionOutput, OutputFormat, compilation_request_from_file_arguments,
    exit_code_from_diagnostics, load_compilation, render_bound_inspection,
    render_declaration_inspection, render_lowered_inspection, render_mir_inspection,
    render_source_inspection, render_symbol_inspection, render_syntax_inspection,
    render_token_inspection,
};

use crate::command::{DriverCommand, DriverCommandKind, DriverInvocation, DriverOptions};
use crate::run::{run_build_command, write_driver_output, write_driver_output_error};

/// Structured result from running the Bray compiler driver.
#[derive(Debug)]
pub struct DriverRunResult {
    exit_code: ExitCode,
    diagnostics: DiagnosticBag,
    compilation: Option<Compilation>,
    output_format: OutputFormat,
    stdout: String,
    stderr: String,
    report_file: Option<PathBuf>,
    profile: Option<CompilationProfileReport>,
    profile_output: Option<PathBuf>,
}

impl DriverRunResult {
    pub(super) fn new(
        exit_code: ExitCode,
        diagnostics: DiagnosticBag,
        output_format: OutputFormat,
    ) -> Self {
        Self::with_output(
            exit_code,
            diagnostics,
            output_format,
            String::new(),
            String::new(),
        )
    }

    fn with_output(
        exit_code: ExitCode,
        diagnostics: DiagnosticBag,
        output_format: OutputFormat,
        stdout: String,
        stderr: String,
    ) -> Self {
        Self {
            exit_code,
            diagnostics,
            compilation: None,
            output_format,
            stdout,
            stderr,
            report_file: None,
            profile: None,
            profile_output: None,
        }
    }

    fn with_compilation(
        exit_code: ExitCode,
        diagnostics: DiagnosticBag,
        output_format: OutputFormat,
        compilation: Compilation,
    ) -> Self {
        let profile = compilation.profile_report();

        Self {
            exit_code,
            diagnostics,
            compilation: Some(compilation),
            output_format,
            stdout: String::new(),
            stderr: String::new(),
            report_file: None,
            profile,
            profile_output: None,
        }
    }

    fn with_output_and_compilation(
        exit_code: ExitCode,
        diagnostics: DiagnosticBag,
        output_format: OutputFormat,
        stdout: String,
        compilation: Compilation,
    ) -> Self {
        let mut result = Self::with_compilation(exit_code, diagnostics, output_format, compilation);

        result.stdout = stdout;

        result
    }

    /// Returns the process exit code selected by the driver.
    pub const fn exit_code(&self) -> ExitCode {
        self.exit_code
    }

    /// Returns diagnostics produced by the driver operation.
    pub const fn diagnostics(&self) -> &DiagnosticBag {
        &self.diagnostics
    }

    pub(crate) fn sources(&self) -> Option<&bray_source::SourceStore> {
        self.compilation.as_ref().map(Compilation::sources)
    }

    /// Returns the output format selected for driver-produced output.
    pub const fn output_format(&self) -> OutputFormat {
        self.output_format
    }

    /// Returns driver-owned stdout text, such as help or version output.
    pub fn stdout(&self) -> &str {
        &self.stdout
    }

    /// Returns driver-owned stderr text, such as command-line parser errors.
    pub fn stderr(&self) -> &str {
        &self.stderr
    }

    /// Returns the destination that should receive a copy of an inspection report.
    pub fn report_file(&self) -> Option<&Path> {
        self.report_file.as_deref()
    }

    /// Returns the immutable compiler profile when profiling was enabled.
    pub const fn profile(&self) -> Option<&CompilationProfileReport> {
        self.profile.as_ref()
    }

    /// Returns the destination for the machine-readable compiler profile.
    pub fn profile_output(&self) -> Option<&Path> {
        self.profile_output.as_deref()
    }

    /// Returns whether this result carries driver-owned terminal output.
    pub fn has_terminal_output(&self) -> bool {
        !self.stdout.is_empty() || !self.stderr.is_empty()
    }

    fn with_report_file(mut self, report_file: Option<PathBuf>) -> Self {
        self.report_file = report_file;

        self
    }

    pub(super) fn with_profile_output(mut self, profile_output: Option<PathBuf>) -> Self {
        self.profile_output = profile_output;

        self
    }
}

/// Runs the Bray compiler driver for the provided process arguments.
pub fn run(arguments: impl IntoIterator<Item = OsString>) -> ExitCode {
    let result = run_result(arguments);
    let mut stdout = io::stdout().lock();
    let mut stderr = io::stderr().lock();

    write_run_result(&result, &mut stdout, &mut stderr)
}

/// Runs the Bray compiler driver and returns its structured outcome.
pub fn run_result(arguments: impl IntoIterator<Item = OsString>) -> DriverRunResult {
    let invocation = match DriverInvocation::try_from_arguments(arguments) {
        Ok(invocation) => invocation,
        Err(error) => {
            let exit_code = error.exit_code();
            let output_format = error.output_format();

            let (diagnostics, stdout, stderr) = error.into_diagnostics_and_output();

            return DriverRunResult::with_output(
                exit_code,
                diagnostics,
                output_format,
                stdout,
                stderr,
            );
        }
    };

    let (options, command, report_file) = invocation.into_parts();

    let output_format = options.output_format();
    let profile_output = options.profile_output().map(Path::to_path_buf);

    let command = match command {
        DriverCommand::Build {
            configuration,
            files,
        } => {
            return run_build_command(&options, configuration, files, output_format)
                .with_profile_output(profile_output);
        }
        command => command,
    };

    let command_kind = command.kind();
    let unit_inspection_target = command.unit_inspection_target();
    let interface_output = command.interface_output().map(Path::to_path_buf);

    let request =
        match compilation_request(&options, command.into_files(), interface_output.is_some()) {
            Ok(request) => request,
            Err(diagnostics) => {
                let exit_code = exit_code_from_diagnostics(&diagnostics);

                return DriverRunResult::new(exit_code, diagnostics, output_format);
            }
        };

    if command_kind == DriverCommandKind::Check {
        return run_check_command(request, interface_output, output_format)
            .with_profile_output(profile_output);
    }

    if command_kind == DriverCommandKind::InspectSource {
        return run_inspect_source_command(request, output_format)
            .with_report_file(report_file)
            .with_profile_output(profile_output);
    }

    if command_kind == DriverCommandKind::InspectTokens {
        return run_fact_inspection_command(request, output_format, render_token_inspection)
            .with_report_file(report_file)
            .with_profile_output(profile_output);
    }

    if command_kind == DriverCommandKind::InspectSyntax {
        return run_fact_inspection_command(request, output_format, render_syntax_inspection)
            .with_report_file(report_file)
            .with_profile_output(profile_output);
    }

    if command_kind == DriverCommandKind::InspectDeclarations {
        return run_fact_inspection_command(request, output_format, render_declaration_inspection)
            .with_report_file(report_file)
            .with_profile_output(profile_output);
    }

    if command_kind == DriverCommandKind::InspectSymbols {
        return run_fact_inspection_command(request, output_format, render_symbol_inspection)
            .with_report_file(report_file)
            .with_profile_output(profile_output);
    }

    if command_kind == DriverCommandKind::InspectBound {
        let Some(target) = unit_inspection_target else {
            unreachable!("bound inspection command must retain its source target");
        };

        return run_fact_inspection_command(
            request,
            output_format,
            |compilation, output_format| {
                render_bound_inspection(compilation, target, output_format)
            },
        )
        .with_report_file(report_file)
        .with_profile_output(profile_output);
    }

    if command_kind == DriverCommandKind::InspectLowered {
        let Some(target) = unit_inspection_target else {
            unreachable!("lowered inspection command must retain its source target");
        };

        return run_fact_inspection_command(
            request,
            output_format,
            |compilation, output_format| {
                render_lowered_inspection(compilation, target, output_format)
            },
        )
        .with_report_file(report_file)
        .with_profile_output(profile_output);
    }

    if command_kind == DriverCommandKind::InspectMir {
        let Some(target) = unit_inspection_target else {
            unreachable!("MIR inspection command must retain its source target");
        };

        return run_fact_inspection_command(
            request,
            output_format,
            |compilation, output_format| render_mir_inspection(compilation, target, output_format),
        )
        .with_report_file(report_file)
        .with_profile_output(profile_output);
    }

    match command_kind {
        DriverCommandKind::Build
        | DriverCommandKind::Check
        | DriverCommandKind::InspectSource
        | DriverCommandKind::InspectTokens
        | DriverCommandKind::InspectSyntax
        | DriverCommandKind::InspectDeclarations
        | DriverCommandKind::InspectSymbols
        | DriverCommandKind::InspectBound
        | DriverCommandKind::InspectLowered
        | DriverCommandKind::InspectMir => {
            unreachable!("handled command kind did not return")
        }
    }
}

fn run_check_command(
    request: CompilationRequest,
    interface_output: Option<PathBuf>,
    output_format: OutputFormat,
) -> DriverRunResult {
    let compilation = match load_compilation(request) {
        Some(compilation) => compilation,
        None => return compilation_load_failure_result(output_format),
    };

    let mut diagnostics = compilation.check_diagnostics().clone();

    if !diagnostics.has_errors()
        && let Some(path) = interface_output
    {
        let diagnostic_id = DiagnosticId::from_index(diagnostics.len());

        if let Err(error) = publish_package_interface(&compilation, &path, diagnostic_id) {
            diagnostics.add(error);

            return driver_result_from_compilation(
                compilation,
                diagnostics,
                output_format,
                ExitCode::FAILURE,
            );
        }
    }

    diagnostic_result_from_compilation(compilation, diagnostics, output_format)
}

fn run_inspect_source_command(
    request: CompilationRequest,
    output_format: OutputFormat,
) -> DriverRunResult {
    let compilation = match load_compilation(request) {
        Some(compilation) => compilation,
        None => return compilation_load_failure_result(output_format),
    };

    if !compilation.source_diagnostics().is_empty() {
        let diagnostics = compilation.source_diagnostics().clone();

        return diagnostic_result_from_compilation(compilation, diagnostics, output_format);
    }

    let stdout = match render_source_inspection(&compilation, output_format) {
        Ok(stdout) => stdout,
        Err(_) => return compilation_load_failure_result(output_format),
    };

    DriverRunResult::with_output_and_compilation(
        ExitCode::SUCCESS,
        DiagnosticBag::new(),
        output_format,
        stdout,
        compilation,
    )
}

fn run_fact_inspection_command<E>(
    request: CompilationRequest,
    output_format: OutputFormat,
    render: impl FnOnce(&Compilation, OutputFormat) -> Result<InspectionOutput, E>,
) -> DriverRunResult {
    let compilation = match load_compilation(request) {
        Some(compilation) => compilation,
        None => return compilation_load_failure_result(output_format),
    };

    if !compilation.source_diagnostics().is_empty() {
        let diagnostics = compilation.source_diagnostics().clone();

        return diagnostic_result_from_compilation(compilation, diagnostics, output_format);
    }

    let output = match render(&compilation, output_format) {
        Ok(output) => output,
        Err(_) => return compilation_load_failure_result(output_format),
    };

    let (stdout, diagnostics) = output.into_parts();

    let exit_code = exit_code_from_diagnostics(&diagnostics);

    DriverRunResult::with_output_and_compilation(
        exit_code,
        diagnostics,
        output_format,
        stdout,
        compilation,
    )
}

fn diagnostic_result_from_compilation(
    compilation: Compilation,
    diagnostics: DiagnosticBag,
    output_format: OutputFormat,
) -> DriverRunResult {
    let exit_code = exit_code_from_diagnostics(&diagnostics);

    driver_result_from_compilation(compilation, diagnostics, output_format, exit_code)
}

pub(super) fn driver_result_from_compilation(
    compilation: Compilation,
    diagnostics: DiagnosticBag,
    output_format: OutputFormat,
    exit_code: ExitCode,
) -> DriverRunResult {
    DriverRunResult::with_compilation(exit_code, diagnostics, output_format, compilation)
}

fn compilation_load_failure_result(output_format: OutputFormat) -> DriverRunResult {
    DriverRunResult::new(ExitCode::FAILURE, DiagnosticBag::new(), output_format)
}

pub(super) fn compilation_request(
    options: &DriverOptions,
    files: Vec<PathBuf>,
    export_interface: bool,
) -> Result<CompilationRequest, DiagnosticBag> {
    let configuration = options.compilation();

    let mut request = compilation_request_from_file_arguments(
        configuration.source_package().clone(),
        files,
        options.compilation_options(),
    )?;

    if options.package_source_authority().is_standard_library() {
        request = request.with_standard_library_source_authority();
    }

    let dependencies = configuration
        .dependencies()
        .iter()
        .map(crate::command::DriverDependencyInterface::load)
        .collect::<Result<Vec<_>, _>>()?;

    request = request.with_dependency_interfaces(dependencies);

    if let Some(profile) = options.profile() {
        // The request retains immutable package-product identity beyond driver configuration.
        request = request
            .with_profile(profile)
            .with_profile_product(configuration.product().clone());
    }

    if let Some(root) = options.standard_library_root() {
        // Compilation requests retain the selected immutable bundle-root identity.
        request = request.with_standard_library_root(root.clone());
    }

    if export_interface {
        request =
            request.with_package_interface_export(bray_tooling::package_interface_export_request(
                configuration.product().clone(),
                configuration.package_version(),
            ));
    }

    Ok(request)
}

fn publish_package_interface(
    compilation: &Compilation,
    destination: &Path,
    diagnostic_id: DiagnosticId,
) -> Result<(), Diagnostic> {
    let bundle = compilation
        .package_interface_export_bundle()
        .and_then(|result| result.as_ref().ok())
        .ok_or_else(|| publication_diagnostic(diagnostic_id, destination, io::ErrorKind::Other))?;

    let artifact = bray_package_interface::encode_package_interface(bundle).map_err(|_| {
        publication_diagnostic(diagnostic_id, destination, io::ErrorKind::InvalidData)
    })?;

    let implementation =
        bray_package_interface::PackageImplementationArtifact::try_from_export_bundle(
            &artifact,
            bundle,
            bray_package_interface::InterfaceValidationLimits::default(),
        )
        .map_err(|_| {
            publication_diagnostic(diagnostic_id, destination, io::ErrorKind::InvalidData)
        })?;

    publish_artifact(destination, artifact.bytes(), diagnostic_id)?;

    let implementation_destination = destination.with_extension("brayimpl");

    publish_artifact(
        &implementation_destination,
        implementation.bytes(),
        diagnostic_id,
    )
}

fn publish_artifact(
    destination: &Path,
    bytes: &[u8],
    diagnostic_id: DiagnosticId,
) -> Result<(), Diagnostic> {
    let mut staging =
        StagedFile::create(destination, FileReplacementMode::ReplaceExisting, None)
            .map_err(|error| publication_diagnostic(diagnostic_id, destination, error.kind()))?;

    staging
        .write_all(bytes)
        .map_err(|error| publication_diagnostic(diagnostic_id, destination, error.kind()))?;

    staging
        .finish()
        .and_then(|staged| staged.promote(destination))
        .map_err(|error| publication_diagnostic(diagnostic_id, destination, error.kind()))
}

fn publication_diagnostic(id: DiagnosticId, path: &Path, kind: io::ErrorKind) -> Diagnostic {
    Diagnostic::new(
        id,
        DiagnosticKind::EmissionArtifactWriteFailed,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::file_path(path))
    .with_arg(DiagnosticArg::io_error_kind(DiagnosticIoErrorKind::from(
        kind,
    )))
}

#[cfg(test)]
fn run_with_writers(
    arguments: impl IntoIterator<Item = OsString>,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> ExitCode {
    let result = run_result(arguments);

    write_run_result(&result, stdout, stderr)
}

fn write_run_result(
    result: &DriverRunResult,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> ExitCode {
    match write_driver_output(result, stdout, stderr) {
        Ok(()) => result.exit_code(),
        Err(error) => {
            let _ = write_driver_output_error(error, result.output_format(), stdout, stderr);

            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::process::ExitCode;

    use bray_diagnostics::DiagnosticKind;

    use super::{run_result, run_with_writers};
    use crate::test_support::{TemporaryFile, unique_temporary_directory};

    #[test]
    fn profiling_is_absent_unless_explicitly_requested() {
        let file = TemporaryFile::write("main.bray", b"module app;\n");

        let result = run_result([
            OsString::from("brayc"),
            OsString::from("check"),
            file.path().as_os_str().to_os_string(),
        ]);

        assert!(result.profile().is_none());
    }

    #[test]
    fn summary_profiling_returns_aggregates_without_trace_events() {
        let file = TemporaryFile::write("main.bray", b"module app;\n");

        let result = run_result([
            OsString::from("brayc"),
            OsString::from("--profile=summary"),
            OsString::from("check"),
            file.path().as_os_str().to_os_string(),
        ]);

        let profile = result
            .profile()
            .unwrap_or_else(|| panic!("requested profile must be returned"));

        assert_eq!(
            profile.mode,
            bray_compilation::CompilationProfileMode::Summary
        );

        assert!(!profile.queries.is_empty());
        assert!(profile.events.is_empty());
    }

    #[test]
    fn trace_profiling_writes_a_versioned_machine_report_and_human_summary() {
        let file = TemporaryFile::write("main.bray", b"module app;\n");
        let report = TemporaryFile::write("profile.json", b"stale");

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let exit_code = run_with_writers(
            [
                OsString::from("brayc"),
                OsString::from("--profile=trace"),
                OsString::from("--profile-output"),
                report.path().as_os_str().to_os_string(),
                OsString::from("check"),
                file.path().as_os_str().to_os_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(exit_code, ExitCode::SUCCESS);

        let bytes = std::fs::read(report.path())
            .unwrap_or_else(|error| panic!("profile report must be readable: {error:?}"));

        let profile: bray_compilation::CompilationProfileReport = serde_json::from_slice(&bytes)
            .unwrap_or_else(|error| panic!("profile report must match its schema: {error:?}"));

        assert_eq!(
            profile.schema_revision,
            bray_profile::COMPILATION_PROFILE_SCHEMA_REVISION
        );

        assert_eq!(profile.mode, bray_compilation::CompilationProfileMode::Trace);
        assert_eq!(profile.context.product, "library");
        assert_eq!(bytes.iter().filter(|byte| **byte == b'\n').count(), 1);
        assert!(!profile.descriptors.operations.is_empty());
        assert!(!profile.descriptors.queries.is_empty());
        assert!(!profile.events.is_empty());
        assert!(profile.events.iter().any(|event| event.subject.is_some()));

        let stderr = String::from_utf8(stderr)
            .unwrap_or_else(|error| panic!("profile summary must be UTF-8: {error:?}"));

        assert!(stderr.contains("Compiler profile:"));
        assert!(stderr.contains("Top operations by worker self time"));
        assert!(stderr.contains("Top queries by evaluation time"));
        assert!(stderr.contains("Trace:"));
    }

    #[test]
    fn run_fails_when_check_diagnostics_have_errors() {
        let file = TemporaryFile::write("bad.bray", &[0xff]);

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        assert_eq!(
            run_with_writers(
                [
                    OsString::from("brayc"),
                    OsString::from("check"),
                    file.path().as_os_str().to_os_string(),
                ],
                &mut stdout,
                &mut stderr,
            ),
            ExitCode::FAILURE
        );
    }

    #[test]
    fn run_result_carries_check_diagnostics() {
        let file = TemporaryFile::write("bad.bray", &[0xff]);

        let result = run_result([
            OsString::from("brayc"),
            OsString::from("check"),
            file.path().as_os_str().to_os_string(),
        ]);

        assert_eq!(result.exit_code(), ExitCode::FAILURE);

        assert_eq!(
            result
                .diagnostics()
                .by_kind(DiagnosticKind::SourceInvalidUtf8)
                .count(),
            1
        );
    }

    #[test]
    fn run_result_carries_invalid_worker_budget_diagnostics() {
        let result = run_result([
            OsString::from("brayc"),
            OsString::from("--format"),
            OsString::from("json"),
            OsString::from("--cpu-count"),
            OsString::from("0"),
            OsString::from("check"),
            OsString::from("main.bray"),
        ]);

        assert_eq!(result.exit_code(), ExitCode::FAILURE);
        assert_eq!(result.output_format(), crate::OutputFormat::Json);

        assert_eq!(
            result
                .diagnostics()
                .by_kind(DiagnosticKind::RequestInvalidWorkerBudget)
                .count(),
            1
        );
    }

    #[test]
    fn run_result_carries_missing_source_input_diagnostics() {
        let result = run_result([OsString::from("brayc"), OsString::from("check")]);

        assert_eq!(result.exit_code(), ExitCode::FAILURE);

        assert_eq!(
            result
                .diagnostics()
                .by_kind(DiagnosticKind::RequestMissingSourceInput)
                .count(),
            1
        );
    }

    #[test]
    fn run_result_carries_file_read_diagnostics() {
        let missing_path = unique_temporary_directory().join("missing.bray");

        let result = run_result([
            OsString::from("brayc"),
            OsString::from("check"),
            missing_path.as_os_str().to_os_string(),
        ]);

        assert_eq!(result.exit_code(), ExitCode::FAILURE);

        assert_eq!(
            result
                .diagnostics()
                .by_kind(DiagnosticKind::SourceFileReadFailed)
                .count(),
            1
        );
    }

    #[test]
    fn check_runs_parser_and_carries_lexical_diagnostics() {
        let file = TemporaryFile::write("bad.bray", b"$");

        let result = run_result([
            OsString::from("brayc"),
            OsString::from("check"),
            file.path().as_os_str().to_os_string(),
        ]);

        assert_eq!(result.exit_code(), ExitCode::FAILURE);

        assert_eq!(
            result
                .diagnostics()
                .by_kind(DiagnosticKind::LexicalInvalidCharacter)
                .count(),
            1
        );
    }

    #[test]
    fn inspect_source_does_not_run_lexing_or_parsing() {
        let file = TemporaryFile::write("bad.bray", b"$");

        let result = run_result([
            OsString::from("brayc"),
            OsString::from("inspect"),
            OsString::from("source"),
            file.path().as_os_str().to_os_string(),
        ]);

        assert_eq!(result.exit_code(), ExitCode::SUCCESS);
        assert!(result.diagnostics().is_empty());
        assert!(result.stdout().contains("$"));
    }

    #[test]
    fn run_writes_help_to_stdout() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let exit_code = run_with_writers(
            [OsString::from("brayc"), OsString::from("--help")],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(exit_code, ExitCode::SUCCESS);
        assert!(stderr.is_empty());

        let stdout = match String::from_utf8(stdout) {
            Ok(stdout) => stdout,
            Err(error) => panic!("stdout should be UTF-8: {error:?}"),
        };

        assert!(stdout.contains("The Bray compiler"));
        assert!(stdout.contains("\x1b[36mUsage:\x1b[0m"));
        assert!(stdout.contains("Usage:"));
        assert!(stdout.contains("check"));
        assert!(stdout.contains("--version"));
    }

    #[test]
    fn run_writes_subcommand_help_to_stdout() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let exit_code = run_with_writers(
            [
                OsString::from("brayc"),
                OsString::from("check"),
                OsString::from("--help"),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(exit_code, ExitCode::SUCCESS);
        assert!(stderr.is_empty());

        let stdout = match String::from_utf8(stdout) {
            Ok(stdout) => stdout,
            Err(error) => panic!("stdout should be UTF-8: {error:?}"),
        };

        assert!(stdout.contains("Usage:"));
        assert!(stdout.contains("FILE"));
    }

    #[test]
    fn run_writes_help_to_stdout_when_no_arguments_are_provided() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let exit_code = run_with_writers([OsString::from("brayc")], &mut stdout, &mut stderr);

        assert_eq!(exit_code, ExitCode::SUCCESS);
        assert!(stderr.is_empty());
        assert!(!stdout.is_empty());
    }

    #[test]
    fn run_writes_version_to_stdout() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let exit_code = run_with_writers(
            [OsString::from("brayc"), OsString::from("--version")],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(exit_code, ExitCode::SUCCESS);
        assert!(stderr.is_empty());

        let stdout = match String::from_utf8(stdout) {
            Ok(stdout) => stdout,
            Err(error) => panic!("stdout should be UTF-8: {error:?}"),
        };

        assert_eq!(
            stdout.trim(),
            format!("brayc {}", env!("CARGO_PKG_VERSION"))
        );
    }

    #[test]
    fn run_writes_clap_errors_to_stderr() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let exit_code = run_with_writers(
            [OsString::from("brayc"), OsString::from("--unknown")],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(exit_code, ExitCode::FAILURE);
        assert!(stdout.is_empty());

        let stderr = match String::from_utf8(stderr) {
            Ok(stderr) => stderr,
            Err(error) => panic!("stderr should be UTF-8: {error:?}"),
        };

        assert!(stderr.contains("\x1b[31merror:\x1b[0m"));
    }

    #[test]
    fn run_writes_text_diagnostics_to_stderr() {
        let file = TemporaryFile::write("bad.bray", &[0xff]);

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let exit_code = run_with_writers(
            [
                OsString::from("brayc"),
                OsString::from("check"),
                file.path().as_os_str().to_os_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(exit_code, ExitCode::FAILURE);
        assert!(stdout.is_empty());

        let stderr = match String::from_utf8(stderr) {
            Ok(stderr) => stderr,
            Err(error) => panic!("stderr should be UTF-8: {error:?}"),
        };

        assert!(stderr.contains("error E1002"));
        assert!(stderr.contains("source input contains invalid UTF-8"));
        assert!(!stderr.contains("byte offset 0"));
        assert!(!stderr.contains("source_invalid_utf8"));
    }

    #[test]
    fn run_writes_lexical_diagnostic_locations_as_source_line_columns() {
        let file = TemporaryFile::write("bad.bray", b"$");

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let exit_code = run_with_writers(
            [
                OsString::from("brayc"),
                OsString::from("check"),
                file.path().as_os_str().to_os_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(exit_code, ExitCode::FAILURE);
        assert!(stdout.is_empty());

        let stderr = match String::from_utf8(stderr) {
            Ok(stderr) => stderr,
            Err(error) => panic!("stderr should be UTF-8: {error:?}"),
        };

        assert!(stderr.contains("bad.bray:1:1"));
        assert!(!stderr.contains("source 0:0..1"));
    }

    #[test]
    fn run_recovers_lone_cr_as_a_line_break_for_later_diagnostics() {
        let file = TemporaryFile::write("bad.bray", b"b\rc\n/* open");

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let exit_code = run_with_writers(
            [
                OsString::from("brayc"),
                OsString::from("check"),
                file.path().as_os_str().to_os_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(exit_code, ExitCode::FAILURE);
        assert!(stdout.is_empty());

        let stderr = match String::from_utf8(stderr) {
            Ok(stderr) => stderr,
            Err(error) => panic!("stderr should be UTF-8: {error:?}"),
        };

        assert!(stderr.contains("bad.bray:1:2"));
        assert!(stderr.contains("bad.bray:3:1..3:7"));
        assert!(!stderr.contains("bad.bray:2:1..2:7"));
    }

    #[test]
    fn run_writes_json_diagnostics_to_stdout() {
        let file = TemporaryFile::write("bad.bray", &[0xff]);

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let exit_code = run_with_writers(
            [
                OsString::from("brayc"),
                OsString::from("--format"),
                OsString::from("json"),
                OsString::from("check"),
                file.path().as_os_str().to_os_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(exit_code, ExitCode::FAILURE);
        assert!(stderr.is_empty());

        let stdout = match String::from_utf8(stdout) {
            Ok(stdout) => stdout,
            Err(error) => panic!("stdout should be UTF-8: {error:?}"),
        };

        assert!(stdout.contains("\"code\": 1002"));
        assert!(stdout.contains("\"kind\": \"source_invalid_utf8\""));
        assert!(!stdout.contains("source input contains invalid UTF-8 at byte offset"));
    }

    #[test]
    fn run_writes_text_source_inspection_to_stdout() {
        let file = TemporaryFile::write("main.bray", b"module main\r\nfunc main() {}\n");

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let exit_code = run_with_writers(
            [
                OsString::from("brayc"),
                OsString::from("inspect"),
                OsString::from("source"),
                file.path().as_os_str().to_os_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(exit_code, ExitCode::SUCCESS);
        assert!(stderr.is_empty());

        let stdout = match String::from_utf8(stdout) {
            Ok(stdout) => stdout,
            Err(error) => panic!("stdout should be UTF-8: {error:?}"),
        };

        assert!(stdout.contains("kind: source_inspection"));
        assert!(stdout.contains("source_count: 1"));
        assert!(stdout.contains("source_id: 0"));
        assert!(stdout.contains("  origin_kind: file"));
        assert!(stdout.contains("  line_starts: [0, 13, 28]"));
        assert!(stdout.contains("module main\r\nfunc main() {}\n"));
    }

    #[test]
    fn run_writes_json_source_inspection_to_stdout() {
        let file = TemporaryFile::write("main.bray", b"module main\n");

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let exit_code = run_with_writers(
            [
                OsString::from("brayc"),
                OsString::from("--format"),
                OsString::from("json"),
                OsString::from("inspect"),
                OsString::from("source"),
                file.path().as_os_str().to_os_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(exit_code, ExitCode::SUCCESS);
        assert!(stderr.is_empty());

        let stdout = match String::from_utf8(stdout) {
            Ok(stdout) => stdout,
            Err(error) => panic!("stdout should be UTF-8: {error:?}"),
        };

        let output_json: serde_json::Value = match serde_json::from_str(&stdout) {
            Ok(value) => value,
            Err(error) => panic!("stdout should be source-inspection JSON: {error:?}"),
        };

        assert_eq!(output_json["kind"], "source_inspection");
        assert_eq!(output_json["source_count"], 1);
        assert_eq!(output_json["sources"][0]["origin"]["kind"], "file");
        assert_eq!(output_json["sources"][0]["byte_len"], 12);

        assert_eq!(
            output_json["sources"][0]["line_starts"],
            serde_json::json!([0, 12])
        );

        assert_eq!(output_json["sources"][0]["text"], "module main\n");
    }

    #[test]
    fn run_writes_text_token_inspection_to_stdout() {
        let file = TemporaryFile::write("tokens.bray", b"  func // tail\nmain");

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let exit_code = run_with_writers(
            [
                OsString::from("brayc"),
                OsString::from("inspect"),
                OsString::from("tokens"),
                file.path().as_os_str().to_os_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(exit_code, ExitCode::SUCCESS);
        assert!(stderr.is_empty());

        let stdout = match String::from_utf8(stdout) {
            Ok(stdout) => stdout,
            Err(error) => panic!("stdout should be UTF-8: {error:?}"),
        };

        assert!(stdout.contains("kind: token_inspection"));
        assert!(stdout.contains("source_count: 1"));
        assert!(stdout.contains("source_unit: file"));
        assert!(stdout.contains("func_keyword"));
        assert!(stdout.contains("\"func\""));
        assert!(stdout.contains("leading=whitespace_trivia"));
        assert!(stdout.contains("line_comment_trivia"));
        assert!(stdout.contains("diagnostics:\n    none"));
    }

    #[test]
    fn run_writes_json_token_inspection_to_stdout() {
        let first = TemporaryFile::write("first.bray", b"func\n");
        let second = TemporaryFile::write("second.bray", b"$");

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let exit_code = run_with_writers(
            [
                OsString::from("brayc"),
                OsString::from("--format"),
                OsString::from("json"),
                OsString::from("inspect"),
                OsString::from("tokens"),
                first.path().as_os_str().to_os_string(),
                second.path().as_os_str().to_os_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(exit_code, ExitCode::FAILURE);
        assert!(stderr.is_empty());

        let stdout = match String::from_utf8(stdout) {
            Ok(stdout) => stdout,
            Err(error) => panic!("stdout should be UTF-8: {error:?}"),
        };

        let output_json: serde_json::Value = match serde_json::from_str(&stdout) {
            Ok(value) => value,
            Err(error) => panic!("stdout should be token-inspection JSON: {error:?}"),
        };

        assert_eq!(output_json["kind"], "token_inspection");
        assert_eq!(output_json["source_count"], 2);
        assert_eq!(output_json["has_errors"], true);

        assert_eq!(
            output_json["sources"][0]["tokens"][0]["kind"],
            "func_keyword"
        );

        assert_eq!(
            output_json["sources"][1]["tokens"][0]["kind"],
            "invalid_token"
        );

        assert_eq!(
            output_json["sources"][1]["diagnostics"][0]["kind"],
            "lexical_invalid_character"
        );

        assert_eq!(output_json["sources"][1]["diagnostics"][0]["code"], 2001);
    }

    #[test]
    fn run_writes_text_syntax_inspection_to_stdout() {
        let file = TemporaryFile::write("syntax.bray", b"module main\n");

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let exit_code = run_with_writers(
            [
                OsString::from("brayc"),
                OsString::from("inspect"),
                OsString::from("syntax"),
                file.path().as_os_str().to_os_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(exit_code, ExitCode::FAILURE);
        assert!(stderr.is_empty());

        let stdout = match String::from_utf8(stdout) {
            Ok(stdout) => stdout,
            Err(error) => panic!("stdout should be UTF-8: {error:?}"),
        };

        assert!(stdout.contains("kind: syntax_inspection"));
        assert!(stdout.contains("source_count: 1"));
        assert!(stdout.contains("source_unit: file"));
        assert!(stdout.contains("└─ source_unit"));
        assert!(stdout.contains("├─ module_keyword"));
        assert!(stdout.contains("semicolon_token"));
        assert!(stdout.contains("[missing]"));
    }

    #[test]
    fn run_writes_json_syntax_inspection_to_stdout() {
        let file = TemporaryFile::write("syntax.bray", b"module main;\n");

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let exit_code = run_with_writers(
            [
                OsString::from("brayc"),
                OsString::from("--format"),
                OsString::from("json"),
                OsString::from("inspect"),
                OsString::from("syntax"),
                file.path().as_os_str().to_os_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(exit_code, ExitCode::SUCCESS);
        assert!(stderr.is_empty());

        let stdout = match String::from_utf8(stdout) {
            Ok(stdout) => stdout,
            Err(error) => panic!("stdout should be UTF-8: {error:?}"),
        };

        let output_json: serde_json::Value = match serde_json::from_str(&stdout) {
            Ok(value) => value,
            Err(error) => panic!("stdout should be syntax-inspection JSON: {error:?}"),
        };

        assert_eq!(output_json["kind"], "syntax_inspection");
        assert_eq!(output_json["source_count"], 1);
        assert_eq!(output_json["has_errors"], false);
        assert_eq!(output_json["sources"][0]["tree"]["kind"], "source_unit");
    }

    #[test]
    fn run_writes_text_declaration_inspection_to_stdout() {
        let file = TemporaryFile::write(
            "declarations.bray",
            b"module app\n{\n    struct point\n    {\n        x: i32;\n    }\n}\n",
        );

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let exit_code = run_with_writers(
            [
                OsString::from("brayc"),
                OsString::from("inspect"),
                OsString::from("declarations"),
                file.path().as_os_str().to_os_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(exit_code, ExitCode::SUCCESS);
        assert!(stderr.is_empty());

        let stdout = match String::from_utf8(stdout) {
            Ok(stdout) => stdout,
            Err(error) => panic!("stdout should be UTF-8: {error:?}"),
        };

        assert!(stdout.contains("kind: declaration_inspection"));
        assert!(stdout.contains("module app [container:"));
        assert!(stdout.contains("module_contribution"));
        assert!(stdout.contains("struct point"));
        assert!(stdout.contains("struct_field x"));
    }

    #[test]
    fn run_writes_json_declaration_inspection_to_stdout() {
        let file = TemporaryFile::write("declarations.bray", b"module app;\n");

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let exit_code = run_with_writers(
            [
                OsString::from("brayc"),
                OsString::from("--format"),
                OsString::from("json"),
                OsString::from("inspect"),
                OsString::from("declarations"),
                file.path().as_os_str().to_os_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(exit_code, ExitCode::SUCCESS);
        assert!(stderr.is_empty());

        let stdout = match String::from_utf8(stdout) {
            Ok(stdout) => stdout,
            Err(error) => panic!("stdout should be UTF-8: {error:?}"),
        };

        let output_json: serde_json::Value = match serde_json::from_str(&stdout) {
            Ok(value) => value,
            Err(error) => panic!("stdout should be declaration-inspection JSON: {error:?}"),
        };

        assert_eq!(output_json["kind"], "declaration_inspection");
        assert_eq!(output_json["module_part_count"], 1);
        assert_eq!(output_json["root"]["modules"][0]["module_path"], "app");
    }

    #[test]
    fn run_writes_text_symbol_inspection_to_stdout() {
        let file = TemporaryFile::write(
            "symbols.bray",
            b"module app\n{\n    struct point\n    {\n        x: i32;\n    }\n}\n",
        );

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let exit_code = run_with_writers(
            [
                OsString::from("brayc"),
                OsString::from("inspect"),
                OsString::from("symbols"),
                file.path().as_os_str().to_os_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(exit_code, ExitCode::SUCCESS);
        assert!(stderr.is_empty());

        let stdout = match String::from_utf8(stdout) {
            Ok(stdout) => stdout,
            Err(error) => panic!("stdout should be UTF-8: {error:?}"),
        };

        assert!(stdout.contains("kind: symbol_inspection"));
        assert!(stdout.contains("package command.line"));
        assert!(stdout.contains("module app"));
        assert!(stdout.contains("struct point"));
        assert!(stdout.contains("fields"));
        assert!(stdout.contains("struct_field x"));
    }

    #[test]
    fn run_writes_json_symbol_inspection_to_stdout() {
        let file = TemporaryFile::write("symbols.bray", b"module app;\n");

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let exit_code = run_with_writers(
            [
                OsString::from("brayc"),
                OsString::from("--format"),
                OsString::from("json"),
                OsString::from("inspect"),
                OsString::from("symbols"),
                file.path().as_os_str().to_os_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(exit_code, ExitCode::SUCCESS);
        assert!(stderr.is_empty());

        let stdout = match String::from_utf8(stdout) {
            Ok(stdout) => stdout,
            Err(error) => panic!("stdout should be UTF-8: {error:?}"),
        };

        let output_json: serde_json::Value = match serde_json::from_str(&stdout) {
            Ok(value) => value,
            Err(error) => panic!("stdout should be symbol-inspection JSON: {error:?}"),
        };

        assert_eq!(output_json["kind"], "symbol_inspection");
        assert_eq!(output_json["has_errors"], false);
        assert!(output_json["symbol_count"].as_u64().is_some());
    }

    #[test]
    fn symbol_inspection_is_deterministic_across_worker_budgets() {
        let file = TemporaryFile::write(
            "symbols.bray",
            b"module app\n{\n    const first: i32 = 1;\n    const second: i32 = 2;\n}\n",
        );

        let serial = run_result([
            OsString::from("brayc"),
            OsString::from("--cpu-count"),
            OsString::from("1"),
            OsString::from("inspect"),
            OsString::from("symbols"),
            file.path().as_os_str().to_os_string(),
        ]);

        let parallel = run_result([
            OsString::from("brayc"),
            OsString::from("--cpu-count"),
            OsString::from("4"),
            OsString::from("inspect"),
            OsString::from("symbols"),
            file.path().as_os_str().to_os_string(),
        ]);

        assert_eq!(serial.exit_code(), ExitCode::SUCCESS);
        assert_eq!(parallel.exit_code(), ExitCode::SUCCESS);
        assert_eq!(serial.stdout(), parallel.stdout());
    }

    #[test]
    fn run_writes_source_wide_json_bound_inspection_to_stdout() {
        let source = concat!(
            "module app;\n",
            "\n",
            "func main(value: i32) -> i32\n",
            "{\n",
            "    let result: i32 = value;\n",
            "\n",
            "    result;\n",
            "}\n",
        );

        let file = TemporaryFile::write("bound.bray", source.as_bytes());

        let result = run_result([
            OsString::from("brayc"),
            OsString::from("--format"),
            OsString::from("json"),
            OsString::from("inspect"),
            OsString::from("bound"),
            file.path().as_os_str().to_os_string(),
        ]);

        assert_eq!(result.exit_code(), ExitCode::SUCCESS);

        let output_json: serde_json::Value = match serde_json::from_str(result.stdout()) {
            Ok(value) => value,
            Err(error) => panic!("stdout should be bound-inspection JSON: {error:?}"),
        };

        assert_eq!(output_json["kind"], "bound_inspection");

        assert_eq!(output_json["units"][0]["unit_kind"], "callable_body");

        assert_eq!(
            output_json["units"][0]["root"]["node_kind"],
            "callable_body"
        );
    }

    #[test]
    fn bound_inspection_is_deterministic_across_worker_budgets() {
        let source = concat!(
            "module app;\n",
            "\n",
            "func main(value: i32) -> i32\n",
            "{\n",
            "    value;\n",
            "}\n",
        );

        let file = TemporaryFile::write("bound.bray", source.as_bytes());

        let offset = source
            .find("value;")
            .unwrap_or_else(|| panic!("test source must contain selected expression"))
            .to_string();

        let inspect = |cpu_count: &str| {
            run_result([
                OsString::from("brayc"),
                OsString::from("--cpu-count"),
                OsString::from(cpu_count),
                OsString::from("inspect"),
                OsString::from("bound"),
                OsString::from("--offset"),
                OsString::from(&offset),
                file.path().as_os_str().to_os_string(),
            ])
        };

        let serial = inspect("1");
        let parallel = inspect("4");

        assert_eq!(serial.exit_code(), ExitCode::SUCCESS);
        assert_eq!(parallel.exit_code(), ExitCode::SUCCESS);
        assert_eq!(serial.stdout(), parallel.stdout());
    }

    #[test]
    fn run_writes_lowered_and_mir_inspections() {
        let source = concat!(
            "module app;\n",
            "\n",
            "func main() -> i32\n",
            "{\n",
            "    return 1;\n",
            "}\n",
        );

        let file = TemporaryFile::write("main.bray", source.as_bytes());

        let lowered = run_result([
            OsString::from("brayc"),
            OsString::from("--format"),
            OsString::from("json"),
            OsString::from("inspect"),
            OsString::from("lowered"),
            file.path().as_os_str().to_os_string(),
        ]);

        let mir = run_result([
            OsString::from("brayc"),
            OsString::from("inspect"),
            OsString::from("mir"),
            file.path().as_os_str().to_os_string(),
        ]);

        assert_eq!(lowered.exit_code(), ExitCode::SUCCESS);

        assert!(
            lowered
                .stdout()
                .contains("\"kind\": \"lowered_inspection\"")
        );

        assert!(lowered.stdout().contains("\"blocks\""));

        assert_eq!(mir.exit_code(), ExitCode::SUCCESS);
        assert!(mir.stdout().contains("mir unit"));
        assert!(mir.stdout().contains("bb0("));
    }

    #[test]
    fn lowered_and_mir_inspections_are_deterministic_across_worker_budgets() {
        let source = concat!(
            "module app;\n",
            "\n",
            "func main() -> i32\n",
            "{\n",
            "    return 1;\n",
            "}\n",
        );

        let file = TemporaryFile::write("main.bray", source.as_bytes());

        for command in ["lowered", "mir"] {
            let inspect = |cpu_count: &str| {
                run_result([
                    OsString::from("brayc"),
                    OsString::from("--cpu-count"),
                    OsString::from(cpu_count),
                    OsString::from("inspect"),
                    OsString::from(command),
                    file.path().as_os_str().to_os_string(),
                ])
            };

            let serial = inspect("1");
            let parallel = inspect("4");

            assert_eq!(serial.exit_code(), ExitCode::SUCCESS, "{command}");
            assert_eq!(parallel.exit_code(), ExitCode::SUCCESS, "{command}");
            assert_eq!(serial.stdout(), parallel.stdout(), "{command}");
        }
    }

    #[test]
    fn inspection_report_files_exactly_mirror_stdout_for_text_and_json() {
        let file = TemporaryFile::write("main.bray", b"module app;\n");
        let text_report = TemporaryFile::write("report.txt", b"stale");
        let json_report = TemporaryFile::write("report.json", b"stale");

        for (format, report) in [("text", &text_report), ("json", &json_report)] {
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();

            let exit_code = run_with_writers(
                [
                    OsString::from("brayc"),
                    OsString::from("--format"),
                    OsString::from(format),
                    OsString::from("inspect"),
                    OsString::from("source"),
                    OsString::from("--output-file"),
                    report.path().as_os_str().to_os_string(),
                    file.path().as_os_str().to_os_string(),
                ],
                &mut stdout,
                &mut stderr,
            );

            let report_bytes = std::fs::read(report.path())
                .unwrap_or_else(|error| panic!("inspection report should be readable: {error:?}"));

            assert_eq!(exit_code, ExitCode::SUCCESS);
            assert_eq!(report_bytes, stdout);
            assert!(stderr.is_empty());
        }
    }

    #[test]
    fn inspection_report_file_failures_fail_without_publishing_stdout() {
        let file = TemporaryFile::write("main.bray", b"module app;\n");

        let report = unique_temporary_directory()
            .join("missing")
            .join("report.txt");

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let exit_code = run_with_writers(
            [
                OsString::from("brayc"),
                OsString::from("inspect"),
                OsString::from("source"),
                OsString::from("--output-file"),
                report.as_os_str().to_os_string(),
                file.path().as_os_str().to_os_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(exit_code, ExitCode::FAILURE);
        assert!(stdout.is_empty());

        assert!(String::from_utf8_lossy(&stderr).contains("could not write inspection report"));

        assert!(!report.exists());
    }

    #[test]
    fn inspection_report_file_failures_publish_structured_json_diagnostics() {
        let file = TemporaryFile::write("main.bray", b"module app;\n");

        let report = unique_temporary_directory()
            .join("missing")
            .join("report.json");

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let exit_code = run_with_writers(
            [
                OsString::from("brayc"),
                OsString::from("--format"),
                OsString::from("json"),
                OsString::from("inspect"),
                OsString::from("source"),
                OsString::from("--output-file"),
                report.as_os_str().to_os_string(),
                file.path().as_os_str().to_os_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(exit_code, ExitCode::FAILURE);
        assert!(stderr.is_empty());

        let stdout = String::from_utf8(stdout)
            .unwrap_or_else(|error| panic!("JSON diagnostics should be UTF-8: {error:?}"));

        assert!(stdout.contains(DiagnosticKind::InspectionReportWriteFailed.as_str()));

        assert!(!report.exists());
    }
}
