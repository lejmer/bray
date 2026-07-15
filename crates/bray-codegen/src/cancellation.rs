/// Read-only cancellation observation for one code generation request.
///
/// Backends must observe cancellation between major phases and before returning artifacts.
pub trait CodegenCancellation: Send + Sync {
    /// Returns whether the current code generation request should stop.
    fn is_cancelled(&self) -> bool;
}

impl<Observe> CodegenCancellation for Observe
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

    use super::CodegenCancellation;

    #[test]
    fn closures_expose_compilation_owned_cancellation() {
        let cancelled = AtomicBool::new(false);
        let observe = || cancelled.load(Ordering::Acquire);

        assert!(!observe.is_cancelled());

        cancelled.store(true, Ordering::Release);

        assert!(observe.is_cancelled());
    }
}
