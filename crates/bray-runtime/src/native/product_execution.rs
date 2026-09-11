use crate::incident::OwnedCleanupIncident;
use crate::product::{ProductCleanup, ProductExecution, RetainedProductExecution, StaticCleanup};
use bray_runtime_abi::NativeRuntimeStatus;

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
    fn admit_cleanup(
        &self,
        entries: &[StaticCleanup],
    ) -> Result<Option<Box<dyn ProductCleanup>>, NativeRuntimeStatus> {
        if !entries.iter().any(|entry| {
            entry.finalizer.execution() == bray_runtime_abi::NativeCleanupExecution::ASYNCHRONOUS
        }) {
            return Ok(None);
        }

        NativeProductCleanup::prepare(&self.retained, entries).map(Some)
    }

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
}

struct NativeProductCleanup {
    core: std::sync::Arc<super::state::NativeRuntimeCore>,
    handle: bray_runtime_abi::NativeTaskHandle,
    prepared: Option<(
        super::state::NativeRunReservation,
        Box<std::mem::MaybeUninit<super::run::NativeHostSequence>>,
        triomphe::Arc<super::frame::NativeTerminalState>,
    )>,
}

impl NativeProductCleanup {
    fn prepare(
        retained: &super::state::RetainedRuntime,
        entries: &[StaticCleanup],
    ) -> Result<Box<dyn ProductCleanup>, NativeRuntimeStatus> {
        let descriptor = super::run::cleanup_descriptor(
            entries
                .iter()
                .filter_map(|entry| entry.finalizer.metadata()),
        )?;

        let terminal = super::frame::NativeTerminalState::reserve()?;

        let mut reservation = super::state::NativeRunReservation::prepare(
            descriptor,
            terminal,
            crate::task::TaskAdmissionKind::Continuation,
        )?;

        reservation.reserve(&retained.core.scheduler)?;

        let sequence = crate::allocation::reserve_storage::<super::run::NativeHostSequence>()
            .map_err(|_| NativeRuntimeStatus::ALLOCATION_FAILURE)?;

        let terminal = super::frame::NativeTerminalState::reserve()?;

        let storage = crate::allocation::reserve_storage::<Self>()
            .map_err(|_| NativeRuntimeStatus::ALLOCATION_FAILURE)?;

        let allocation = retained
            .core
            .allocate_kind(crate::task::TaskAdmissionKind::Continuation, false);

        let handle = allocation.task().ok_or(allocation.status())?;

        Ok(Box::write(
            storage,
            Self {
                core: std::sync::Arc::clone(&retained.core),
                handle,
                prepared: Some((reservation, sequence, terminal)),
            },
        ))
    }
}

impl Drop for NativeProductCleanup {
    fn drop(&mut self) {
        self.core.release_task_reservation(self.handle);
    }
}

impl ProductCleanup for NativeProductCleanup {
    fn run(
        mut self: Box<Self>,
        product: usize,
        entries: Vec<StaticCleanup>,
        completed: fn(usize, bray_runtime_abi::NativeStaticIdentity, usize),
    ) -> Result<(), NativeRuntimeStatus> {
        let (reservation, storage, terminal) = self
            .prepared
            .take()
            .ok_or(NativeRuntimeStatus::ALREADY_INITIALIZED)?;

        reservation.run().install_sequence(Box::write(
            storage,
            super::run::NativeHostSequence::Static(super::run::NativeStaticSequence::new(
                product, entries, terminal, completed,
            )),
        ));

        let run = triomphe::Arc::clone(reservation.run());

        let result = super::state::with_runtime(|runtime| {
            if !std::sync::Arc::ptr_eq(&runtime.core, &self.core) {
                return Err(NativeRuntimeStatus::INVALID_ARGUMENT);
            }

            let status = runtime.start_run(self.handle, reservation);

            if !status.is_success() {
                return Err(status);
            }

            let root = bray_runtime_abi::NativeRootHandle::new(self.handle.raw())
                .ok_or(NativeRuntimeStatus::INVALID_ARGUMENT)?;

            let outcome = runtime.observe_root(root);

            if outcome.state() != bray_runtime_abi::NativeRunState::COMPLETED {
                // Keep unresolved execution owned by the failed domain. It cannot authorize unload.
                return Err(NativeRuntimeStatus::RUNTIME_FAILURE);
            }

            let status = runtime.resolve_root_completion(root);

            if status.is_success() {
                Ok(())
            } else {
                Err(status)
            }
        })
        .and_then(|result| result);

        if result.is_err() {
            self.core.retain_failed_run(self.handle, run);
        }

        result
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
    thread_local! {
        static PHASE: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
        static RUN: std::cell::Cell<Option<crate::TaskId>> = const { std::cell::Cell::new(None) };
    }

    fn phase(expected: usize) {
        assert_eq!(PHASE.get() % 5, expected);
        PHASE.set(PHASE.get() + 1);
        let task = crate::current_task_execution_context().unwrap().task();

        if let Some(first) = RUN.get() {
            assert_eq!(task, first);
        } else {
            RUN.set(Some(task));
        }

        crate::native::state::with_runtime(|runtime| {
            assert_eq!(runtime.scheduler.task_count().unwrap(), 1);
        })
        .unwrap();
    }

    extern "C" fn metadata() -> Option<&'static bray_runtime_abi::NativeFrameMetadata> {
        static METADATA: bray_runtime_abi::NativeFrameMetadata =
            bray_runtime_abi::NativeFrameMetadata::new(
                [158; 32],
                1,
                1,
                1,
                0,
                1,
                crate::test_support::native_origin_frame_state,
            );

        Some(&METADATA)
    }

