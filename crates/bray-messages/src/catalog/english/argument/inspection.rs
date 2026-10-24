pub(super) fn format_english_inspection_failure(
    failure: &bray_diagnostics::DiagnosticInspectionFailure,
) -> String {
    use bray_diagnostics::DiagnosticInspectionFailure as Failure;

    match failure {
        Failure::Source { format, cause } => format!(
            "{} source inspection {}",
            inspection_format(*format),
            inspection_detail_cause(cause)
        ),
        Failure::Token { format, cause } => format!(
            "{} token inspection {}",
            inspection_format(*format),
            inspection_detail_cause(cause)
        ),
        Failure::Syntax { format, cause } => format!(
            "{} syntax inspection {}",
            inspection_format(*format),
            inspection_detail_cause(cause)
        ),
        Failure::Declaration { format, cause } => {
            format!(
                "{} declaration inspection {}",
                inspection_format(*format),
                inspection_detail_cause(cause)
            )
        }
        Failure::Symbol { format, cause } => format!(
            "{} symbol inspection {}",
            inspection_format(*format),
            symbol_inspection_cause(cause)
        ),
        Failure::Bound {
            target,
            format,
            cause,
        } => format!(
            "{} bound-tree inspection for {} {}",
            inspection_format(*format),
            inspection_target(*target),
            bound_inspection_cause(cause)
        ),
        Failure::Lowered {
            target,
            format,
            cause,
        } => {
            format!(
                "{} lowered-tree inspection for {} {}",
                inspection_format(*format),
                inspection_target(*target),
                lowered_inspection_cause(cause)
            )
        }
        Failure::Mir {
            target,
            format,
            cause,
        } => format!(
            "{} MIR inspection for {} {}",
            inspection_format(*format),
            inspection_target(*target),
            lowered_inspection_cause(cause)
        ),
    }
}

const fn inspection_format(
    format: bray_diagnostics::DiagnosticInspectionOutputFormat,
) -> &'static str {
    use bray_diagnostics::DiagnosticInspectionOutputFormat as Format;

    match format {
        Format::Text => "text",
        Format::Json => "JSON",
    }
}

fn inspection_target(target: bray_diagnostics::DiagnosticInspectionTarget) -> String {
    target.position.map_or_else(
        || format!("source {}", target.source_id),
        |position| format!("source {} at byte {position}", target.source_id),
    )
}

fn symbol_inspection_cause(cause: &bray_diagnostics::DiagnosticSymbolInspectionFailure) -> String {
    match cause {
        bray_diagnostics::DiagnosticSymbolInspectionFailure::Evaluation(failure) => {
            super::native::format_english_emission_evaluation_failure(failure)
        }
        bray_diagnostics::DiagnosticSymbolInspectionFailure::Detail(detail) => {
            inspection_detail_cause(detail)
        }
    }
}

fn bound_inspection_cause(cause: &bray_diagnostics::DiagnosticBoundInspectionFailure) -> String {
    use bray_diagnostics::DiagnosticBoundInspectionFailure as Cause;

    match cause {
        Cause::Evaluation(failure) => {
            return super::native::format_english_emission_evaluation_failure(failure);
        }
        Cause::Source => "requires a loaded source and a byte position within that source",
        Cause::Detail(detail) => return inspection_detail_cause(detail),
    }
    .to_owned()
}

fn lowered_inspection_cause(
    cause: &bray_diagnostics::DiagnosticLoweredInspectionFailure,
) -> String {
    use bray_diagnostics::DiagnosticLoweredInspectionFailure as Cause;

    match cause {
        Cause::Evaluation(failure) => {
            return super::native::format_english_emission_evaluation_failure(failure);
        }
        Cause::Source => "requires a loaded source and a byte position within that source",
        Cause::Detail(detail) => return inspection_detail_cause(detail),
    }
    .to_owned()
}

fn inspection_detail_cause(detail: &bray_diagnostics::DiagnosticInspectionFailureDetail) -> String {
    let message = match detail.reason() {
        "inspection_resource_limit" => {
            let resource = detail
                .context()
                .iter()
                .find_map(|field| {
                    if field.name() != "resource" {
                        return None;
                    }

                    match field.value() {
                        bray_diagnostics::DiagnosticFailureValue::Text(value) => {
                            Some(value.as_str())
                        }
                        _ => None,
                    }
                })
                .expect("inspection capacity failure must identify its resource");

            match resource {
                "source_text_bytes" => "exceeds the supported source size",
                "source_line_count" => "exceeds the supported source line count",
                "type_depth" => "exceeds the supported type nesting depth",
                "inspection_storage_alternative_count" => {
                    "exceeds the supported number of storage alternatives"
                }
                _ => panic!("inspection capacity has no English message: {resource}"),
            }
        }
        "source_inspection_json"
        | "token_inspection_json"
        | "syntax_inspection_json"
        | "declaration_inspection_json"
        | "symbol_inspection_json"
        | "bound_inspection_json"
        | "lowered_inspection_json" => "could not serialize its JSON report",
        reason => panic!("inspection failure has no English message: {reason}"),
    };

    super::native::format_failure_detail(message, detail.context())
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{
        DiagnosticFailureField, DiagnosticFailureValue, DiagnosticInspectionFailureDetail,
    };

    #[test]
    fn inspection_failures_render_their_specific_limit_and_report() {
        let limit = DiagnosticInspectionFailureDetail::new(
            "inspection_resource_limit",
            [
                DiagnosticFailureField::new(
                    "resource",
                    DiagnosticFailureValue::Text("type_depth".to_owned()),
                ),
                DiagnosticFailureField::new(
                    "maximum",
                    DiagnosticFailureValue::Text("256".to_owned()),
                ),
            ],
        );

        let rendered = super::inspection_detail_cause(&limit);

        assert!(rendered.contains("type nesting depth"));
        assert!(rendered.contains("supported limit: 256"));

        let report = DiagnosticInspectionFailureDetail::new(
            "bound_inspection_json",
            [DiagnosticFailureField::new(
                "report",
                DiagnosticFailureValue::Text("invalid map key".to_owned()),
            )],
        );

        let rendered = super::inspection_detail_cause(&report);

        assert!(rendered.contains("JSON report"));
        assert!(rendered.contains("invalid map key"));
    }
}
