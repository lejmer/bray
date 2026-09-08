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
pub enum CheckerOutcome<T, Upstream = std::convert::Infallible> {
    /// The operation completed with a typed value and its owned diagnostics.
    Complete(DiagnosticResult<T>),
    /// Cancellation was observed before the operation could complete.
    Cancelled,
    /// Compiler infrastructure prevented the operation from completing.
    // rust-style: broad-failure
    InfrastructureFailure(CheckerInfrastructureError),
    /// The coordinating query layer returned one of its own exact failures.
    UpstreamFailure(Upstream),
}

impl<T, Upstream> CheckerOutcome<T, Upstream> {
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
            Self::Cancelled | Self::InfrastructureFailure(_) | Self::UpstreamFailure(_) => None,
        }
    }

    /// Consumes the outcome into a completed result, or `None` when checking did not complete.
    pub fn into_result(self) -> Option<DiagnosticResult<T>> {
        match self {
            Self::Complete(result) => Some(result),
            Self::Cancelled | Self::InfrastructureFailure(_) | Self::UpstreamFailure(_) => None,
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
            Self::Complete(_) | Self::Cancelled | Self::UpstreamFailure(_) => None,
        }
    }
}

impl<T> CheckerOutcome<T> {
    /// Widens a checker-local outcome to a boundary with an upstream error type.
    pub fn with_upstream<Upstream>(self) -> CheckerOutcome<T, Upstream> {
        match self {
            Self::Complete(result) => CheckerOutcome::Complete(result),
            Self::Cancelled => CheckerOutcome::Cancelled,
            Self::InfrastructureFailure(error) => CheckerOutcome::InfrastructureFailure(error),
            Self::UpstreamFailure(error) => match error {},
        }
    }
}

impl<T, Upstream> From<crate::CheckerQueryError<Upstream>> for CheckerOutcome<T, Upstream> {
    fn from(error: crate::CheckerQueryError<Upstream>) -> Self {
        match error {
            crate::CheckerQueryError::Cancelled => Self::Cancelled,
            crate::CheckerQueryError::Infrastructure(error) => Self::InfrastructureFailure(error),
            crate::CheckerQueryError::Upstream(error) => Self::UpstreamFailure(error),
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
    fn query_failures_preserve_upstream_identity_and_cancellation() {
        let upstream: CheckerOutcome<(), u32> = crate::CheckerQueryError::Upstream(17).into();
        let cancelled: CheckerOutcome<(), u32> = crate::CheckerQueryError::Cancelled.into();

        let error = CheckerInfrastructureError::MissingSource {
            source_id: bray_source::SourceId::new(7),
        };

        let infrastructure: CheckerOutcome<(), u32> =
            crate::CheckerQueryError::Infrastructure(error).into();

        assert_eq!(upstream, CheckerOutcome::UpstreamFailure(17));
        assert_eq!(cancelled, CheckerOutcome::Cancelled);
        assert_eq!(infrastructure, CheckerOutcome::InfrastructureFailure(error));
    }

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

        let outcome =
            CheckerOutcome::<_>::complete(result, DiagnosticBag::single(diagnostic.clone()));

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
