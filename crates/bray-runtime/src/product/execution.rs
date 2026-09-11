use std::cell::Cell;

use bray_runtime_abi::{NativeRuntimeStatus, NativeStaticIdentity};

use crate::incident::OwnedCleanupIncident;

pub(crate) type RetainedProductExecution = triomphe::Arc<Box<dyn ProductExecution>>;
pub(crate) type RetainProductExecution =
    fn() -> Result<RetainedProductExecution, NativeRuntimeStatus>;

pub(crate) trait ProductCleanup: Send {
    fn run(
        self: Box<Self>,
        product: usize,
        entries: Vec<super::cleanup::StaticCleanup>,
        completed: fn(usize, NativeStaticIdentity, usize),
    ) -> Result<(), NativeRuntimeStatus>;
}

/// Execution owned by an already initialized scheduler, retained through product retirement.
pub(crate) trait ProductExecution: Send + Sync {
    fn admit_cleanup(
        &self,
        entries: &[super::cleanup::StaticCleanup],
    ) -> Result<Option<Box<dyn ProductCleanup>>, NativeRuntimeStatus>;
    fn owns_current_worker(&self) -> bool;
    fn admit_worker_cleanup(&self, product: usize) -> Result<(), NativeRuntimeStatus>;
    fn detach_workers(&self, product: usize);
    fn release(&self);
    /// Invokes the callback exactly once with this owner's cleanup execution bound.
    fn with_cleanup(&self, callback: &mut dyn FnMut()) -> Vec<OwnedCleanupIncident>;
}

thread_local! {
    // Scheduler entry installs this factory. Reading it never initializes execution.
    static EXECUTION_FACTORY: Cell<Option<RetainProductExecution>> = const { Cell::new(None) };
}

pub(crate) fn retain_execution() -> Result<Option<RetainedProductExecution>, NativeRuntimeStatus> {
    EXECUTION_FACTORY.get().map(|retain| retain()).transpose()
}

pub(crate) fn replace_execution_factory(
    factory: Option<RetainProductExecution>,
) -> Option<RetainProductExecution> {
    EXECUTION_FACTORY.replace(factory)
}

pub(crate) fn with_execution_factory<T>(
    factory: RetainProductExecution,
    callback: impl FnOnce() -> T,
) -> T {
    struct Restore(Option<RetainProductExecution>);

    impl Drop for Restore {
        fn drop(&mut self) {
            replace_execution_factory(self.0);
        }
    }

    let _restore = Restore(replace_execution_factory(Some(factory)));

    callback()
}

#[cfg(test)]
mod tests {
    use super::{retain_execution, with_execution_factory};
    use bray_runtime_abi::NativeRuntimeStatus;

    #[test]
    fn execution_binding_restores_absence_after_nested_unwind() {
        fn outer() -> Result<super::RetainedProductExecution, NativeRuntimeStatus> {
            Err(NativeRuntimeStatus::ALLOCATION_FAILURE)
        }

        fn inner() -> Result<super::RetainedProductExecution, NativeRuntimeStatus> {
            Err(NativeRuntimeStatus::RUNTIME_FAILURE)
        }

        assert!(retain_execution().unwrap().is_none());

        with_execution_factory(outer, || {
            assert_eq!(
                retain_execution().err(),
                Some(NativeRuntimeStatus::ALLOCATION_FAILURE)
            );

            let panic = std::panic::catch_unwind(|| {
                with_execution_factory(inner, || {
                    assert_eq!(
                        retain_execution().err(),
                        Some(NativeRuntimeStatus::RUNTIME_FAILURE)
                    );

                    panic!("unwind execution binding");
                })
            });

            assert!(panic.is_err());

            assert_eq!(
                retain_execution().err(),
                Some(NativeRuntimeStatus::ALLOCATION_FAILURE)
            );
        });

        assert!(retain_execution().unwrap().is_none());
    }
}
