use serde::Serialize;

use super::super::emission::{
    DiagnosticEmissionFailureJson, DiagnosticEmissionFieldJson, diagnostic_failure_context,
};

#[derive(Serialize)]
#[serde(tag = "domain", rename_all = "snake_case")]
pub(in crate::output::diagnostic::json) enum DiagnosticInspectionFailureJson {
    Source {
        format: &'static str,
        cause: &'static str,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        context: Vec<DiagnosticEmissionFieldJson>,
    },
    Token {
        format: &'static str,
        cause: &'static str,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        context: Vec<DiagnosticEmissionFieldJson>,
    },
    Syntax {
        format: &'static str,
        cause: &'static str,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        context: Vec<DiagnosticEmissionFieldJson>,
    },
    Declaration {
        format: &'static str,
        cause: &'static str,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        context: Vec<DiagnosticEmissionFieldJson>,
    },
    Symbol {
        format: &'static str,
        cause: &'static str,
        #[serde(skip_serializing_if = "Option::is_none")]
        evaluation: Option<DiagnosticEmissionFailureJson>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        context: Vec<DiagnosticEmissionFieldJson>,
    },
    Bound {
        target: DiagnosticInspectionTargetJson,
        format: &'static str,
        cause: &'static str,
        #[serde(skip_serializing_if = "Option::is_none")]
        evaluation: Option<DiagnosticEmissionFailureJson>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        context: Vec<DiagnosticEmissionFieldJson>,
    },
    Lowered {
        target: DiagnosticInspectionTargetJson,
        format: &'static str,
        cause: &'static str,
        #[serde(skip_serializing_if = "Option::is_none")]
        evaluation: Option<DiagnosticEmissionFailureJson>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        context: Vec<DiagnosticEmissionFieldJson>,
    },
    Mir {
        target: DiagnosticInspectionTargetJson,
        format: &'static str,
        cause: &'static str,
        #[serde(skip_serializing_if = "Option::is_none")]
        evaluation: Option<DiagnosticEmissionFailureJson>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        context: Vec<DiagnosticEmissionFieldJson>,
    },
}

#[derive(Serialize)]
pub(in crate::output::diagnostic::json) struct DiagnosticInspectionTargetJson {
    source_id: u32,
    position: Option<u32>,
}

impl DiagnosticInspectionFailureJson {
    pub(super) fn from_failure(failure: &bray_diagnostics::DiagnosticInspectionFailure) -> Self {
        use bray_diagnostics::DiagnosticInspectionFailure as Failure;

        match failure {
            Failure::Source { format, cause } => Self::Source {
                format: inspection_output_format_key(*format),
                cause: source_inspection_failure_key(cause),
                context: source_inspection_failure_context(cause),
            },
            Failure::Token { format, cause } => Self::Token {
                format: inspection_output_format_key(*format),
                cause: token_inspection_failure_key(cause),
                context: token_inspection_failure_context(cause),
            },
            Failure::Syntax { format, cause } => Self::Syntax {
                format: inspection_output_format_key(*format),
                cause: syntax_inspection_failure_key(cause),
                context: syntax_inspection_failure_context(cause),
            },
            Failure::Declaration { format, cause } => Self::Declaration {
                format: inspection_output_format_key(*format),
                cause: declaration_inspection_failure_key(cause),
                context: declaration_inspection_failure_context(cause),
            },
            Failure::Symbol { format, cause } => {
                let (cause, evaluation, context) = symbol_inspection_failure(cause);

                Self::Symbol {
                    format: inspection_output_format_key(*format),
                    cause,
                    evaluation,
                    context,
                }
            }
            Failure::Bound {
                target,
                format,
                cause,
            } => {
                let (cause, evaluation, context) = bound_inspection_failure(cause);

                Self::Bound {
                    target: inspection_target_json(*target),
                    format: inspection_output_format_key(*format),
                    cause,
                    evaluation,
                    context,
                }
            }
            Failure::Lowered {
                target,
                format,
                cause,
            } => {
                let (cause, evaluation, context) = lowered_inspection_failure(cause);

                Self::Lowered {
                    target: inspection_target_json(*target),
                    format: inspection_output_format_key(*format),
                    cause,
                    evaluation,
                    context,
                }
            }
            Failure::Mir {
                target,
                format,
                cause,
            } => {
                let (cause, evaluation, context) = lowered_inspection_failure(cause);

                Self::Mir {
                    target: inspection_target_json(*target),
                    format: inspection_output_format_key(*format),
                    cause,
                    evaluation,
                    context,
                }
            }
        }
    }
}

const fn inspection_output_format_key(
    format: bray_diagnostics::DiagnosticInspectionOutputFormat,
) -> &'static str {
    use bray_diagnostics::DiagnosticInspectionOutputFormat as Format;

    match format {
        Format::Text => "text",
        Format::Json => "json",
    }
}

const fn inspection_target_json(
    target: bray_diagnostics::DiagnosticInspectionTarget,
) -> DiagnosticInspectionTargetJson {
    DiagnosticInspectionTargetJson {
        source_id: target.source_id,
        position: target.position,
    }
}

