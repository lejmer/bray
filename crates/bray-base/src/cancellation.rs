/// Read-only cancellation observation for one compiler operation.
pub trait Cancellation: Send + Sync {
    /// Returns whether the current operation should stop.
    fn is_cancelled(&self) -> bool;
}

impl<Observe> Cancellation for Observe
where
    Observe: Fn() -> bool + Send + Sync,
{
    fn is_cancelled(&self) -> bool {
        self()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::Cancellation;

    #[test]
    fn closures_expose_compilation_owned_cancellation() {
        let cancelled = AtomicBool::new(false);
        let observe = || cancelled.load(Ordering::Acquire);

        assert!(!observe.is_cancelled());

        cancelled.store(true, Ordering::Release);

        assert!(observe.is_cancelled());
    }
}
