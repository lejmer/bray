use bray_diagnostics::DiagnosticLinkerDriverIdentity;
use serde::Serialize;

use super::execution::{
    DiagnosticProfileComparisonProblemJson, DiagnosticProfileValidationProblemJson,
    DiagnosticProjectProcessFailureJson, DiagnosticTestExecutionPlanProblemJson,
    test_execution_plan_failure_json,
};
use super::inspection::{DiagnosticInspectionFailureJson, native_linker_build_failure_key};
use crate::output::path_to_output_string;

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(in crate::output::diagnostic::json) enum DiagnosticProjectDependencyCycleMemberJson {
    Package { identity: String },
    Product { package: String, product: String },
}

impl DiagnosticProjectDependencyCycleMemberJson {
    pub(in crate::output::diagnostic::json) fn from_member(
        member: &bray_diagnostics::DiagnosticProjectDependencyCycleMember,
    ) -> Self {
        use bray_diagnostics::DiagnosticProjectDependencyCycleMember as Member;

        match member {
            Member::Package { identity } => Self::Package {
                identity: identity.clone(),
            },
            Member::Product { package, product } => Self::Product {
                package: package.clone(),
                product: product.clone(),
            },
        }
    }
}

#[derive(Serialize)]
pub(in crate::output::diagnostic::json) struct DiagnosticLinkerDriverIdentityJson {
    kind: &'static str,
    name: String,
    capability_revision: String,
    toolchain_revision: String,
}

#[derive(Serialize)]
pub(in crate::output::diagnostic::json) struct DiagnosticExternalToolExitJson {
    exit_code: Option<i32>,
    standard_output: DiagnosticExternalToolStreamCaptureJson,
    standard_error: DiagnosticExternalToolStreamCaptureJson,
}

impl DiagnosticExternalToolExitJson {
    pub(in crate::output::diagnostic::json) fn from_exit(exit: &bray_diagnostics::DiagnosticExternalToolExit) -> Self {
        Self {
            exit_code: exit.code(),
            standard_output: DiagnosticExternalToolStreamCaptureJson::from_capture(
                exit.standard_output(),
            ),
            standard_error: DiagnosticExternalToolStreamCaptureJson::from_capture(
                exit.standard_error(),
            ),
        }
    }
}

#[derive(Serialize)]
struct DiagnosticExternalToolStreamCaptureJson {
    text: String,
    original_byte_count: u64,
    captured_byte_count: u64,
    omitted_byte_count: u64,
    lossy_utf8: bool,
}

impl DiagnosticExternalToolStreamCaptureJson {
    fn from_capture(capture: &bray_diagnostics::DiagnosticExternalToolStreamCapture) -> Self {
        Self {
            text: capture.text().to_owned(),
            original_byte_count: capture.original_byte_count(),
            captured_byte_count: capture.captured_byte_count(),
            omitted_byte_count: capture.omitted_byte_count(),
            lossy_utf8: capture.is_lossy_utf8(),
        }
    }
}

impl DiagnosticLinkerDriverIdentityJson {
    pub(in crate::output::diagnostic::json) fn from_identity(identity: &DiagnosticLinkerDriverIdentity) -> Self {
        Self {
            kind: identity.kind().as_str(),
            name: identity.name().to_owned(),
            capability_revision: identity.capability_revision().to_owned(),
            toolchain_revision: identity.toolchain_revision().to_owned(),
        }
    }
}

#[derive(Serialize)]
pub(in crate::output::diagnostic::json) struct DiagnosticSourceInputJson {
    index: u64,
    kind: &'static str,
    origin: DiagnosticSourceInputOriginJson,
}

