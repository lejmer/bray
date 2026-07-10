use bray_bound_tree::BoundUnitId;
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};

/// Whole-unit checker completion data available before language rules exist.
///
/// This service-output stub retains the unit identity so binder orchestration
/// cannot apply a completed result to another bound unit. This transfer value
/// is not itself published as bound state.
// TODO(checker): Add durable conclusion fields owned by bray-bound-tree as
// rules land.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct UnitCheckConclusions {
    unit: BoundUnitId,
}

impl UnitCheckConclusions {
    /// Creates the initial conclusions stub for an exact bound unit.
    pub const fn new(unit: BoundUnitId) -> Self {
        Self { unit }
    }

    /// Returns the bound unit these conclusions describe.
    pub const fn unit(self) -> BoundUnitId {
        self.unit
    }
}

/// The result of one focused checker service operation.
///
/// Completed operations own their structured diagnostics alongside the typed
/// value. Cancellation carries neither diagnostics nor partial conclusions, so
/// orchestration cannot accidentally publish abandoned checker work.
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
    use bray_diagnostics::{Diagnostic, DiagnosticBag, DiagnosticId, DiagnosticKind, SeverityKind};

    use super::{CheckerOutcome, UnitCheckConclusions};
    use bray_bound_tree::BoundUnitId;

    #[test]
    fn completed_outcomes_keep_typed_values_with_owned_diagnostics() {
        let diagnostic = Diagnostic::new(
            DiagnosticId::new(3),
            DiagnosticKind::DeclarationDuplicateName,
            SeverityKind::Error,
        );

        let conclusions = UnitCheckConclusions::new(BoundUnitId::new(4));
        let outcome =
            CheckerOutcome::complete(conclusions, DiagnosticBag::single(diagnostic.clone()));

        let Some(result) = outcome.result() else {
            panic!("completed checker outcomes retain their result");
        };

        assert_eq!(result.value(), &conclusions);
        assert_eq!(result.diagnostics().diagnostics(), &[diagnostic]);
    }

    #[test]
    fn cancelled_outcomes_expose_no_partial_result() {
        let outcome = CheckerOutcome::<UnitCheckConclusions>::Cancelled;

        assert!(outcome.is_cancelled());
        assert_eq!(outcome.into_result(), None);
    }

    #[test]
    fn outcomes_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<CheckerOutcome<UnitCheckConclusions>>();
    }
}
