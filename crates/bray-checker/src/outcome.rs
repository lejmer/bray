use bray_bound_tree::{BoundUnitId, BoundUnitKind, CheckedControlFlowFacts, ControlCompletion};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};

/// The control-flow facts established for one bound semantic unit.
///
/// This result is deliberately narrower than a complete semantic unit check.
/// It does not imply that type, storage, dependency, effect, capability, or
/// contract checking has completed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ControlFlowCheckResult {
    facts: CheckedControlFlowFacts,
}

impl ControlFlowCheckResult {
    pub(crate) const fn new(facts: CheckedControlFlowFacts) -> Self {
        Self { facts }
    }

    /// Returns the exact bound unit these control-flow facts describe.
    pub const fn unit(&self) -> BoundUnitId {
        self.facts.unit()
    }

    /// Returns the semantic category of the checked bound unit.
    pub const fn kind(&self) -> BoundUnitKind {
        self.facts.kind()
    }

    /// Returns the unit's checked control-completion categories.
    pub const fn completion(&self) -> ControlCompletion {
        self.facts.completion()
    }

    /// Returns whether conservative recovery affected control-flow checking.
    pub fn is_recovered(&self) -> bool {
        self.facts.is_recovered()
    }

    /// Returns the durable control-flow facts established by this check.
    pub fn into_facts(self) -> CheckedControlFlowFacts {
        self.facts
    }
}

/// The result of one focused checker service operation.
///
/// Completed operations own their structured diagnostics alongside the typed
/// value. Cancellation carries neither diagnostics nor partial results.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CheckerOutcome<T> {
    /// The operation completed with a typed value and its owned diagnostics.
    Complete(DiagnosticResult<T>),
    /// Cancellation was observed before the operation could complete.
    Cancelled,
}

impl<T> CheckerOutcome<T> {
    /// Creates a completed checker outcome.
    pub const fn complete(value: T, diagnostics: DiagnosticBag) -> Self {
        Self::Complete(DiagnosticResult::new(value, diagnostics))
    }

    /// Creates a completed checker outcome without diagnostics.
    pub fn without_diagnostics(value: T) -> Self {
        Self::Complete(DiagnosticResult::without_diagnostics(value))
    }

    /// Returns the completed result, or `None` after cancellation.
    pub const fn result(&self) -> Option<&DiagnosticResult<T>> {
        match self {
            Self::Complete(result) => Some(result),
            Self::Cancelled => None,
        }
    }

    /// Consumes the outcome into a completed result, or `None` after cancellation.
    pub fn into_result(self) -> Option<DiagnosticResult<T>> {
        match self {
            Self::Complete(result) => Some(result),
            Self::Cancelled => None,
        }
    }

    /// Returns whether cancellation prevented completion.
    pub const fn is_cancelled(&self) -> bool {
        matches!(self, Self::Cancelled)
    }
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{
        BoundUnitId, CheckedControlFlowFactsBuilder, ControlCompletion, ControlCompletionKind,
    };
    use bray_diagnostics::{Diagnostic, DiagnosticBag, DiagnosticId, DiagnosticKind, SeverityKind};

    use super::{CheckerOutcome, ControlFlowCheckResult};

    #[test]
    fn control_flow_results_keep_unit_category_completion_and_recovery_together() {
        let unit = BoundUnitId::new(4);
        let key = crate::test_support::callable_key();
        let (tree, _) = crate::test_support::recovered_tree(unit, &key);
        let completion = ControlCompletion::from_kinds([
            ControlCompletionKind::Return,
            ControlCompletionKind::Recovered,
        ]);

        let facts =
            match CheckedControlFlowFactsBuilder::new(unit).finish(tree.view(&key), completion) {
                Ok(facts) => facts,
                Err(error) => panic!("test control facts must validate: {error:?}"),
            };

        let result = ControlFlowCheckResult::new(facts);

        assert_eq!(result.unit(), unit);
        assert_eq!(result.kind(), bray_bound_tree::BoundUnitKind::CallableBody);
        assert_eq!(result.completion(), completion);
        assert!(result.is_recovered());
    }

    #[test]
    fn completed_outcomes_keep_typed_values_with_owned_diagnostics() {
        let diagnostic = Diagnostic::new(
            DiagnosticId::new(3),
            DiagnosticKind::DeclarationDuplicateName,
            SeverityKind::Error,
        );

        let unit = BoundUnitId::new(4);
        let key = crate::test_support::callable_key();
        let (tree, _) = crate::test_support::recovered_tree(unit, &key);

        let facts = match CheckedControlFlowFactsBuilder::new(unit)
            .finish(tree.view(&key), ControlCompletion::default())
        {
            Ok(facts) => facts,
            Err(error) => panic!("test control facts must validate: {error:?}"),
        };

        let result = ControlFlowCheckResult::new(facts);

        let outcome =
            CheckerOutcome::complete(result.clone(), DiagnosticBag::single(diagnostic.clone()));

        let Some(completed) = outcome.result() else {
            panic!("completed checker outcomes retain their result");
        };

        assert_eq!(completed.value(), &result);
        assert_eq!(completed.diagnostics().diagnostics(), &[diagnostic]);
    }

    #[test]
    fn cancelled_outcomes_expose_no_partial_result() {
        let outcome = CheckerOutcome::<ControlFlowCheckResult>::Cancelled;

        assert!(outcome.is_cancelled());
        assert_eq!(outcome.into_result(), None);
    }

    #[test]
    fn outcomes_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<CheckerOutcome<ControlFlowCheckResult>>();
    }
}
