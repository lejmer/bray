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
                cause: cause.reason(),
                context: diagnostic_failure_context(cause.context()),
            },
            Failure::Token { format, cause } => Self::Token {
                format: inspection_output_format_key(*format),
                cause: cause.reason(),
                context: diagnostic_failure_context(cause.context()),
            },
            Failure::Syntax { format, cause } => Self::Syntax {
                format: inspection_output_format_key(*format),
                cause: cause.reason(),
                context: diagnostic_failure_context(cause.context()),
            },
            Failure::Declaration { format, cause } => Self::Declaration {
                format: inspection_output_format_key(*format),
                cause: cause.reason(),
                context: diagnostic_failure_context(cause.context()),
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

fn symbol_inspection_failure(
    cause: &bray_diagnostics::DiagnosticSymbolInspectionFailure,
) -> (
    &'static str,
    Option<DiagnosticEmissionFailureJson>,
    Vec<DiagnosticEmissionFieldJson>,
) {
    use bray_diagnostics::DiagnosticSymbolInspectionFailure as Cause;

    match cause {
        Cause::Evaluation(failure) => evaluation_failure(failure),
        Cause::Detail(detail) => detail_failure(detail),
    }
}

fn bound_inspection_failure(
    cause: &bray_diagnostics::DiagnosticBoundInspectionFailure,
) -> (
    &'static str,
    Option<DiagnosticEmissionFailureJson>,
    Vec<DiagnosticEmissionFieldJson>,
) {
    use bray_diagnostics::DiagnosticBoundInspectionFailure as Cause;

    match cause {
        Cause::Evaluation(failure) => evaluation_failure(failure),
        Cause::Detail(detail) => detail_failure(detail),
        Cause::Source => ("source", None, Vec::new()),
    }
}

fn lowered_inspection_failure(
    cause: &bray_diagnostics::DiagnosticLoweredInspectionFailure,
) -> (
    &'static str,
    Option<DiagnosticEmissionFailureJson>,
    Vec<DiagnosticEmissionFieldJson>,
) {
    use bray_diagnostics::DiagnosticLoweredInspectionFailure as Cause;

    match cause {
        Cause::Evaluation(failure) => evaluation_failure(failure),
        Cause::Detail(detail) => detail_failure(detail),
        Cause::Source => ("source", None, Vec::new()),
    }
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
