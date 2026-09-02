pub(super) fn format_english_product_query_failure(
    _failure: &bray_diagnostics::DiagnosticProductQueryFailure,
) -> String {
    super::format_internal_compiler_error(
        "product construction violated an internal contract",
    )
}
