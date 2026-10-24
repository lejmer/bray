pub(super) fn format_english_foreign_query_failure(
    failure: &bray_diagnostics::DiagnosticForeignQueryFailure,
) -> String {
    let message = match failure.as_str() {
        "foreign_query_numeric_overflow" => {
            "the native parameter count exceeds its supported range"
        }
        "foreign_query_conflicting_source_roles" => {
            "the same declaration cannot supply both a runtime role and a platform service"
        }
        _ => "the same declaration cannot supply more than one configured native role",
    };

    super::artifact::format_failure_detail(message, failure.context())
}
