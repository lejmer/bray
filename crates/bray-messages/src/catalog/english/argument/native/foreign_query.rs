pub(super) fn format_english_foreign_query_failure(
    _failure: &bray_diagnostics::DiagnosticForeignQueryFailure,
) -> String {
    super::format_internal_compiler_error(
        "native-boundary construction violated an internal contract",
    )
}
