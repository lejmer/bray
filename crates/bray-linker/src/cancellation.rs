/// Read-only cancellation observation for one native link operation.
///
/// Compilation owns cancellation and publication. Linker drivers observe this signal before
/// invocation, while waiting for external tools, before validating outputs, and before returning
/// success.
pub trait LinkCancellation: Send + Sync {
    /// Returns whether the current link operation should stop.
    fn is_cancelled(&self) -> bool;
}

impl<Observe> LinkCancellation for Observe
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

    use super::LinkCancellation;

    #[test]
    fn closures_observe_compilation_owned_cancellation() {
        let cancelled = AtomicBool::new(false);
        let observe = || cancelled.load(Ordering::Acquire);

        assert!(!observe.is_cancelled());

        cancelled.store(true, Ordering::Release);

        assert!(observe.is_cancelled());
    }
}
