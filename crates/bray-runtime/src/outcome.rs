use crate::RuntimePanic;

/// Terminal outcome observed across one independently running boundary.
#[derive(Debug)]
pub enum RunOutcome<T> {
    /// The run completed normally with its result.
    Completed(T),
    /// The run completed cancellation cleanup without a result.
    Cancelled,
    /// A panic crossed the run boundary.
    Panicked(RuntimePanic),
}

impl<T> RunOutcome<T> {
    /// Returns the terminal outcome category.
    pub const fn kind(&self) -> RunOutcomeKind {
        match self {
            Self::Completed(_) => RunOutcomeKind::Completed,
            Self::Cancelled => RunOutcomeKind::Cancelled,
            Self::Panicked(_) => RunOutcomeKind::Panicked,
        }
    }
}

/// Terminal run category without its owned payload.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RunOutcomeKind {
    /// Normal completion.
    Completed,
    /// Cancellation.
    Cancelled,
    /// Panic.
    Panicked,
}
