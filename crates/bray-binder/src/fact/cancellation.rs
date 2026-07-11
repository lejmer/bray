/// Read-only cancellation observation for one binding request.
///
/// The compilation query layer owns the cancellation signal and publication behavior. Binder
/// operations observe that signal through this contract and return
/// [`BinderFactError::Cancelled`](crate::BinderFactError) without publishing partial work.
pub trait BinderCancellation: Send + Sync {
    /// Returns whether the current binding request should stop.
    fn is_cancelled(&self) -> bool;
}

impl<Observe> BinderCancellation for Observe
where
    Observe: Fn() -> bool + Send + Sync,
{
    fn is_cancelled(&self) -> bool {
        self()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Barrier,
        atomic::{AtomicBool, Ordering},
    };
    use std::thread;

    use super::BinderCancellation;

    #[test]
    fn closures_observe_concurrent_compilation_cancellation() {
        let barrier = Barrier::new(2);
        let cancelled = AtomicBool::new(false);
        let observe = || cancelled.load(Ordering::Acquire);

        assert!(!observe.is_cancelled());

        thread::scope(|scope| {
            scope.spawn(|| {
                barrier.wait();
                cancelled.store(true, Ordering::Release);
            });

            barrier.wait();

            while !observe.is_cancelled() {
                thread::yield_now();
            }
        });

        assert!(observe.is_cancelled());
    }
}
