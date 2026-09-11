use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::incident::OwnedCleanupIncident;
use crate::product::{ProductExecution, RetainedProductExecution};
use bray_runtime_abi::{
    NativeBrayCallOutcome, NativeRuntimeStatus, NativeStaticFinalizer, NativeStaticFinalizerStatus,
};

struct NativeProductExecution {
    retained: super::state::RetainedRuntime,
}

pub(in crate::native) fn retain_current_execution()
-> Result<RetainedProductExecution, NativeRuntimeStatus> {
    allocate_execution(super::state::retain_runtime()?)
}

fn allocate_execution(
    retained: super::state::RetainedRuntime,
) -> Result<RetainedProductExecution, NativeRuntimeStatus> {
    let owner = NativeProductExecution { retained };

    let storage = crate::allocation::reserve_storage::<NativeProductExecution>()
        .map_err(|_| NativeRuntimeStatus::ALLOCATION_FAILURE)?;

    let owner: Box<dyn ProductExecution> = Box::write(storage, owner);

    crate::allocation::allocate_shared(owner).map_err(|_| NativeRuntimeStatus::ALLOCATION_FAILURE)
}

impl Drop for NativeProductExecution {
    fn drop(&mut self) {
        self.retained.release();
    }
}

impl ProductExecution for NativeProductExecution {
    fn owns_current_worker(&self) -> bool {
        self.retained.owns_current_worker()
    }

    fn admit_worker_cleanup(&self, product: usize) -> Result<(), NativeRuntimeStatus> {
        self.retained.admit_product_worker_cleanup(product)
    }

    fn detach_workers(&self, product: usize) {
        self.retained.detach_product_workers(product);
    }

    fn release(&self) {
        self.retained.release();
    }

    fn with_cleanup(&self, callback: &mut dyn FnMut()) -> Vec<OwnedCleanupIncident> {
        super::static_finalizer::with_retained_static_cleanup_runtime(&self.retained, callback).1
    }

    fn run_finalizer(&self, finalizer: NativeStaticFinalizer) -> Vec<OwnedCleanupIncident> {
        let mut frame = super::inactive_frame_output();
        let destination = (&raw mut frame).addr();
        let mut outcome = NativeBrayCallOutcome::completed();

        let result = catch_unwind(AssertUnwindSafe(|| {
            (finalizer.start())(destination, &mut outcome)
        }));

        if let Some(incident) = OwnedCleanupIncident::boundary(outcome, finalizer.panics()) {
            let mut incidents = vec![incident];

            if let Err(payload) = result {
                incidents.push(OwnedCleanupIncident::host(payload));
            }

            return incidents;
        }

        match result {
            Ok(NativeStaticFinalizerStatus::SUCCESS) => {
                super::static_finalizer::run_static_finalizer(frame, finalizer.panics())
            }
            Ok(_) => vec![OwnedCleanupIncident::runtime_failure()],
            Err(payload) => vec![OwnedCleanupIncident::host(payload)],
        }
    }
}

fn admit_execution() -> Result<Option<RetainedProductExecution>, NativeRuntimeStatus> {
    let retained = match super::state::retain_runtime() {
        Ok(retained) => retained,
        Err(NativeRuntimeStatus::NOT_INITIALIZED) => super::state::admit_cleanup_runtime()?,
        Err(status) => return Err(status),
    };

    allocate_execution(retained).map(Some)
}

native_export! {
    /// Admits execution for an asynchronous product before publishing its host obligations.
    pub extern "C" fn bray_runtime_asynchronous_product_host_control(
        descriptor: &bray_runtime_abi::NativeProductHostDescriptor,
        operation: bray_runtime_abi::NativeProductHostOperation,
    ) -> bray_runtime_abi::NativeProductHostObservation {
        crate::product::control_with_execution(descriptor, operation, admit_execution)
    }
}

#[cfg(test)]
mod tests {
    use bray_runtime_abi::{
        NativeProductHostDescriptor, NativeProductHostOperation, NativeProductHostState,
        NativeProductHostStatus, NativeProductIdentity, NativeStaticHostEntry,
    };

    #[test]
    fn library_admission_retains_execution_without_main_authority_or_repeated_allocation() {
        extern "C" fn no_statics(_: usize) -> NativeStaticHostEntry {
            panic!("empty host has no static entries");
        }

        let execution = super::admit_execution().unwrap().unwrap();
        assert!(super::super::state::with_runtime(|_| ()).is_err());

        assert!(
            execution
                .with_cleanup(&mut || {
                    super::super::state::with_runtime(|runtime| assert!(!runtime.main_thread_lane))
                        .unwrap();
                })
                .is_empty()
        );

        execution.release();

        let descriptor =
            NativeProductHostDescriptor::new(NativeProductIdentity::new([157; 32]), no_statics, 0);

        let control = |operation| {
            super::bray_runtime_asynchronous_product_host_control(&descriptor, operation)
        };

        assert_eq!(
            control(NativeProductHostOperation::FORM).status(),
            NativeProductHostStatus::SUCCESS
        );

        assert!(super::super::state::with_runtime(|_| ()).is_err());

        assert_eq!(
            crate::test_support::with_allocation_failure(|| control(
                NativeProductHostOperation::FORM
            ))
            .status(),
            NativeProductHostStatus::SUCCESS
        );

        assert_eq!(
            control(NativeProductHostOperation::CLOSE).state(),
            NativeProductHostState::CLOSED
        );

        assert!(super::super::state::with_runtime(|_| ()).is_err());
    }
}
