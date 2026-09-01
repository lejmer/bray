use super::failure::{DiagnosticEmissionFieldJson, count_u64_field, text_field};

pub(in crate::output::diagnostic::json) fn foreign_query_failure_context(
    failure: &bray_diagnostics::DiagnosticForeignQueryFailure,
) -> Vec<DiagnosticEmissionFieldJson> {
    use bray_diagnostics::DiagnosticForeignQueryFailure as Failure;

    let mut fields = vec![text_field("cause", failure.as_str())];

    match failure {
        Failure::Missing { context, data } => {
            fields.extend(context_fields(context));
            fields.push(text_field("data_kind", data.as_str()));
        }
        Failure::CountMismatch {
            context,
            data,
            expected,
            actual,
        } => {
            fields.extend(context_fields(context));

            fields.extend([
                text_field("data_kind", data.as_str()),
                count_usize_field("expected_count", *expected),
                count_usize_field("actual_count", *actual),
            ]);
        }
        Failure::UnexpectedSemanticType {
            ty,
            expected,
            actual,
        } => fields.extend([
            text_field("semantic_type", ty),
            text_field("expected_type_kind", expected),
            text_field("actual_semantic_type", actual),
        ]),
        Failure::UnexpectedTypeTemplate {
            context,
            expected,
            actual,
        } => {
            fields.extend(context_fields(context));

            fields.extend([
                text_field("expected_type_kind", expected),
                text_field("actual_type_template", actual),
            ]);
        }
        Failure::UnexpectedGenericArgument {
            substitution,
            expected,
            actual,
        } => fields.extend([
            text_field("substitution", substitution),
            text_field("expected_argument_kind", expected),
            text_field("actual_argument_kind", actual),
        ]),
        Failure::NumericOverflow {
            context,
            value,
            target,
        } => {
            fields.extend(context_fields(context));

            fields.extend([
                count_usize_field("value", *value),
                text_field("integer_width", target),
            ]);
        }
        Failure::InvalidPlatformServiceRole { role } => {
            fields.push(text_field("platform_service_role", role));
        }
        Failure::CallableSignature { function, cause } => fields.extend([
            text_field("function", function),
            text_field("signature_cause", cause),
        ]),
        Failure::ConflictingSourceRoles {
            function,
            runtime,
            platform,
        } => fields.extend([
            text_field("function", function),
            text_field("runtime_role", runtime),
            text_field("platform_role", platform),
        ]),
        Failure::DuplicateSourceRole {
            function,
            first,
            duplicate,
        } => fields.extend([
            text_field("function", function),
            text_field("first_role", first),
            text_field("duplicate_role", duplicate),
        ]),
    }

    fields
}

fn context_fields(
    context: &bray_diagnostics::DiagnosticForeignQueryContext,
) -> [DiagnosticEmissionFieldJson; 2] {
    [
        text_field("foreign_context_kind", context.kind().as_str()),
        text_field("foreign_context_identity", context.identity()),
    ]
}

fn count_usize_field(name: &'static str, value: usize) -> DiagnosticEmissionFieldJson {
    let value = u64::try_from(value).expect("supported pointer widths fit diagnostic counts");

    count_u64_field(name, value)
}
