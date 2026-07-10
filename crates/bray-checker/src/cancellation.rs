/// Read-only cancellation observation for checker operations.
pub trait CheckerCancellation: Sync {
    /// Returns whether the current compiler operation should stop.
    fn is_cancelled(&self) -> bool;
}

impl<Observe> CheckerCancellation for Observe
where
    Observe: Fn() -> bool + Sync,
{
    fn is_cancelled(&self) -> bool {
        self()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::CheckerCancellation;

    #[test]
    fn closures_adapt_compilation_owned_cancellation_state() {
        let cancelled = AtomicBool::new(false);
        let observe = || cancelled.load(Ordering::Acquire);

        assert!(!observe.is_cancelled());

        cancelled.store(true, Ordering::Release);

        assert!(observe.is_cancelled());
    }
}
