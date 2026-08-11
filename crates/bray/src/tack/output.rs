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
        let (_, child_stdout, child_stderr) = output.into_parts();

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
    let (success, stdout, stderr) = output.into_parts();

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
    let mut stderr = String::new();

    for output in outputs {
        let (_, stdout, child_stderr) = output.into_parts();

        stderr.push_str(&child_stderr);

        let Ok(mut report) = serde_json::from_str::<serde_json::Value>(&stdout) else {
            return failure(
                compiler_json_failure(DiagnosticDocumentParseKind::Syntax),
                output_format,
            );
        };

        let Some(entries) = report
            .get_mut("diagnostics")
            .and_then(serde_json::Value::as_array_mut)
        else {
            return failure(
                compiler_json_failure(DiagnosticDocumentParseKind::Schema),
                output_format,
            );
        };

        diagnostics.append(entries);
    }

    let stdout = match serde_json::to_string_pretty(&serde_json::json!({
        "has_errors": !success,
        "diagnostics": diagnostics,
    })) {
        Ok(stdout) => format!("{stdout}\n"),
        Err(_) => {
            return failure(
                compiler_json_failure(DiagnosticDocumentParseKind::Serialization),
                output_format,
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

fn compiler_json_failure(problem: DiagnosticDocumentParseKind) -> DiagnosticBag {
    operation_diagnostics(DiagnosticProjectCommandFailure::Document {
        operation: DiagnosticProjectOperation::CompilerJsonOutput,
        path: None,
        problem,
    })
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
