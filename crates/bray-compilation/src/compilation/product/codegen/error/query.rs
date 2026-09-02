use bray_diagnostics::DiagnosticNativeProductFailureKind;

use crate::fact::FactQueryError;

pub(super) fn fact_query_failure_kind(
    error: &FactQueryError,
) -> Option<DiagnosticNativeProductFailureKind> {
    (!matches!(error, FactQueryError::Cancelled))
        .then(|| error.diagnostic_evaluation_failure().into())
}
