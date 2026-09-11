use std::panic::{AssertUnwindSafe, catch_unwind};

use bray_runtime_abi::{
    NativeFrameEntry, NativeFrameMetadata, NativeRootConstructor, NativeRootHandle,
    NativeRootStart, NativeRuntimeConfiguration, NativeRuntimeStatus, NativeTaskHandle,
};

use super::frame::{NativeFrame, NativeFrameTransfer, NativeTerminalState};
use super::frames::FrameShape;
use super::run::NativeActivationReservation;
use super::state::{NativeRunReservation, NativeRuntimeCore, with_runtime};

// Owns the unpublished table slot until startup either commits it or rolls back.
struct RootAdmission {
    core: std::sync::Arc<NativeRuntimeCore>,
    handle: NativeTaskHandle,
}

impl Drop for RootAdmission {
    fn drop(&mut self) {
        self.core.release_task_reservation(self.handle);
    }
}

pub(super) fn execute(
    metadata: Option<&NativeFrameMetadata>,
    construct: NativeRootConstructor,
    configuration: NativeRuntimeConfiguration,
) -> NativeRootStart {
    match start(metadata, construct, configuration) {
        Ok(root) => NativeRootStart::success(root),
        Err(status) => NativeRootStart::failure(status),
    }
}

fn start(
    metadata: Option<&NativeFrameMetadata>,
    construct: NativeRootConstructor,
    configuration: NativeRuntimeConfiguration,
) -> Result<NativeRootHandle, NativeRuntimeStatus> {
    let metadata = metadata.ok_or(NativeRuntimeStatus::INVALID_ARGUMENT)?;
    let descriptor = NativeFrame::checked_descriptor(metadata)?;
    let status = super::export::initialize_for_execution(configuration);

    if !status.is_success() {
        return Err(status);
    }

    let terminal = NativeTerminalState::reserve()?;

    // The run and constructor failure path share the admitted incident destination.
    let mut reservation = NativeRunReservation::prepare(
        descriptor.clone(),
        triomphe::Arc::clone(&terminal),
        crate::task::TaskAdmissionKind::Independent,
    )?;

    let admission = with_runtime(|runtime| {
        reservation.reserve(&runtime.scheduler)?;
        let allocation = runtime.allocate();
        let handle = allocation.task().ok_or(allocation.status())?;

        Ok(RootAdmission {
            core: std::sync::Arc::clone(&runtime.core),
            handle,
        })
    })??;

    let result = super::incident::with_incident_owner(&terminal, || {
        let mut transfer = construct_frame(construct)?;
        let actual = transfer.frame().metadata();

        if FrameShape::of(actual) != FrameShape::of(metadata)
            || !NativeFrame::matches_metadata_states(&descriptor, actual)
        {
            return Err(NativeRuntimeStatus::INVALID_ARGUMENT);
        }

        let activation =
            match super::frames::claim(transfer.frame().context(), transfer.entry(), actual)? {
                Some(claim) => claim.install_activation(transfer.take()),
                None => {
                    // Native constructors have no owned captures or asynchronous rollback obligation.
                    // Their raw frame can still be discarded if activation admission fails.
                    let activation = NativeActivationReservation::prepare(actual)?;

                    activation.install(transfer.take())
                }
            };

        reservation.run().install_root(activation);

        let status = with_runtime(|runtime| {
            if !std::sync::Arc::ptr_eq(&runtime.core, &admission.core) {
                return NativeRuntimeStatus::INVALID_ARGUMENT;
            }

            runtime.start_run(admission.handle, reservation)
        })?;

        if !status.is_success() {
            return Err(status);
        }

        NativeRootHandle::new(admission.handle.raw()).ok_or(NativeRuntimeStatus::RUNTIME_FAILURE)
    });

    if result.is_err() {
        super::incident::retain_cleanup_incidents(terminal.take_cleanup_incidents());
    }

    result
}

fn construct_frame(
    construct: NativeRootConstructor,
) -> Result<NativeFrameTransfer, NativeRuntimeStatus> {
    let mut output = super::frame::inactive_frame_output();

    let status = match catch_unwind(AssertUnwindSafe(|| construct(&mut output))) {
        Ok(status) => status,
        Err(payload) => {
            let incident = crate::incident::OwnedCleanupIncident::host(payload);

            // Startup installed its admitted incident owner before invoking the constructor.
            let _ = super::incident::retain_cleanup_incident(incident);

            NativeRuntimeStatus::PANICKED
        }
    };

    let transfer = (output.context() != 0)
        .then(|| NativeFrameTransfer::from_inactive(output, NativeFrameEntry::Body));

    if !status.is_success() {
        return Err(status);
    }

    transfer.ok_or(NativeRuntimeStatus::INVALID_ARGUMENT)
}

