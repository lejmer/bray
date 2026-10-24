use bray_diagnostics::DiagnosticSemanticQueryFailure;

pub(crate) fn diagnostic_symbol_capacity_failure(index: usize) -> DiagnosticSemanticQueryFailure {
    DiagnosticSemanticQueryFailure::new(
        "symbol_capacity",
        "symbol_capacity_exceeded",
        [super::diagnostic_context::natural_field("index", index)],
    )
}
