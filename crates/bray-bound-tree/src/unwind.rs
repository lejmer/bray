use bray_symbols::CurrentRunCancellation;

/// Abnormal outcomes established by the owning unwind analysis.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct UnwindBehavior {
    may_unwind: bool,
    current_run_cancellation: CurrentRunCancellation,
}

impl UnwindBehavior {
    /// No abnormal outcome can propagate.
    pub const NONE: Self = Self::new(false, CurrentRunCancellation::NotEntered);
    /// Panic may propagate, while current-run cancellation is excluded.
    pub const PANIC: Self = Self::new(true, CurrentRunCancellation::NotEntered);
    /// Neither panic nor current-run cancellation is excluded.
    pub const ANY: Self = Self::new(true, CurrentRunCancellation::MayEnter);

    /// Creates an outcome bound. Cancellation is always an abnormal outcome.
    pub const fn new(may_unwind: bool, cancellation: CurrentRunCancellation) -> Self {
        Self {
            may_unwind: may_unwind || matches!(cancellation, CurrentRunCancellation::MayEnter),
            current_run_cancellation: cancellation,
        }
    }

    /// Whether panic or cancellation may propagate.
    pub const fn may_unwind(self) -> bool {
        self.may_unwind
    }

    /// Whether current-run cancellation may propagate.
    pub const fn current_run_cancellation(self) -> CurrentRunCancellation {
        self.current_run_cancellation
    }

    /// Combines the outcomes of reachable operations or dependencies.
    pub const fn merge(self, other: Self) -> Self {
        let cancellation = if matches!(
            self.current_run_cancellation,
            CurrentRunCancellation::MayEnter
        ) || matches!(
            other.current_run_cancellation,
            CurrentRunCancellation::MayEnter
        ) {
            CurrentRunCancellation::MayEnter
        } else {
            CurrentRunCancellation::NotEntered
        };

        Self::new(self.may_unwind || other.may_unwind, cancellation)
    }
}

/// Call occurrences and selected targets proven unable to propagate panic or cancellation.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct CheckedUnwind {
    calls: std::collections::BTreeSet<crate::BoundExpressionId>,
    targets: std::collections::BTreeSet<crate::BoundCallableTarget>,
    non_cancelling_calls: std::collections::BTreeSet<crate::BoundExpressionId>,
}

impl CheckedUnwind {
    /// Publishes the owning unwind analysis's occurrence and target results.
    pub fn new(
        calls: std::collections::BTreeSet<crate::BoundExpressionId>,
        targets: std::collections::BTreeSet<crate::BoundCallableTarget>,
        non_cancelling_calls: std::collections::BTreeSet<crate::BoundExpressionId>,
    ) -> Self {
        Self {
            calls,
            targets,
            non_cancelling_calls,
        }
    }

    /// Whether this invocation and its implicit argument conversions cannot unwind.
    pub fn call_cannot_unwind(&self, expression: crate::BoundExpressionId) -> bool {
        self.calls.contains(&expression)
    }

    /// Whether this invocation cannot propagate current-run cancellation.
    pub fn call_cannot_cancel(&self, expression: crate::BoundExpressionId) -> bool {
        self.non_cancelling_calls.contains(&expression)
    }

    /// Whether this selected target cannot unwind under the checked entry conditions.
    pub fn target_cannot_unwind(&self, target: crate::BoundCallableTarget) -> bool {
        self.targets.contains(&target)
    }
}
