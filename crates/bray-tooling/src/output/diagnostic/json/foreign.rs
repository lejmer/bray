use serde::Serialize;

use super::DiagnosticTypeJson;
use super::interface::{
    DiagnosticProblemFieldJson, DiagnosticProblemFieldValueJson, problem_count_u64, problem_text,
};

#[derive(Serialize)]
pub(in crate::output::diagnostic::json) struct DiagnosticProblemJson {
    pub(in crate::output::diagnostic::json) reason: &'static str,
    pub(in crate::output::diagnostic::json) context: Vec<DiagnosticProblemFieldJson>,
}

pub(in crate::output::diagnostic::json) fn native_link_directive_problem_json(
    problem: &bray_diagnostics::DiagnosticNativeLinkDirectiveProblem,
) -> DiagnosticProblemJson {
    use bray_diagnostics::DiagnosticNativeLinkDirectiveProblem as Problem;

    match problem {
        Problem::Argument(problem) => directive_argument_problem_json(problem),
        Problem::UnsupportedKind { provided } => DiagnosticProblemJson {
            reason: "unsupported_kind",
            context: vec![problem_text("provided", provided.clone())],
        },
        Problem::AmbiguousInput {
            name,
            kind,
            matches,
        } => {
            let mut context = vec![
                problem_text("name", name.clone()),
                problem_count_u64("matches", *matches),
            ];

            if let Some(kind) = kind {
                context.push(problem_text("link_kind", native_link_kind_key(*kind)));
            }

            DiagnosticProblemJson {
                reason: "ambiguous_input",
                context,
            }
        }
    }
}

pub(in crate::output::diagnostic::json) fn native_symbol_directive_problem_json(
    problem: &bray_diagnostics::DiagnosticNativeSymbolDirectiveProblem,
) -> DiagnosticProblemJson {
    match problem {
        bray_diagnostics::DiagnosticNativeSymbolDirectiveProblem::Argument(problem) => {
            directive_argument_problem_json(problem)
        }
    }
}

pub(in crate::output::diagnostic::json) fn platform_service_signature_problem_json(
    problem: &bray_diagnostics::DiagnosticPlatformServiceSignatureProblem,
) -> DiagnosticProblemJson {
    use bray_diagnostics::DiagnosticPlatformServiceSignatureProblem as Problem;

    match problem {
        Problem::CallableAbi { role, actual } => DiagnosticProblemJson {
            reason: "callable_abi",
            context: vec![
                problem_count_u64("role_id", u64::from(role.id())),
                problem_text("actual", actual.as_str()),
            ],
        },
        Problem::Execution { role, actual } => DiagnosticProblemJson {
            reason: "execution",
            context: vec![
                problem_count_u64("role_id", u64::from(role.id())),
                problem_text(
                    "actual",
                    match actual {
                        bray_diagnostics::DiagnosticCallableExecution::Synchronous => "synchronous",
                        bray_diagnostics::DiagnosticCallableExecution::Asynchronous => {
                            "asynchronous"
                        }
                    },
                ),
            ],
        },
        Problem::ParameterCount {
            role,
            expected,
            actual,
        } => DiagnosticProblemJson {
            reason: "parameter_count",
            context: vec![
                problem_count_u64("role_id", u64::from(role.id())),
                problem_count_u64("expected", *expected),
                problem_count_u64("actual", *actual),
            ],
        },
        Problem::ParameterType {
            role,
            ordinal,
            expected,
            actual,
        } => DiagnosticProblemJson {
            reason: "parameter_type",
            context: vec![
                problem_count_u64("role_id", u64::from(role.id())),
                problem_count_u64("ordinal", *ordinal),
                problem_text("expected", platform_abi_type_key(*expected)),
                DiagnosticProblemFieldJson {
                    name: "actual",
                    value: DiagnosticProblemFieldValueJson::Type(DiagnosticTypeJson::from_type(
                        actual,
                    )),
                },
            ],
        },
        Problem::ResultType {
            role,
            expected,
            actual,
        } => DiagnosticProblemJson {
            reason: "result_type",
            context: vec![
                problem_count_u64("role_id", u64::from(role.id())),
                problem_text("expected", platform_abi_type_key(*expected)),
                DiagnosticProblemFieldJson {
                    name: "actual",
                    value: DiagnosticProblemFieldValueJson::Type(DiagnosticTypeJson::from_type(
                        actual,
                    )),
                },
            ],
        },
    }
}

const fn platform_abi_type_key(ty: bray_diagnostics::DiagnosticPlatformAbiType) -> &'static str {
    use bray_diagnostics::DiagnosticPlatformAbiType as Type;

    match ty {
        Type::I32 => "i32",
        Type::U32 => "u32",
        Type::U64 => "u64",
        Type::I64 => "i64",
        Type::PointerU8 => "pointer_u8",
        Type::PointerU32 => "pointer_u32",
        Type::PointerU64 => "pointer_u64",
        Type::PointerI64 => "pointer_i64",
        Type::RawAddressPointer => "raw_address_pointer",
        Type::Path => "path",
        Type::NativeText => "native_text",
        Type::FileOptions => "file_options",
        Type::FileMetadataPointer => "file_metadata_pointer",
        Type::ChildRequest => "child_request",
        Type::ExitStatusPointer => "exit_status_pointer",
        Type::TemporalDateTime => "temporal_date_time",
        Type::TemporalDateTimePointer => "temporal_date_time_pointer",
        Type::TemporalObservationPointer => "temporal_observation_pointer",
        Type::TemporalResolutionPointer => "temporal_resolution_pointer",
        Type::TemporalValue => "temporal_value",
        Type::TemporalValuePointer => "temporal_value_pointer",
        Type::Status => "status",
    }
}

fn directive_argument_problem_json(
    problem: &bray_diagnostics::DiagnosticDirectiveArgumentProblem,
) -> DiagnosticProblemJson {
    use bray_diagnostics::DiagnosticDirectiveArgumentProblem as Problem;

    match problem {
        Problem::Positional { ordinal } => DiagnosticProblemJson {
            reason: "positional_argument",
            context: vec![problem_count_u64("ordinal", *ordinal)],
        },
        Problem::Unknown { name } => DiagnosticProblemJson {
            reason: "unknown_argument",
            context: vec![problem_text("name", name.clone())],
        },
        Problem::Duplicate { name } => DiagnosticProblemJson {
            reason: "duplicate_argument",
            context: vec![problem_text("name", name.clone())],
        },
        Problem::Missing { name } => DiagnosticProblemJson {
            reason: "missing_argument",
            context: vec![problem_text("name", name.clone())],
        },
        Problem::EmptyString { name } => DiagnosticProblemJson {
            reason: "empty_string",
            context: vec![problem_text("name", name.clone())],
        },
    }
}

const fn native_link_kind_key(kind: bray_diagnostics::DiagnosticNativeLinkKind) -> &'static str {
    match kind {
        bray_diagnostics::DiagnosticNativeLinkKind::Dynamic => "dynamic",
        bray_diagnostics::DiagnosticNativeLinkKind::Static => "static",
        bray_diagnostics::DiagnosticNativeLinkKind::System => "system",
        bray_diagnostics::DiagnosticNativeLinkKind::Framework => "framework",
    }
}
