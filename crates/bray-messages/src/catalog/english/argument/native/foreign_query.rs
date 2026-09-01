pub(super) fn format_english_foreign_query_failure(
    failure: &bray_diagnostics::DiagnosticForeignQueryFailure,
) -> String {
    use bray_diagnostics::DiagnosticForeignQueryFailure as Failure;

    let detail = match failure {
        Failure::Missing { context, data } => format!(
            "{} was unavailable for {} '{}'",
            foreign_query_name(data.as_str()),
            foreign_query_name(context.kind().as_str()),
            context.identity(),
        ),
        Failure::CountMismatch {
            context,
            data,
            expected,
            actual,
        } => format!(
            "{actual} {} values were available instead of {expected} for {} '{}'",
            foreign_query_name(data.as_str()),
            foreign_query_name(context.kind().as_str()),
            context.identity(),
        ),
        Failure::UnexpectedSemanticType {
            ty,
            expected,
            actual,
        } => format!(
            "semantic type data '{actual}' was retained instead of {expected} for type '{ty}'"
        ),
        Failure::UnexpectedTypeTemplate {
            context,
            expected,
            actual,
        } => format!(
            "type template '{actual}' was retained instead of {expected} for {} '{}'",
            foreign_query_name(context.kind().as_str()),
            context.identity(),
        ),
        Failure::UnexpectedGenericArgument {
            substitution,
            expected,
            actual,
        } => format!(
            "generic argument category '{actual}' was retained instead of '{expected}' in substitution '{substitution}'"
        ),
        Failure::NumericOverflow {
            context,
            value,
            target,
        } => format!(
            "value {value} cannot be represented as {target} for {} '{}'",
            foreign_query_name(context.kind().as_str()),
            context.identity(),
        ),
        Failure::InvalidPlatformServiceRole { role } => {
            format!("platform service role '{role}' is unsupported")
        }
        Failure::CallableSignature { function, cause } => {
            format!("function '{function}' has an invalid foreign callable signature: {cause}")
        }
        Failure::ConflictingSourceRoles {
            function,
            runtime,
            platform,
        } => format!(
            "function '{function}' has both runtime role '{runtime}' and platform role '{platform}'"
        ),
        Failure::DuplicateSourceRole {
            function,
            first,
            duplicate,
        } => {
            format!("function '{function}' has duplicate source roles '{first}' and '{duplicate}'")
        }
    };

    super::format_internal_compiler_error(detail)
}

fn foreign_query_name(name: &str) -> String {
    name.replace('_', " ")
}