impl DiagnosticSourceInputJson {
    pub(in crate::output::diagnostic::json) fn from_input(input: &bray_diagnostics::DiagnosticSourceInput) -> Self {
        Self {
            index: input.index(),
            kind: input.kind().as_str(),
            origin: DiagnosticSourceInputOriginJson::from_origin(input.origin()),
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
enum DiagnosticSourceInputOriginJson {
    File(String),
    Name(String),
    Uri(String),
    Missing,
}

impl DiagnosticSourceInputOriginJson {
    fn from_origin(origin: &bray_diagnostics::DiagnosticSourceInputOrigin) -> Self {
        match origin {
            bray_diagnostics::DiagnosticSourceInputOrigin::File(path) => {
                Self::File(path_to_output_string(path))
            }
            bray_diagnostics::DiagnosticSourceInputOrigin::Name(name) => Self::Name(name.clone()),
            bray_diagnostics::DiagnosticSourceInputOrigin::Uri(uri) => Self::Uri(uri.clone()),
            bray_diagnostics::DiagnosticSourceInputOrigin::Missing => Self::Missing,
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub(in crate::output::diagnostic::json) enum DiagnosticUnsupportedEmissionReasonJson {
    ToolUnavailable {
        tool: &'static str,
        configured: Option<String>,
    },
    ToolInspectionFailed {
        tool: &'static str,
        path: String,
        error: &'static str,
    },
    InvalidToolFile {
        tool: &'static str,
        path: String,
    },
    MissingHostEnvironment {
        variable: &'static str,
    },
}

impl DiagnosticUnsupportedEmissionReasonJson {
    pub(in crate::output::diagnostic::json) fn from_reason(
        reason: &bray_diagnostics::DiagnosticUnsupportedEmissionReason,
    ) -> Self {
        use bray_diagnostics::DiagnosticUnsupportedEmissionReason as Reason;

        match reason {
            Reason::ToolUnavailable { tool, configured } => Self::ToolUnavailable {
                tool: tool.as_str(),
                configured: configured.as_deref().map(path_to_output_string),
            },
            Reason::ToolInspectionFailed { tool, path, error } => Self::ToolInspectionFailed {
                tool: tool.as_str(),
                path: path_to_output_string(path),
                error: error.as_str(),
            },
            Reason::InvalidToolFile { tool, path } => Self::InvalidToolFile {
                tool: tool.as_str(),
                path: path_to_output_string(path),
            },
            Reason::MissingHostEnvironment(variable) => Self::MissingHostEnvironment {
                variable: variable.as_str(),
            },
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "category", rename_all = "snake_case")]
pub(in crate::output::diagnostic::json) enum DiagnosticProjectCommandFailureJson {
    HostIo {
        operation: &'static str,
        error: &'static str,
    },
    Io {
        operation: &'static str,
        path: String,
        error: &'static str,
    },
    CurrentExecutable {
        operation: &'static str,
        error: &'static str,
    },
    MissingParent {
        operation: &'static str,
        path: String,
    },
    PathContract {
        operation: &'static str,
        path: String,
        requirement: &'static str,
    },
    Process {
        operation: &'static str,
        program: String,
        native_operation: &'static str,
        failure: DiagnosticProjectProcessFailureJson,
    },
    ProcessExit {
        operation: &'static str,
        program: String,
        code: Option<i32>,
    },
    ToolStreamIo {
        operation: &'static str,
        stream: &'static str,
        error: &'static str,
    },
    ToolStreamInvalidUtf8 {
        operation: &'static str,
        stream: &'static str,
    },
    ToolProtocol {
        operation: &'static str,
        failure: &'static str,
        stream: &'static str,
    },
    Document {
        operation: &'static str,
        path: Option<String>,
        problem: &'static str,
    },
    TestExecutionPlan {
        problem: DiagnosticTestExecutionPlanProblemJson,
    },
    TestScheduling {
        reason: &'static str,
        test: String,
    },
    ProfileComparison {
        problem: DiagnosticProfileComparisonProblemJson,
    },
    ProfileValidation {
        path: String,
        problem: DiagnosticProfileValidationProblemJson,
    },
    MissingTestHostLocation {
        test: String,
    },
    MissingResult {
        operation: &'static str,
    },
    CapacityExceeded {
        operation: &'static str,
    },
    CompilationDiagnosticCapacityExceeded {
        count: u64,
    },
    CompilationDiagnosticCountUnrepresentable,
    NativeLinkerConstruction {
        target: String,
        failure: &'static str,
    },
    Inspection {
        failure: DiagnosticInspectionFailureJson,
    },
    Invariant {
        operation: &'static str,
    },
}

impl DiagnosticProjectCommandFailureJson {
    pub(in crate::output::diagnostic::json) fn from_failure(
        failure: &bray_diagnostics::DiagnosticProjectCommandFailure,
    ) -> Self {
        use bray_diagnostics::DiagnosticProjectCommandFailure as Failure;

        match failure {
            Failure::HostIo { operation, error } => Self::HostIo {
                operation: operation.as_str(),
                error: error.as_str(),
            },
            Failure::Io {
                operation,
                path,
                error,
            } => Self::Io {
                operation: operation.as_str(),
                path: path_to_output_string(path),
                error: error.as_str(),
            },
            Failure::CurrentExecutable { operation, error } => Self::CurrentExecutable {
                operation: operation.as_str(),
                error: error.as_str(),
            },
            Failure::MissingParent { operation, path } => Self::MissingParent {
                operation: operation.as_str(),
                path: path_to_output_string(path),
            },
            Failure::PathContract {
                operation,
                path,
                requirement,
            } => Self::PathContract {
                operation: operation.as_str(),
                path: path_to_output_string(path),
                requirement: requirement.as_str(),
            },
            Failure::Process {
                operation,
                program,
                native_operation,
                failure,
            } => Self::Process {
                operation: operation.as_str(),
                program: path_to_output_string(program),
                native_operation: native_operation.as_str(),
                failure: DiagnosticProjectProcessFailureJson::from_failure(*failure),
            },
            Failure::ProcessExit {
                operation,
                program,
                code,
            } => Self::ProcessExit {
                operation: operation.as_str(),
                program: path_to_output_string(program),
                code: *code,
            },
            Failure::ToolStreamIo {
                operation,
                stream,
                error,
            } => Self::ToolStreamIo {
                operation: operation.as_str(),
                stream: diagnostic_tool_stream_key(*stream),
                error: error.as_str(),
            },
            Failure::ToolStreamInvalidUtf8 { operation, stream } => Self::ToolStreamInvalidUtf8 {
                operation: operation.as_str(),
                stream: diagnostic_tool_stream_key(*stream),
            },
            Failure::ToolProtocol { operation, failure } => {
                let (failure, stream) = match failure {
                    bray_diagnostics::DiagnosticToolProtocolFailure::MissingStream(stream) => {
                        ("missing_stream", diagnostic_tool_stream_key(*stream))
                    }
                    bray_diagnostics::DiagnosticToolProtocolFailure::StreamThreadPanicked(
                        stream,
                    ) => (
                        "stream_thread_panicked",
                        diagnostic_tool_stream_key(*stream),
                    ),
                };

                Self::ToolProtocol {
                    operation: operation.as_str(),
                    failure,
                    stream,
                }
            }
            Failure::Document {
                operation,
                path,
                problem,
            } => Self::Document {
                operation: operation.as_str(),
                path: path.as_ref().map(|path| path_to_output_string(path)),
                problem: problem.as_str(),
            },
            Failure::TestExecutionPlan(problem) => test_execution_plan_failure_json(problem),
            Failure::TestScheduling(problem) => {
                let (reason, test) = match problem {
                    bray_diagnostics::DiagnosticTestSchedulingProblem::InvocationNotActive(
                        test,
                    ) => ("invocation_not_active", test.clone()),
                    bray_diagnostics::DiagnosticTestSchedulingProblem::ResultIdentityMismatch(
                        test,
                    ) => ("result_identity_mismatch", test.clone()),
                };

                Self::TestScheduling { reason, test }
            }
            Failure::ProfileComparison(problem) => Self::ProfileComparison {
                problem: DiagnosticProfileComparisonProblemJson::from_problem(problem),
            },
            Failure::ProfileValidation { path, problem } => Self::ProfileValidation {
                path: path_to_output_string(path),
                problem: DiagnosticProfileValidationProblemJson::from_problem(problem),
            },
            Failure::MissingTestHostLocation(test) => {
                Self::MissingTestHostLocation { test: test.clone() }
            }
            Failure::MissingResult(operation) => Self::MissingResult {
                operation: operation.as_str(),
            },
            Failure::CapacityExceeded(operation) => Self::CapacityExceeded {
                operation: operation.as_str(),
            },
            Failure::CompilationDiagnosticCapacityExceeded { count } => {
                Self::CompilationDiagnosticCapacityExceeded { count: *count }
            }
            Failure::CompilationDiagnosticCountUnrepresentable => {
                Self::CompilationDiagnosticCountUnrepresentable
            }
            Failure::NativeLinkerConstruction { target, failure } => {
                Self::NativeLinkerConstruction {
                    target: target.clone(),
                    failure: native_linker_build_failure_key(*failure),
                }
            }
            Failure::Inspection(failure) => Self::Inspection {
                failure: DiagnosticInspectionFailureJson::from_failure(*failure),
            },
            Failure::Invariant(operation) => Self::Invariant {
                operation: operation.as_str(),
            },
        }
    }
}

const fn diagnostic_tool_stream_key(
    stream: bray_diagnostics::DiagnosticToolStream,
) -> &'static str {
    use bray_diagnostics::DiagnosticToolStream as Stream;

    match stream {
        Stream::StandardInput => "standard_input",
        Stream::StandardOutput => "standard_output",
        Stream::StandardError => "standard_error",
    }
}