const fn source_inspection_failure_key(
    cause: &bray_diagnostics::DiagnosticSourceInspectionFailure,
) -> &'static str {
    use bray_diagnostics::DiagnosticSourceInspectionFailure as Cause;

    match cause {
        Cause::SourceIndex => "source_index",
        Cause::Json => "json",
        Cause::Detail(detail) => detail.reason(),
    }
}

const fn token_inspection_failure_key(
    cause: &bray_diagnostics::DiagnosticTokenInspectionFailure,
) -> &'static str {
    use bray_diagnostics::DiagnosticTokenInspectionFailure as Cause;

    match cause {
        Cause::SourceIndex => "source_index",
        Cause::TokenText => "token_text",
        Cause::TriviaText => "trivia_text",
        Cause::Json => "json",
        Cause::Detail(detail) => detail.reason(),
    }
}

const fn syntax_inspection_failure_key(
    cause: &bray_diagnostics::DiagnosticSyntaxInspectionFailure,
) -> &'static str {
    use bray_diagnostics::DiagnosticSyntaxInspectionFailure as Cause;

    match cause {
        Cause::SourceIndex => "source_index",
        Cause::SourceMismatch => "source_mismatch",
        Cause::TokenText => "token_text",
        Cause::TriviaText => "trivia_text",
        Cause::TreeStructure => "tree_structure",
        Cause::Json => "json",
        Cause::Detail(detail) => detail.reason(),
    }
}

const fn declaration_inspection_failure_key(
    cause: &bray_diagnostics::DiagnosticDeclarationInspectionFailure,
) -> &'static str {
    use bray_diagnostics::DiagnosticDeclarationInspectionFailure as Cause;

    match cause {
        Cause::Container => "container",
        Cause::Declaration => "declaration",
        Cause::ModulePart => "module_part",
        Cause::Source => "source",
        Cause::SourceIndex => "source_index",
        Cause::Json => "json",
        Cause::Detail(detail) => detail.reason(),
    }
}

fn source_inspection_failure_context(
    cause: &bray_diagnostics::DiagnosticSourceInspectionFailure,
) -> Vec<DiagnosticEmissionFieldJson> {
    match cause {
        bray_diagnostics::DiagnosticSourceInspectionFailure::Detail(detail) => {
            diagnostic_failure_context(detail.context())
        }
        _ => Vec::new(),
    }
}

fn token_inspection_failure_context(
    cause: &bray_diagnostics::DiagnosticTokenInspectionFailure,
) -> Vec<DiagnosticEmissionFieldJson> {
    match cause {
        bray_diagnostics::DiagnosticTokenInspectionFailure::Detail(detail) => {
            diagnostic_failure_context(detail.context())
        }
        _ => Vec::new(),
    }
}

fn syntax_inspection_failure_context(
    cause: &bray_diagnostics::DiagnosticSyntaxInspectionFailure,
) -> Vec<DiagnosticEmissionFieldJson> {
    match cause {
        bray_diagnostics::DiagnosticSyntaxInspectionFailure::Detail(detail) => {
            diagnostic_failure_context(detail.context())
        }
        _ => Vec::new(),
    }
}

fn declaration_inspection_failure_context(
    cause: &bray_diagnostics::DiagnosticDeclarationInspectionFailure,
) -> Vec<DiagnosticEmissionFieldJson> {
    match cause {
        bray_diagnostics::DiagnosticDeclarationInspectionFailure::Detail(detail) => {
            diagnostic_failure_context(detail.context())
        }
        _ => Vec::new(),
    }
}

fn symbol_inspection_failure(
    cause: &bray_diagnostics::DiagnosticSymbolInspectionFailure,
) -> (
    &'static str,
    Option<DiagnosticEmissionFailureJson>,
    Vec<DiagnosticEmissionFieldJson>,
) {
    use bray_diagnostics::DiagnosticSymbolInspectionFailure as Cause;

    let cause = match cause {
        Cause::Evaluation(failure) => return evaluation_failure(failure),
        Cause::Detail(detail) => return detail_failure(detail),
        Cause::Declaration => "declaration",
        Cause::Json => "json",
        Cause::Source => "source",
        Cause::SourceIndex => "source_index",
        Cause::Symbol => "symbol",
        Cause::SymbolCycle => "symbol_cycle",
        Cause::Type => "type",
        Cause::UnsupportedRelationship => "unsupported_relationship",
    };

    (cause, None, Vec::new())
}

fn bound_inspection_failure(
    cause: &bray_diagnostics::DiagnosticBoundInspectionFailure,
) -> (
    &'static str,
    Option<DiagnosticEmissionFailureJson>,
    Vec<DiagnosticEmissionFieldJson>,
) {
    use bray_diagnostics::DiagnosticBoundInspectionFailure as Cause;

    let cause = match cause {
        Cause::Evaluation(failure) => return evaluation_failure(failure),
        Cause::Detail(detail) => return detail_failure(detail),
        Cause::Json => "json",
        Cause::MissingNode => "missing_node",
        Cause::Source => "source",
        Cause::SourceIndex => "source_index",
        Cause::Symbol => "symbol",
        Cause::Type => "type",
        Cause::Selection => "selection",
    };

    (cause, None, Vec::new())
}

