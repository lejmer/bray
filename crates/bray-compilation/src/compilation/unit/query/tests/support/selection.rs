use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticArgName, DiagnosticArgValue, DiagnosticSelectionRejections,
};

pub(in crate::compilation::unit::query::tests) fn selection_rejections(
    diagnostic: &Diagnostic,
) -> &DiagnosticSelectionRejections {
    let Some(DiagnosticArgValue::SelectionRejections(rejections)) = diagnostic
        .args()
        .iter()
        .find(|argument| argument.name() == DiagnosticArgName::SelectionRejections)
        .map(DiagnosticArg::value)
    else {
        panic!("selection diagnostic must retain rejected candidates: {diagnostic:?}");
    };

    rejections
}
