use std::collections::BTreeSet;

use bray_bound_tree::{
    CheckedAnonymousCallable, CheckedCallableBody, CheckedConstantTemplateUnit,
    CheckedConstraintUnit, CheckedContractClauseUnit, CheckedPredicateDefinitionUnit,
    CheckedRuntimeDefaultUnit,
};
use bray_diagnostics::DiagnosticResult;

use crate::BinderDependency;

/// One complete semantic-unit stage computation awaiting compilation-owned publication.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckedUnitComputation<T> {
    result: DiagnosticResult<T>,
    dependencies: Box<[BinderDependency]>,
}

impl<T> CheckedUnitComputation<T> {
    /// Creates a complete computation with canonical deterministic dependencies.
    pub fn new(
        result: DiagnosticResult<T>,
        dependencies: impl IntoIterator<Item = BinderDependency>,
    ) -> Self {
        let dependencies = dependencies
            .into_iter()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
            .into_boxed_slice();

        Self {
            result,
            dependencies,
        }
    }

    /// Returns the atomic diagnostic-bearing checked value.
    pub const fn result(&self) -> &DiagnosticResult<T> {
        &self.result
    }

    /// Returns every semantic fact observed by the computation in canonical order.
    pub fn dependencies(&self) -> &[BinderDependency] {
        &self.dependencies
    }

    /// Consumes the computation into its publication parts.
    pub fn into_parts(self) -> (DiagnosticResult<T>, Box<[BinderDependency]>) {
        (self.result, self.dependencies)
    }
}

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
    use bray_diagnostics::{
        Diagnostic, DiagnosticBag, DiagnosticId, DiagnosticKind, DiagnosticResult, SeverityKind,
    };

    use super::{
        CheckedAnonymousCallableResult, CheckedCallableBodyResult, CheckedConstantTemplateResult,
        CheckedConstraintResult, CheckedContractClauseResult, CheckedPredicateDefinitionResult,
        CheckedRuntimeDefaultResult, CheckedUnitComputation,
    };
    use crate::BinderDependency;
    use crate::BindingOutcome;

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
        assert_send_sync::<CheckedUnitComputation<u32>>();
    }

    #[test]
    fn checked_unit_computations_canonicalize_dependencies() {
        let dependency = BinderDependency::Target(bray_symbols::ConstantSymbolId::from_symbol_id(
            bray_symbols::SymbolId::new(7),
        ));
        let result = DiagnosticResult::without_diagnostics(11_u32);

        let computation =
            CheckedUnitComputation::new(result.clone(), [dependency.clone(), dependency.clone()]);

        assert_eq!(computation.result(), &result);
        assert_eq!(computation.dependencies(), &[dependency]);
    }
}
