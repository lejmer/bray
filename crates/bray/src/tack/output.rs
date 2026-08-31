use std::process::ExitCode;

use bray_diagnostics::{
    DiagnosticBag, DiagnosticDocumentParseKind, DiagnosticProjectCommandFailure,
    DiagnosticProjectOperation,
};
use bray_tooling::OutputFormat;

use crate::tack::error::operation_diagnostics;
use crate::tack::result::TackRunResult;
use crate::tack::tool::ToolOutput;

pub(super) fn result_from_operation(
    operation: Result<(), DiagnosticBag>,
    output_format: OutputFormat,
) -> TackRunResult {
    match operation {
        Ok(()) => TackRunResult::new(ExitCode::SUCCESS, DiagnosticBag::new(), output_format),
        Err(diagnostics) => failure(diagnostics, output_format),
    }
}

pub(super) fn result_from_outputs(
    outputs: Vec<ToolOutput>,
    output_format: OutputFormat,
) -> TackRunResult {
    if output_format == OutputFormat::Json && outputs.len() > 1 {
        return aggregate_json_outputs(outputs, output_format);
    }

    let success = outputs.iter().all(ToolOutput::success);
    let mut stdout = String::new();
    let mut stderr = String::new();

    for output in outputs {
        let (_, _, _, child_stdout, child_stderr) = output.into_parts();

        stdout.push_str(&child_stdout);
        stderr.push_str(&child_stderr);
    }

    TackRunResult::with_output(
        exit_code(success),
        DiagnosticBag::new(),
        output_format,
        stdout,
        stderr,
    )
}

pub(super) fn result_from_output(output: ToolOutput, output_format: OutputFormat) -> TackRunResult {
    let (success, _, _, stdout, stderr) = output.into_parts();

    TackRunResult::with_output(
        exit_code(success),
        DiagnosticBag::new(),
        output_format,
        stdout,
        stderr,
    )
}

fn aggregate_json_outputs(outputs: Vec<ToolOutput>, output_format: OutputFormat) -> TackRunResult {
    let success = outputs.iter().all(ToolOutput::success);
    let mut diagnostics = Vec::new();
    let mut artifacts = Vec::new();
    let mut stderr = String::new();

    for output in outputs {
        let (child_success, exit_code, subject, stdout, child_stderr) = output.into_parts();

        let product = subject.unwrap_or_else(|| "unknown product".to_owned());

        stderr.push_str(&child_stderr);

        if !child_success && stdout.trim().is_empty() {
            return TackRunResult::with_output(
                ExitCode::FAILURE,
                operation_diagnostics(DiagnosticProjectCommandFailure::CompilerOutputMissing {
                    product,
                    program: std::path::PathBuf::from("brayc"),
                    code: exit_code,
                }),
                output_format,
                String::new(),
                stderr,
            );
        }

        let mut report = match serde_json::from_str::<serde_json::Value>(&stdout) {
            Ok(report) => report,
            Err(error) => {
                append_invalid_compiler_stdout(&mut stderr, &stdout);

                return TackRunResult::with_output(
                    ExitCode::FAILURE,
                    compiler_json_failure(
                        product,
                        exit_code,
                        DiagnosticDocumentParseKind::Syntax,
                        Some(error.to_string()),
                    ),
                    output_format,
                    String::new(),
                    stderr,
                );
            }
        };

        let Some(entries) = report
            .get_mut("diagnostics")
            .and_then(serde_json::Value::as_array_mut)
        else {
            append_invalid_compiler_stdout(&mut stderr, &stdout);

            return TackRunResult::with_output(
                ExitCode::FAILURE,
                compiler_json_failure(
                    product,
                    exit_code,
                    DiagnosticDocumentParseKind::Schema,
                    None,
                ),
                output_format,
                String::new(),
                stderr,
            );
        };

        diagnostics.append(entries);

        if let Some(entries) = report
            .get_mut("artifacts")
            .and_then(serde_json::Value::as_array_mut)
        {
            artifacts.append(entries);
        }
    }

    let stdout = match serde_json::to_string_pretty(&serde_json::json!({
        "has_errors": !success,
        "diagnostics": diagnostics,
        "artifacts": artifacts,
    })) {
        Ok(stdout) => format!("{stdout}\n"),
        Err(error) => {
            return TackRunResult::with_output(
                ExitCode::FAILURE,
                operation_diagnostics(DiagnosticProjectCommandFailure::Document {
                    operation: DiagnosticProjectOperation::CompilerJsonOutput,
                    path: None,
                    problem: DiagnosticDocumentParseKind::Serialization,
                    detail: Some(error.to_string()),
                }),
                output_format,
                String::new(),
                stderr,
            );
        }
    };

    TackRunResult::with_output(
        exit_code(success),
        DiagnosticBag::new(),
        output_format,
        stdout,
        stderr,
    )
}

