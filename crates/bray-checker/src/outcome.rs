use bray_bound_tree::{BoundUnitId, BoundUnitKind, CheckedControlFlow, ControlCompletion};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};

use crate::CheckerInfrastructureError;

/// The control-flow result established for one bound semantic unit.
///
/// This result is deliberately narrower than a complete semantic unit check.
/// It does not imply that type, storage, dependency, effect, capability, or
/// contract checking has completed.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ControlFlowCheckResult {
    control_flow: CheckedControlFlow,
}

impl ControlFlowCheckResult {
    pub(crate) const fn new(
        unit: BoundUnitId,
        kind: BoundUnitKind,
        completion: ControlCompletion,
    ) -> Self {
        Self {
            control_flow: CheckedControlFlow::new(unit, kind, completion),
        }
    }

    /// Returns the exact bound unit this control-flow result describes.
    pub const fn unit(self) -> BoundUnitId {
        self.control_flow.unit()
    }

    /// Returns the semantic category of the checked bound unit.
    pub const fn kind(self) -> BoundUnitKind {
        self.control_flow.kind()
    }

    /// Returns the unit's checked control-completion categories.
    pub const fn completion(self) -> ControlCompletion {
        self.control_flow.completion()
    }

    /// Returns whether conservative recovery affected control-flow checking.
    pub const fn is_recovered(self) -> bool {
        self.control_flow.is_recovered()
    }

    /// Returns the durable control-flow result established by this check.
    pub const fn into_control_flow(self) -> CheckedControlFlow {
        self.control_flow
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
    /// Compiler infrastructure prevented the operation from completing.
    InfrastructureFailure(CheckerInfrastructureError),
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

    /// Returns the completed result, or `None` when checking did not complete.
    pub const fn result(&self) -> Option<&DiagnosticResult<T>> {
        match self {
            Self::Complete(result) => Some(result),
            Self::Cancelled | Self::InfrastructureFailure(_) => None,
        }
    }

    /// Consumes the outcome into a completed result, or `None` when checking did not complete.
    pub fn into_result(self) -> Option<DiagnosticResult<T>> {
        match self {
            Self::Complete(result) => Some(result),
            Self::Cancelled | Self::InfrastructureFailure(_) => None,
        }
    }

    /// Returns whether cancellation prevented completion.
    pub const fn is_cancelled(&self) -> bool {
        matches!(self, Self::Cancelled)
    }

    /// Returns the infrastructure failure that prevented completion.
    pub const fn infrastructure_failure(&self) -> Option<CheckerInfrastructureError> {
        match self {
            Self::InfrastructureFailure(error) => Some(*error),
            Self::Complete(_) | Self::Cancelled => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{BoundUnitId, BoundUnitKind, ControlCompletion, ControlCompletionKind};
    use bray_diagnostics::{Diagnostic, DiagnosticBag, DiagnosticId, DiagnosticKind, SeverityKind};

    use super::{CheckerOutcome, ControlFlowCheckResult};
    use crate::CheckerInfrastructureError;

    #[test]
    fn control_flow_results_keep_unit_category_completion_and_recovery_together() {
        let unit = BoundUnitId::new(4);

        let completion = ControlCompletion::from_kinds([
            ControlCompletionKind::Return,
            ControlCompletionKind::Recovered,
        ]);

        let result = ControlFlowCheckResult::new(unit, BoundUnitKind::CallableBody, completion);

        assert_eq!(result.unit(), unit);
        assert_eq!(result.kind(), BoundUnitKind::CallableBody);
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

        let result = ControlFlowCheckResult::new(
            BoundUnitId::new(4),
            BoundUnitKind::CallableBody,
            ControlCompletion::default(),
        );

        let outcome = CheckerOutcome::complete(result, DiagnosticBag::single(diagnostic.clone()));

        let Some(completed) = outcome.result() else {
            panic!("completed checker outcomes retain their result");
        };

        assert_eq!(completed.value(), &result);

        assert_eq!(
            completed.diagnostics().iter().cloned().collect::<Vec<_>>(),
            [diagnostic]
        );
    }

    #[test]
    fn cancelled_outcomes_expose_no_partial_result() {
        let outcome = CheckerOutcome::<ControlFlowCheckResult>::Cancelled;

        assert!(outcome.is_cancelled());
        assert_eq!(outcome.into_result(), None);
    }

    #[test]
    fn infrastructure_failures_are_distinct_from_cancellation_and_diagnostics() {
        let error = CheckerInfrastructureError::MissingSource {
            source_id: bray_source::SourceId::new(7),
        };

        let outcome = CheckerOutcome::<ControlFlowCheckResult>::InfrastructureFailure(error);

        assert!(!outcome.is_cancelled());
        assert_eq!(outcome.result(), None);
        assert_eq!(outcome.infrastructure_failure(), Some(error));
    }

    #[test]
    fn outcomes_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<CheckerOutcome<ControlFlowCheckResult>>();
    }
}
