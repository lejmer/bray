pub(super) fn format_english_product_query_failure(
    failure: &bray_diagnostics::DiagnosticProductQueryFailure,
) -> String {
    let message = match failure.as_str() {
        "product_query_unsupported_constant_value" => {
            "a storage address cannot determine a generic specialization identity"
        }
        "product_query_generic_substitution" => {
            "the generic parameter count exceeds the compiler's supported limit"
        }
        "product_query_source_index" => "the source exceeds the compiler's supported text size",
        "product_query_test_product_mismatch" => {
            "test discovery requires the compilation's selected package and test product"
        }
        "product_query_codegen_backend_selection" => {
            "the requested native backend is not selected or installed"
        }
        "product_query_unsupported_entry_result_type" => {
            "the selected native profile cannot represent the executable entry result"
        }
        _ => "an exported declaration could not be represented for this product",
    };

    super::artifact::format_failure_detail(message, failure.context())
}
