use std::num::NonZeroUsize;

use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticKind, DiagnosticNote,
    DiagnosticNoteKind, SeverityKind,
};

/// Positive CPU worker budget for compiler-owned work.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct WorkerBudget {
    workers: NonZeroUsize,
}

impl WorkerBudget {
    /// Creates a worker budget from a positive worker count.
    pub fn new(workers: usize) -> Result<Self, WorkerBudgetError> {
        match NonZeroUsize::new(workers) {
            Some(workers) => Ok(Self::from_nonzero(workers)),
            None => Err(WorkerBudgetError::Zero),
        }
    }

    /// Creates a worker budget from a known non-zero worker count.
    pub const fn from_nonzero(workers: NonZeroUsize) -> Self {
        Self { workers }
    }

    /// Returns the serial worker budget.
    pub const fn serial() -> Self {
        Self {
            workers: NonZeroUsize::MIN,
        }
    }

    /// Returns a worker budget based on host parallelism, falling back to serial.
    pub fn available_parallelism() -> Self {
        match std::thread::available_parallelism() {
            Ok(workers) => Self::from_nonzero(workers),
            Err(_) => Self::serial(),
        }
    }

    /// Returns the positive worker count.
    pub const fn get(self) -> usize {
        self.workers.get()
    }
}

impl Default for WorkerBudget {
    fn default() -> Self {
        if cfg!(test) {
            Self::serial()
        } else {
            Self::available_parallelism()
        }
    }
}

/// Error returned when constructing a worker budget.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WorkerBudgetError {
    /// Worker budgets must be positive.
    Zero,
}

impl std::fmt::Display for WorkerBudgetError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Zero => formatter.write_str("worker budget must be positive"),
        }
    }
}

impl std::error::Error for WorkerBudgetError {}

impl WorkerBudgetError {
    /// Converts this user-facing worker-budget error into a diagnostic.
    pub fn into_diagnostic(self, id: DiagnosticId) -> Diagnostic {
        match self {
            Self::Zero => Diagnostic::new(
                id,
                DiagnosticKind::RequestInvalidWorkerBudget,
                SeverityKind::Error,
            )
            .with_arg(DiagnosticArg::worker_count(0))
            .with_note(DiagnosticNote::new(
                DiagnosticNoteKind::WorkerBudgetMustBePositive,
            )),
        }
    }

    /// Converts this user-facing worker-budget error into a diagnostic bag.
    pub fn into_diagnostic_bag(self) -> DiagnosticBag {
        DiagnosticBag::single(self.into_diagnostic(DiagnosticId::new(0)))
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{
        DiagnosticArg, DiagnosticArgName, DiagnosticArgValue, DiagnosticId, DiagnosticKind,
        DiagnosticNote, DiagnosticNoteKind,
    };

    use super::{WorkerBudget, WorkerBudgetError};

    #[test]
    fn worker_budgets_are_positive() {
        assert_eq!(WorkerBudget::new(0), Err(WorkerBudgetError::Zero));

        let budget = match WorkerBudget::new(4) {
            Ok(budget) => budget,
            Err(error) => panic!("test worker budget should be valid: {error:?}"),
        };

        assert_eq!(budget.get(), 4);

        assert_eq!(WorkerBudget::serial().get(), 1);
        assert!(WorkerBudget::default().get() >= 1);
    }

    #[test]
    fn worker_budget_errors_convert_to_diagnostics() {
        let diagnostic = WorkerBudgetError::Zero.into_diagnostic(DiagnosticId::new(2));

        assert_eq!(diagnostic.id(), DiagnosticId::new(2));

        assert_eq!(
            diagnostic.kind(),
            DiagnosticKind::RequestInvalidWorkerBudget
        );

        assert_eq!(
            diagnostic.args(),
            &[DiagnosticArg::new(
                DiagnosticArgName::WorkerCount,
                DiagnosticArgValue::WorkerCount(0)
            )]
        );

        assert_eq!(
            diagnostic.notes(),
            &[DiagnosticNote::new(
                DiagnosticNoteKind::WorkerBudgetMustBePositive
            )]
        );
    }
}