    extern "C" fn prepare() {
        phase(0);
    }
    extern "C-unwind" fn start(
        _: usize,
        _: &mut bray_runtime_abi::NativeBrayCallOutcome,
    ) -> bray_runtime_abi::NativeStaticFinalizerStatus {
        phase(1);

        bray_runtime_abi::NativeStaticFinalizerStatus::INCIDENT
    }
    extern "C-unwind" fn destroy() -> bray_runtime_abi::NativeBrayCallOutcome {
        phase(2);

        bray_runtime_abi::NativeBrayCallOutcome::completed()
    }
    extern "C" fn detach() {
        phase(3);
    }
    fn completed(_: usize, _: bray_runtime_abi::NativeStaticIdentity, count: usize) {
        assert_eq!(count, 1);
        phase(4);
    }
    extern "C-unwind" fn unexpected(_: usize) -> bray_runtime_abi::NativeRuntimeStatus {
        panic!("runtime incidents do not use provider panic callbacks");
    }

    fn entry() -> crate::product::StaticCleanup {
        crate::product::StaticCleanup {
            identity: bray_runtime_abi::NativeStaticIdentity::new([158; 32]),
            order: 0,
            prepare,
            destroy,
            detach,
            finalizer: bray_runtime_abi::NativeStaticFinalizer::new(
                bray_runtime_abi::NativeCleanupExecution::ASYNCHRONOUS,
                Some(metadata),
                start,
                crate::test_support::panic_callbacks(unexpected, unexpected),
            ),
        }
    }

    #[test]
    fn admitted_batch_keeps_one_run_and_drains_start_failures_without_new_capacity() {
        let execution = super::admit_execution().unwrap().unwrap();
        PHASE.set(0);
        RUN.set(None);
        let entries = vec![entry(); 32];
        let mut driver = execution.admit_cleanup(&entries).unwrap();
        let mut entries = Some(entries);

        let incidents = execution.with_cleanup(&mut || {
            crate::test_support::with_allocation_failure(|| {
                driver
                    .take()
                    .unwrap()
                    .run(0, entries.take().unwrap(), completed)
                    .unwrap();
            });
        });

        assert!(incidents.is_empty());
        assert_eq!(PHASE.get(), 32 * 5);
        execution.release();
    }

    #[test]
    fn failed_binding_keeps_unstarted_batch_in_its_admitted_native_slot() {
        let retained = super::super::state::admit_cleanup_runtime().unwrap();
        let entries = vec![entry()];
        let driver = super::NativeProductCleanup::prepare(&retained, &entries).unwrap();
        assert!(super::super::state::with_runtime(|_| ()).is_err());

        assert_eq!(
            driver.run(0, entries, completed),
            Err(bray_runtime_abi::NativeRuntimeStatus::NOT_INITIALIZED)
        );

        let handle = *retained.core.tasks.lock().unwrap().keys().next().unwrap();
        retained.core.release_task_reservation(handle);

        let removed = {
            let mut tasks = retained.core.tasks.lock().unwrap();
            assert_eq!(tasks.len(), 1);

            // This synthetic batch owns no provider storage, so the test may discard it explicitly.
            std::mem::take(&mut *tasks)
        };

        drop(removed);
        retained.release();
    }

    #[test]
    fn admission_rejects_main_cleanup_in_a_domain_without_main_authority() {
        extern "C" fn main_metadata() -> Option<&'static bray_runtime_abi::NativeFrameMetadata> {
            static METADATA: bray_runtime_abi::NativeFrameMetadata =
                bray_runtime_abi::NativeFrameMetadata::new(
                    [159; 32],
                    1,
                    1,
                    1,
                    0,
                    1,
                    crate::test_support::native_main_frame_state,
                );

            Some(&METADATA)
        }

        let execution = super::admit_execution().unwrap().unwrap();
        let mut entry = entry();

        entry.finalizer = bray_runtime_abi::NativeStaticFinalizer::new(
            bray_runtime_abi::NativeCleanupExecution::ASYNCHRONOUS,
            Some(main_metadata),
            start,
            crate::test_support::panic_callbacks(unexpected, unexpected),
        );

        assert!(execution.admit_cleanup(&[entry]).is_err());
        execution.release();
    }
}
