use bray_bound_tree::{
    CheckedAnonymousCallable, CheckedCallableBody, CheckedConstantTemplateUnit,
    CheckedConstraintUnit, CheckedContractClauseUnit, CheckedPredicateDefinitionUnit,
    CheckedRuntimeDefaultUnit,
};
use bray_diagnostics::DiagnosticResult;

/// The canonical diagnostic-bearing fact payload for a declared callable body.
pub type CheckedCallableBodyResult = DiagnosticResult<CheckedCallableBody>;

/// The canonical diagnostic-bearing fact payload for an anonymous callable.
pub type CheckedAnonymousCallableResult = DiagnosticResult<CheckedAnonymousCallable>;

/// The canonical diagnostic-bearing fact payload for a runtime-default expression.
pub type CheckedRuntimeDefaultResult = DiagnosticResult<CheckedRuntimeDefaultUnit>;

/// The canonical diagnostic-bearing fact payload for a constant definition template.
pub type CheckedConstantTemplateResult = DiagnosticResult<CheckedConstantTemplateUnit>;

/// The canonical diagnostic-bearing fact payload for a predicate definition.
pub type CheckedPredicateDefinitionResult = DiagnosticResult<CheckedPredicateDefinitionUnit>;

/// The canonical diagnostic-bearing fact payload for a declaration constraint.
pub type CheckedConstraintResult = DiagnosticResult<CheckedConstraintUnit>;

/// The canonical diagnostic-bearing fact payload for a callable contract clause.
pub type CheckedContractClauseResult = DiagnosticResult<CheckedContractClauseUnit>;

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use bray_diagnostics::{Diagnostic, DiagnosticBag, DiagnosticId, DiagnosticKind, SeverityKind};

    use super::{
        CheckedAnonymousCallableResult, CheckedCallableBodyResult, CheckedConstantTemplateResult,
        CheckedConstraintResult, CheckedContractClauseResult, CheckedPredicateDefinitionResult,
        CheckedRuntimeDefaultResult,
    };
    use crate::{BinderCancellation, BindingOutcome};

    #[test]
    fn binding_outcomes_publish_values_and_diagnostics_together() {
        let diagnostic = Diagnostic::new(
            DiagnosticId::new(5),
            DiagnosticKind::DeclarationDuplicateName,
            SeverityKind::Error,
        );
        let outcome = BindingOutcome::complete(7_u32, DiagnosticBag::single(diagnostic.clone()));

        let Some(result) = outcome.result() else {
            panic!("completed binding must retain its atomic result");
        };

        assert_eq!(result.value(), &7);
        assert_eq!(result.diagnostics().diagnostics(), &[diagnostic]);
    }

    #[test]
    fn binding_cancellation_exposes_no_partial_fact_payload() {
        let cancelled = AtomicBool::new(false);
        let observe = || cancelled.load(Ordering::Acquire);

        assert!(!observe.is_cancelled());

        cancelled.store(true, Ordering::Release);

        assert!(observe.is_cancelled());

        let outcome = BindingOutcome::<u32>::Cancelled;

        assert!(outcome.is_cancelled());
        assert_eq!(outcome.into_result(), None);
    }

    #[test]
    fn checked_unit_fact_payloads_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<CheckedCallableBodyResult>();
        assert_send_sync::<CheckedAnonymousCallableResult>();
        assert_send_sync::<CheckedRuntimeDefaultResult>();
        assert_send_sync::<CheckedConstantTemplateResult>();
        assert_send_sync::<CheckedPredicateDefinitionResult>();
        assert_send_sync::<CheckedConstraintResult>();
        assert_send_sync::<CheckedContractClauseResult>();
    }
}
