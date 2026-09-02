use bray_diagnostics::{
    DiagnosticEvaluationFailureDetail, DiagnosticFailureField, DiagnosticFailureValue,
};

use super::diagnostic_context::identity;

pub(crate) fn diagnostic_cycle_failure(
    cycle: &super::FactCycle,
) -> DiagnosticEvaluationFailureDetail {
    let facts = cycle.facts();

    DiagnosticEvaluationFailureDetail::new(
        "cycle",
        [
            DiagnosticFailureField::new(
                "fact_kinds",
                DiagnosticFailureValue::TextList(
                    facts
                        .iter()
                        .map(super::runtime_diagnostic::compilation_fact_kind)
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                        .into_boxed_slice(),
                ),
            ),
            DiagnosticFailureField::new(
                "fact_identities",
                DiagnosticFailureValue::IdentityList(
                    facts
                        .iter()
                        .map(identity)
                        .collect::<Vec<_>>()
                        .into_boxed_slice(),
                ),
            ),
        ],
    )
}
