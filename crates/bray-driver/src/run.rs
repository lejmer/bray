use std::ffi::OsString;
use std::io;
use std::process::ExitCode;

use bray_compilation::{Compilation, CompilationRequest};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::PackageIdentity;

use crate::command::{DriverCommandKind, DriverInvocation, DriverOutputFormat};
use crate::declaration_inspection::render_declaration_inspection;
use crate::diagnostic_output::write_driver_output;
use crate::exit_status::exit_code_from_diagnostics;
use crate::file_arguments::compilation_request_from_file_arguments;
use crate::inspection::InspectionOutput;
use crate::source_inspection::render_source_inspection;
use crate::syntax_inspection::render_syntax_inspection;
use crate::token_inspection::render_token_inspection;

/// Structured result from running the Bray compiler driver.
#[derive(Debug)]
pub struct DriverRunResult {
    exit_code: ExitCode,
    diagnostics: DiagnosticBag,
    compilation: Option<Compilation>,
    output_format: DriverOutputFormat,
    stdout: String,
    stderr: String,
}

impl DriverRunResult {
    fn new(
        exit_code: ExitCode,
        diagnostics: DiagnosticBag,
        output_format: DriverOutputFormat,
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
        output_format: DriverOutputFormat,
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
        }
    }

    fn with_compilation(
        exit_code: ExitCode,
        diagnostics: DiagnosticBag,
        output_format: DriverOutputFormat,
        compilation: Compilation,
    ) -> Self {
        Self {
            exit_code,
            diagnostics,
            compilation: Some(compilation),
            output_format,
            stdout: String::new(),
            stderr: String::new(),
        }
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
    pub const fn output_format(&self) -> DriverOutputFormat {
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

    /// Returns whether this result carries driver-owned terminal output.
    pub fn has_terminal_output(&self) -> bool {
        !self.stdout.is_empty() || !self.stderr.is_empty()
    }
}

/// Runs the Bray compiler driver for the provided process arguments.
pub fn run(arguments: impl IntoIterator<Item = OsString>) -> ExitCode {
    let mut stdout = io::stdout().lock();
    let mut stderr = io::stderr().lock();

    run_with_writers(arguments, &mut stdout, &mut stderr)
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

    let (options, command) = invocation.into_parts();

    let output_format = options.output_format();
    let command_kind = command.kind();

    let request = match compilation_request_from_file_arguments(
        command_line_package_identity(),
        command.into_files(),
        options.compilation_options(),
    ) {
        Ok(request) => request,
        Err(diagnostics) => {
            let exit_code = exit_code_from_diagnostics(&diagnostics);

            return DriverRunResult::new(exit_code, diagnostics, output_format);
        }
    };

    if command_kind == DriverCommandKind::Check {
        return run_check_command(request, output_format);
    }

    if command_kind == DriverCommandKind::InspectSource {
        return run_inspect_source_command(request, output_format);
    }

    if command_kind == DriverCommandKind::InspectTokens {
        return run_fact_inspection_command(request, output_format, render_token_inspection);
    }

    if command_kind == DriverCommandKind::InspectSyntax {
        return run_fact_inspection_command(request, output_format, render_syntax_inspection);
    }

    if command_kind == DriverCommandKind::InspectDeclarations {
        return run_fact_inspection_command(request, output_format, render_declaration_inspection);
    }

    match command_kind {
        DriverCommandKind::Check
        | DriverCommandKind::InspectSource
        | DriverCommandKind::InspectTokens
        | DriverCommandKind::InspectSyntax
        | DriverCommandKind::InspectDeclarations => {
            unreachable!("handled command kind did not return")
        }
    }
}

fn command_line_package_identity() -> PackageIdentity {
    // Loose-file commands compile as one explicitly named command-line package.
    match PackageIdentity::try_new("command.line") {
        Some(identity) => identity,
        None => panic!("the compiler's command-line package identity must be valid"),
    }
}

fn run_check_command(
    request: CompilationRequest,
    output_format: DriverOutputFormat,
) -> DriverRunResult {
    let compilation = match Compilation::load(request) {
        Ok(compilation) => compilation,
        Err(_) => return compilation_load_failure_result(output_format),
    };

    let diagnostics = compilation.check_diagnostics().clone();

    diagnostic_result_from_compilation(compilation, diagnostics, output_format)
}

fn run_inspect_source_command(
    request: CompilationRequest,
    output_format: DriverOutputFormat,
) -> DriverRunResult {
    let compilation = match Compilation::load(request) {
        Ok(compilation) => compilation,
        Err(_) => return compilation_load_failure_result(output_format),
    };

    if !compilation.source_diagnostics().is_empty() {
        let diagnostics = compilation.source_diagnostics().clone();

        return diagnostic_result_from_compilation(compilation, diagnostics, output_format);
    }

    let stdout = match render_source_inspection(&compilation, output_format) {
        Ok(stdout) => stdout,
        Err(_) => return compilation_load_failure_result(output_format),
    };

    DriverRunResult::with_output(
        ExitCode::SUCCESS,
        DiagnosticBag::new(),
        output_format,
        stdout,
        String::new(),
    )
}

fn run_fact_inspection_command<E>(
    request: CompilationRequest,
    output_format: DriverOutputFormat,
    render: impl FnOnce(&Compilation, DriverOutputFormat) -> Result<InspectionOutput, E>,
) -> DriverRunResult {
    let compilation = match Compilation::load(request) {
        Ok(compilation) => compilation,
        Err(_) => return compilation_load_failure_result(output_format),
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

    DriverRunResult::with_output(exit_code, diagnostics, output_format, stdout, String::new())
}

fn diagnostic_result_from_compilation(
    compilation: Compilation,
    diagnostics: DiagnosticBag,
    output_format: DriverOutputFormat,
) -> DriverRunResult {
    let exit_code = exit_code_from_diagnostics(&diagnostics);

    DriverRunResult::with_compilation(exit_code, diagnostics, output_format, compilation)
}

fn compilation_load_failure_result(output_format: DriverOutputFormat) -> DriverRunResult {
    DriverRunResult::new(ExitCode::FAILURE, DiagnosticBag::new(), output_format)
}

fn run_with_writers(
    arguments: impl IntoIterator<Item = OsString>,
    stdout: &mut impl io::Write,
    stderr: &mut impl io::Write,
) -> ExitCode {
    let result = run_result(arguments);

    match write_driver_output(&result, stdout, stderr) {
        Ok(()) => result.exit_code(),
        Err(_) => ExitCode::FAILURE,
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
        assert_eq!(result.output_format(), crate::DriverOutputFormat::Json);

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
}
