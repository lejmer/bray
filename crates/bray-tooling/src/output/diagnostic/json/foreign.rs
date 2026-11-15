use bray_diagnostics::{DiagnosticNativeLinkDirectiveProblem as LinkProblem, DiagnosticNativeSymbolDirectiveProblem as SymbolProblem, DiagnosticPlatformServiceSignatureProblem as ServiceProblem, DiagnosticPlatformAbiType as PlatformType, DiagnosticDirectiveArgumentProblem as ArgumentProblem};
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

impl DiagnosticProblemJson {
    pub(in crate::output::diagnostic::json) fn without_context(reason: &'static str) -> Self {
        Self { reason, context: Vec::new() }
    }
}

pub(in crate::output::diagnostic::json) fn native_link_directive_problem_json(
    problem: &bray_diagnostics::DiagnosticNativeLinkDirectiveProblem,
) -> DiagnosticProblemJson {
    match problem {
        LinkProblem::Argument(problem) => directive_argument_problem_json(problem),
        LinkProblem::UnsupportedKind { provided } => DiagnosticProblemJson {
            reason: "unsupported_kind",
            context: vec![problem_text("provided", provided.clone())],
        },
        LinkProblem::AmbiguousInput {
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
        SymbolProblem::Argument(problem) => directive_argument_problem_json(problem),
        SymbolProblem::MissingIdentity => DiagnosticProblemJson::without_context("missing_identity"),
        SymbolProblem::ConflictingIdentity => DiagnosticProblemJson::without_context("conflicting_identity"),
        SymbolProblem::UnsupportedValue { name, provided } => DiagnosticProblemJson {
            reason: "unsupported_value",
            context: vec![
                problem_text("name", name.clone()),
                problem_text("provided", provided.clone()),
            ],
        },
        SymbolProblem::UnsupportedTargetOption { name } => DiagnosticProblemJson {
            reason: "unsupported_target_option",
            context: vec![problem_text("name", name.clone())],
        },
        SymbolProblem::IncompatiblePolicy { name } => DiagnosticProblemJson {
            reason: "incompatible_policy",
            context: vec![problem_text("name", name.clone())],
        },
    }
}

pub(in crate::output::diagnostic::json) fn platform_service_signature_problem_json(
    problem: &bray_diagnostics::DiagnosticPlatformServiceSignatureProblem,
) -> DiagnosticProblemJson {
    match problem {
        ServiceProblem::CallableAbi { role, actual } => DiagnosticProblemJson {
            reason: "callable_abi",
            context: vec![
                problem_count_u64("role_id", u64::from(role.id())),
                problem_text("actual", actual.as_str()),
            ],
        },
        ServiceProblem::Execution { role, actual } => DiagnosticProblemJson {
            reason: "execution",
            context: vec![
                problem_count_u64("role_id", u64::from(role.id())),
                problem_text(
                    "actual",
                    actual.as_str(),
                ),
            ],
        },
        ServiceProblem::ParameterCount {
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
        ServiceProblem::ParameterType {
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
        ServiceProblem::ResultType {
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
    match ty {
        PlatformType::I32 => "i32",
        PlatformType::U32 => "u32",
        PlatformType::U64 => "u64",
        PlatformType::I64 => "i64",
        PlatformType::FunctionPointer => "function_pointer",
        PlatformType::PointerU8 => "pointer_u8",
        PlatformType::PointerU32 => "pointer_u32",
        PlatformType::PointerU64 => "pointer_u64",
        PlatformType::PointerI64 => "pointer_i64",
        PlatformType::RawAddressPointer => "raw_address_pointer",
        PlatformType::Path => "path",
        PlatformType::NativeText => "native_text",
        PlatformType::FileOptions => "file_options",
        PlatformType::FileMetadataPointer => "file_metadata_pointer",
        PlatformType::ChildRequest => "child_request",
        PlatformType::ExitStatusPointer => "exit_status_pointer",
        PlatformType::TemporalDateTime => "temporal_date_time",
        PlatformType::TemporalDateTimePointer => "temporal_date_time_pointer",
        PlatformType::TemporalObservationPointer => "temporal_observation_pointer",
        PlatformType::TemporalResolutionPointer => "temporal_resolution_pointer",
        PlatformType::TemporalValue => "temporal_value",
        PlatformType::TemporalValuePointer => "temporal_value_pointer",
        PlatformType::Status => "status",
    }
}

fn directive_argument_problem_json(
    problem: &bray_diagnostics::DiagnosticDirectiveArgumentProblem,
) -> DiagnosticProblemJson {
    match problem {
        ArgumentProblem::Positional { ordinal } => DiagnosticProblemJson {
            reason: "positional_argument",
            context: vec![problem_count_u64("ordinal", *ordinal)],
        },
        ArgumentProblem::Unknown { name } => DiagnosticProblemJson {
            reason: "unknown_argument",
            context: vec![problem_text("name", name.clone())],
        },
        ArgumentProblem::Duplicate { name } => DiagnosticProblemJson {
            reason: "duplicate_argument",
            context: vec![problem_text("name", name.clone())],
        },
        ArgumentProblem::Missing { name } => DiagnosticProblemJson {
            reason: "missing_argument",
            context: vec![problem_text("name", name.clone())],
        },
        ArgumentProblem::EmptyString { name } => DiagnosticProblemJson {
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