#[cfg(test)]
mod tests {
    use super::{
        NativeFrameEntry, NativeFrameMetadata, NativeRuntimeConfiguration, NativeRuntimeStatus,
        execute,
    };
    use crate::native::state::{initialize, shutdown, with_runtime};
    use crate::test_support::{
        native_origin_frame_state, with_allocation_failure, with_allocation_failure_after,
    };
    use bray_runtime_abi::{
        NativeFrameExit, NativeFrameProgress, NativeFrameProgressKind, NativeInactiveFrame,
        NativeProtectedFrame, NativeRunState,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};

    static CONSTRUCTIONS: AtomicUsize = AtomicUsize::new(0);
    static BODIES: AtomicUsize = AtomicUsize::new(0);
    thread_local! { static ADMITTED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }

    fn metadata() -> NativeFrameMetadata {
        NativeFrameMetadata::new([211; 32], 1, 8, 8, 0, 1, native_origin_frame_state)
    }

    extern "C-unwind" fn reject(_: &mut NativeInactiveFrame) -> NativeRuntimeStatus {
        CONSTRUCTIONS.fetch_add(1, Ordering::Relaxed);

        with_runtime(|runtime| {
            assert_eq!(runtime.tasks.lock().unwrap().len(), 1);
            assert_eq!(runtime.independent_tasks.load(Ordering::Relaxed), 1);
            assert!(runtime.allocate().task().is_none());
        })
        .unwrap();

        NativeRuntimeStatus::ALLOCATION_FAILURE
    }

    #[test]
    fn admission_failure_skips_construction_and_constructor_failure_returns_capacity() {
        let configuration = NativeRuntimeConfiguration::new(1, 1);
        assert!(initialize(configuration).is_success());
        CONSTRUCTIONS.store(0, Ordering::Relaxed);
        let start = with_allocation_failure(|| execute(Some(&metadata()), reject, configuration));
        assert_eq!(start.status(), NativeRuntimeStatus::ALLOCATION_FAILURE);
        assert_eq!(CONSTRUCTIONS.load(Ordering::Relaxed), 0);

        for _ in 0..3 {
            assert_eq!(
                execute(Some(&metadata()), reject, configuration).status(),
                NativeRuntimeStatus::ALLOCATION_FAILURE
            );

            with_runtime(|runtime| {
                assert!(runtime.tasks.lock().unwrap().is_empty());
                assert_eq!(runtime.independent_tasks.load(Ordering::Relaxed), 0);
                assert_eq!(runtime.scheduler.task_count().unwrap(), 0);
            })
            .unwrap();
        }

        assert_eq!(CONSTRUCTIONS.load(Ordering::Relaxed), 3);
        assert!(shutdown().is_success());
    }

    extern "C-unwind" fn construct_admitted(
        output: &mut NativeInactiveFrame,
    ) -> NativeRuntimeStatus {
        *output = NativeInactiveFrame::new(ADMITTED.get(), adapter);

        // The enclosing test installs a finite budget. Exhaust it before root publication.
        while !crate::test_support::allocation_should_fail() {}

        NativeRuntimeStatus::SUCCESS
    }

    extern "C" fn adapter(context: usize, _: NativeFrameEntry) -> NativeProtectedFrame {
        NativeProtectedFrame::new(
            context,
            metadata(),
            body,
            body,
            ignore,
            resolve,
            move_completion,
            release,
        )
    }
    extern "C-unwind" fn body(_: usize) -> NativeFrameProgress {
        BODIES.fetch_add(1, Ordering::Relaxed);

        NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 0)
    }
    extern "C-unwind" fn ignore(_: usize) {}
    extern "C-unwind" fn resolve(_: usize, _: NativeFrameExit) {}
    extern "C-unwind" fn move_completion(_: usize, _: usize) {}
    extern "C-unwind" fn release(context: usize) {
        crate::native::frames::bray_runtime_frame_storage_release(context);
    }

    #[test]
    fn constructed_generated_root_uses_admitted_activation_without_late_allocation() {
        let configuration = NativeRuntimeConfiguration::new(1, 1);
        assert!(initialize(configuration).is_success());
        BODIES.store(0, Ordering::Relaxed);

        let address =
            crate::native::frames::bray_runtime_frame_storage_admission(Some(&metadata()));

        assert_ne!(address, 0);
        ADMITTED.set(address);

        let start = with_allocation_failure_after(128, || {
            execute(Some(&metadata()), construct_admitted, configuration)
        });

        assert_eq!(start.status(), NativeRuntimeStatus::SUCCESS);
        assert_eq!(BODIES.load(Ordering::Relaxed), 0);

        with_runtime(|runtime| {
            assert_eq!(runtime.scheduler.task_count().unwrap(), 1);
            let root = start.root().unwrap();

            assert_eq!(
                runtime.observe_root(root).state(),
                NativeRunState::COMPLETED
            );

            assert_eq!(
                runtime.resolve_root_completion(root),
                NativeRuntimeStatus::SUCCESS
            );

            assert!(runtime.tasks.lock().unwrap().is_empty());
        })
        .unwrap();

        assert_eq!(BODIES.load(Ordering::Relaxed), 1);
        assert!(!crate::native::frames::is_admitted(address));
        assert!(shutdown().is_success());
    }
}