fn lowered_inspection_failure(
    cause: &bray_diagnostics::DiagnosticLoweredInspectionFailure,
) -> (
    &'static str,
    Option<DiagnosticEmissionFailureJson>,
    Vec<DiagnosticEmissionFieldJson>,
) {
    use bray_diagnostics::DiagnosticLoweredInspectionFailure as Cause;

    let cause = match cause {
        Cause::Evaluation(failure) => return evaluation_failure(failure),
        Cause::Detail(detail) => return detail_failure(detail),
        Cause::Json => "json",
        Cause::Model => "model",
        Cause::Source => "source",
    };

    (cause, None, Vec::new())
}

fn evaluation_failure(
    failure: &bray_diagnostics::DiagnosticEmissionEvaluationFailure,
) -> (
    &'static str,
    Option<DiagnosticEmissionFailureJson>,
    Vec<DiagnosticEmissionFieldJson>,
) {
    (
        "evaluation",
        Some(DiagnosticEmissionFailureJson::from_evaluation(failure)),
        Vec::new(),
    )
}

fn detail_failure(
    detail: &bray_diagnostics::DiagnosticInspectionFailureDetail,
) -> (
    &'static str,
    Option<DiagnosticEmissionFailureJson>,
    Vec<DiagnosticEmissionFieldJson>,
) {
    (
        detail.reason(),
        None,
        diagnostic_failure_context(detail.context()),
    )
}

pub(super) const fn native_linker_build_failure_key(
    failure: bray_diagnostics::DiagnosticNativeLinkerBuildFailure,
) -> &'static str {
    use bray_diagnostics::{
        DiagnosticInvocationBuildFailure as Invocation,
        DiagnosticLinkerCapabilityBuildFailure as Capability,
        DiagnosticNativeLinkerBuildFailure as Failure,
    };

    match failure {
        Failure::ArchiveIdentity => "archive_identity",
        Failure::ArchiveDriverKindMismatch => "archive_driver_kind_mismatch",
        Failure::ArchiveCapabilities(Capability::DriverKindMismatch) => {
            "archive_capabilities_driver_kind_mismatch"
        }
        Failure::ArchiveCapabilities(Capability::UnsupportedTarget) => {
            "archive_capabilities_unsupported_target"
        }
        Failure::ArchiveCapabilities(Capability::MissingTargets) => {
            "archive_capabilities_missing_targets"
        }
        Failure::ArchiveCapabilities(Capability::DuplicateTarget) => {
            "archive_capabilities_duplicate_target"
        }
        Failure::ArchiveProgramPathNotExplicit => "archive_program_path_not_explicit",
        Failure::ArchiveInvocation(Invocation::EmptyProgram) => "archive_invocation_empty_program",
        Failure::ArchiveInvocation(Invocation::EmptyCurrentDirectory) => {
            "archive_invocation_empty_current_directory"
        }
        Failure::ArchiveInvocation(Invocation::EmptyEnvironmentVariableName) => {
            "archive_invocation_empty_environment_variable_name"
        }
        Failure::ArchiveInvocation(Invocation::DuplicateEnvironmentVariableName) => {
            "archive_invocation_duplicate_environment_variable_name"
        }
        Failure::ArchiveInvocation(Invocation::DuplicateResponseFile) => {
            "archive_invocation_duplicate_response_file"
        }
        Failure::SystemIdentity => "system_identity",
        Failure::TargetIdentity => "target_identity",
        Failure::SystemProgramPathNotExplicit => "system_program_path_not_explicit",
        Failure::SystemInvocation(Invocation::EmptyProgram) => "system_invocation_empty_program",
        Failure::SystemInvocation(Invocation::EmptyCurrentDirectory) => {
            "system_invocation_empty_current_directory"
        }
        Failure::SystemInvocation(Invocation::EmptyEnvironmentVariableName) => {
            "system_invocation_empty_environment_variable_name"
        }
        Failure::SystemInvocation(Invocation::DuplicateEnvironmentVariableName) => {
            "system_invocation_duplicate_environment_variable_name"
        }
        Failure::SystemInvocation(Invocation::DuplicateResponseFile) => {
            "system_invocation_duplicate_response_file"
        }
        Failure::SystemThinLtoCacheRootEmpty => "system_thin_lto_cache_root_empty",
        Failure::SystemDriverKindMismatch => "system_driver_kind_mismatch",
        Failure::SystemCapabilities(Capability::DriverKindMismatch) => {
            "system_capabilities_driver_kind_mismatch"
        }
        Failure::SystemCapabilities(Capability::UnsupportedTarget) => {
            "system_capabilities_unsupported_target"
        }
        Failure::SystemCapabilities(Capability::MissingTargets) => {
            "system_capabilities_missing_targets"
        }
        Failure::SystemCapabilities(Capability::DuplicateTarget) => {
            "system_capabilities_duplicate_target"
        }
        Failure::DuplicateDriver => "duplicate_driver",
    }
}