fn compiler_json_failure(
    product: String,
    code: Option<i32>,
    problem: DiagnosticDocumentParseKind,
    detail: Option<String>,
) -> DiagnosticBag {
    operation_diagnostics(DiagnosticProjectCommandFailure::CompilerOutputInvalid {
        product,
        program: std::path::PathBuf::from("brayc"),
        code,
        problem,
        detail,
    })
}

fn append_invalid_compiler_stdout(stderr: &mut String, stdout: &str) {
    if stdout.is_empty() {
        return;
    }

    if !stderr.is_empty() && !stderr.ends_with('\n') {
        stderr.push('\n');
    }

    stderr.push_str("compiler structured output:\n");
    stderr.push_str(stdout);

    if !stderr.ends_with('\n') {
        stderr.push('\n');
    }
}

pub(super) fn failure(diagnostics: DiagnosticBag, output_format: OutputFormat) -> TackRunResult {
    TackRunResult::new(ExitCode::FAILURE, diagnostics, output_format)
}

const fn exit_code(success: bool) -> ExitCode {
    if success {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{
        DiagnosticArgValue, DiagnosticDocumentParseKind, DiagnosticKind,
        DiagnosticProjectCommandFailure,
    };
    use bray_tooling::OutputFormat;

    use super::result_from_outputs;
    use crate::tack::tool::ToolOutput;

    #[test]
    fn aggregated_build_json_retains_complete_artifact_paths() {
        let output = ToolOutput::new(
            true,
            serde_json::json!({
                "has_errors": false,
                "diagnostics": [],
                "artifacts": ["build/native/debug/hello_world/application.exe"]
            })
            .to_string(),
            String::new(),
        );

        let result = result_from_outputs(vec![output, empty_output()], OutputFormat::Json);

        let report: serde_json::Value = serde_json::from_str(result.stdout())
            .unwrap_or_else(|error| panic!("aggregated build JSON must parse: {error}"));

        assert_eq!(
            report["artifacts"][0],
            "build/native/debug/hello_world/application.exe"
        );
    }

    #[test]
    fn failed_compiler_outputs_retain_product_exit_and_error_context() {
        let output = ToolOutput::from_process(
            false,
            Some(-1_073_741_575),
            String::new(),
            "native compiler failure\n".to_owned(),
        )
        .with_subject("std/api");

        let result = result_from_outputs(vec![output, empty_output()], OutputFormat::Json);

        assert_eq!(result.stderr(), "native compiler failure\n");

        let diagnostic = result
            .diagnostics()
            .by_kind(DiagnosticKind::ProjectCompilerDefect)
            .next()
            .expect("missing compiler output must produce a compiler-defect diagnostic");

        let failure = diagnostic
            .args()
            .iter()
            .find_map(|argument| match argument.value() {
                DiagnosticArgValue::ProjectCommandFailure(failure) => Some(failure),
                _ => None,
            })
            .expect("compiler-defect diagnostic must retain its structured cause");

        assert_eq!(
            failure,
            &DiagnosticProjectCommandFailure::CompilerOutputMissing {
                product: "std/api".to_owned(),
                program: std::path::PathBuf::from("brayc"),
                code: Some(-1_073_741_575),
            }
        );
    }

    #[test]
    fn malformed_compiler_outputs_retain_raw_output_and_structured_context() {
        let output = ToolOutput::from_process(
            false,
            Some(17),
            "{not-json".to_owned(),
            "child stderr\n".to_owned(),
        )
        .with_subject("std/api");

        let result = result_from_outputs(vec![output, empty_output()], OutputFormat::Json);

        assert_eq!(
            result.stderr(),
            "child stderr\ncompiler structured output:\n{not-json\n"
        );

        let diagnostic = result
            .diagnostics()
            .by_kind(DiagnosticKind::ProjectCompilerDefect)
            .next()
            .expect("invalid compiler output must produce a compiler-defect diagnostic");

        let failure = diagnostic
            .args()
            .iter()
            .find_map(|argument| match argument.value() {
                DiagnosticArgValue::ProjectCommandFailure(failure) => Some(failure),
                _ => None,
            })
            .expect("compiler-defect diagnostic must retain its structured cause");

        let DiagnosticProjectCommandFailure::CompilerOutputInvalid {
            product,
            program,
            code,
            problem,
            detail,
        } = failure
        else {
            panic!("unexpected compiler-output failure: {failure:?}");
        };

        assert_eq!(product, "std/api");
        assert_eq!(program, &std::path::PathBuf::from("brayc"));
        assert_eq!(*code, Some(17));
        assert_eq!(*problem, DiagnosticDocumentParseKind::Syntax);
        assert!(detail.as_deref().is_some_and(|detail| !detail.is_empty()));
    }

    fn empty_output() -> ToolOutput {
        ToolOutput::new(
            true,
            serde_json::json!({
                "has_errors": false,
                "diagnostics": []
            })
            .to_string(),
            String::new(),
        )
    }
}
