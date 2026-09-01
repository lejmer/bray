pub(super) fn format_english_foreign_query_failure(
    failure: &bray_diagnostics::DiagnosticForeignQueryFailure,
) -> String {
    use bray_diagnostics::DiagnosticForeignQueryFailure as Failure;

    match failure {
        Failure::Missing { context, data } => format!(
            "an internal compiler error could not obtain {} for {} '{}'",
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
            "an internal compiler error found {actual} {} values instead of {expected} for {} '{}'",
            foreign_query_name(data.as_str()),
            foreign_query_name(context.kind().as_str()),
            context.identity(),
        ),
        Failure::UnexpectedSemanticType {
            ty,
            expected,
            actual,
        } => format!(
            "an internal compiler error found semantic type data '{actual}' instead of {expected} for type '{ty}'"
        ),
        Failure::UnexpectedTypeTemplate {
            context,
            expected,
            actual,
        } => format!(
            "an internal compiler error found type template '{actual}' instead of {expected} for {} '{}'",
            foreign_query_name(context.kind().as_str()),
            context.identity(),
        ),
        Failure::UnexpectedGenericArgument {
            substitution,
            expected,
            actual,
        } => format!(
            "an internal compiler error found generic argument category '{actual}' instead of '{expected}' in substitution '{substitution}'"
        ),
        Failure::NumericOverflow {
            context,
            value,
            target,
        } => format!(
            "an internal compiler error cannot represent value {value} as {target} for {} '{}'",
            foreign_query_name(context.kind().as_str()),
            context.identity(),
        ),
        Failure::InvalidPlatformServiceRole { role } => {
            format!("an internal compiler error found unsupported platform service role '{role}'")
        }
        Failure::CallableSignature { function, cause } => format!(
            "an internal compiler error found invalid foreign callable signature for function '{function}': {cause}"
        ),
        Failure::ConflictingSourceRoles {
            function,
            runtime,
            platform,
        } => format!(
            "an internal compiler error assigned runtime role '{runtime}' and platform role '{platform}' to function '{function}'"
        ),
        Failure::DuplicateSourceRole {
            function,
            first,
            duplicate,
        } => format!(
            "an internal compiler error assigned duplicate source roles '{first}' and '{duplicate}' to function '{function}'"
        ),
    }
}

fn foreign_query_name(name: &str) -> String {
    name.replace('_', " ")
}
