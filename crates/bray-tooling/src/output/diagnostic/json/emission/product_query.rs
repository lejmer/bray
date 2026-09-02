use super::context::diagnostic_failure_context;
use super::failure::{DiagnosticEmissionFieldJson, text_field};

pub(in crate::output::diagnostic::json) fn product_query_failure_context(
    failure: &bray_diagnostics::DiagnosticProductQueryFailure,
) -> Vec<DiagnosticEmissionFieldJson> {
    let mut context = vec![text_field("cause", failure.as_str())];
    context.extend(diagnostic_failure_context(failure.context()));

    context
}
