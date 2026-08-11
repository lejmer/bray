use super::super::{DiagnosticArg, DiagnosticArgName, DiagnosticArgValue};

impl DiagnosticArg {
    /// Creates an argument carrying the target-predicate literal category supplied by the user.
    pub const fn actual_target_predicate_value_kind(
        kind: crate::DiagnosticTargetPredicateValueKind,
    ) -> Self {
        Self::new(
            DiagnosticArgName::ActualTargetPredicateValueKind,
            DiagnosticArgValue::TargetPredicateValueKind(kind),
        )
    }

    /// Creates an argument carrying the target-predicate literal category a property accepts.
    pub const fn expected_target_predicate_value_kind(
        kind: crate::DiagnosticTargetPredicateValueKind,
    ) -> Self {
        Self::new(
            DiagnosticArgName::ExpectedTargetPredicateValueKind,
            DiagnosticArgValue::TargetPredicateValueKind(kind),
        )
    }
}
