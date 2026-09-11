use crate::native::state::{initialize, shutdown, with_runtime};
use crate::test_support::{with_allocation_failure, with_allocation_failure_after};
use bray_runtime_abi::{
    NativeBrayCallOutcome, NativeCleanupExecution, NativeFrameEntry, NativeFrameExit,
    NativeFrameMetadata, NativeFrameProgress, NativeFrameProgressKind, NativeInactiveFrame,
    NativeProductHostDescriptor, NativeProductHostOperation, NativeProductHostState,
    NativeProductIdentity, NativeProtectedFrame, NativeRuntimeConfiguration, NativeRuntimeStatus,
    NativeStaticHostEntry, NativeValueCleanup,
};
use std::cell::Cell;

thread_local! {
    static FRAME: Cell<usize> = const { Cell::new(0) };
    static VALUE: Cell<usize> = const { Cell::new(0) };
    static TRACE: Cell<usize> = const { Cell::new(0) };
    static DESTROYED: Cell<usize> = const { Cell::new(0) };
}

static METADATA: NativeFrameMetadata = NativeFrameMetadata::new(
    [176; 32],
    1,
    8,
    8,
    0,
    1,
    crate::test_support::native_origin_frame_state,
);
extern "C" fn metadata() -> Option<&'static NativeFrameMetadata> {
    Some(&METADATA)
}
extern "C" fn no_statics(_: usize) -> NativeStaticHostEntry {
    panic!("empty product has no statics");
}
extern "C-unwind" fn unexpected_panic(_: usize) -> NativeRuntimeStatus {
    panic!("no provider panic");
}
fn cleanup(start: bray_runtime_abi::NativeValueCleanupCallback) -> NativeValueCleanup {
    NativeValueCleanup::new(
        NativeCleanupExecution::ASYNCHRONOUS,
        Some(metadata),
        start,
        crate::test_support::panic_callbacks(unexpected_panic, unexpected_panic),
    )
}
extern "C-unwind" fn start(
    value: usize,
    frame: &mut NativeInactiveFrame,
    _: &mut NativeBrayCallOutcome,
) -> NativeRuntimeStatus {
    assert_eq!(value, VALUE.get());
    assert_eq!(TRACE.replace(2), 1);
    *frame = NativeInactiveFrame::new(FRAME.replace(0), adapter);

    NativeRuntimeStatus::SUCCESS
}
extern "C-unwind" fn broadcast(
    value: usize,
    _: &mut NativeInactiveFrame,
    _: &mut NativeBrayCallOutcome,
) -> NativeRuntimeStatus {
    assert_eq!(value, VALUE.get());
    assert_eq!(TRACE.replace(1), 0);

    NativeRuntimeStatus::SUCCESS
}
extern "C" fn adapter(context: usize, _: NativeFrameEntry) -> NativeProtectedFrame {
    NativeProtectedFrame::new(
        context, METADATA, resume, resume, ignore, resolve, complete, release,
    )
}
extern "C-unwind" fn resume(_: usize) -> NativeFrameProgress {
    let previous = TRACE.get();
    TRACE.set(previous + 1);

    NativeFrameProgress::new(
        if previous == 2 {
            NativeFrameProgressKind::YIELDED
        } else {
            NativeFrameProgressKind::COMPLETED
        },
        0,
        0,
    )
}
extern "C-unwind" fn ignore(_: usize) {}
extern "C-unwind" fn resolve(_: usize, _: NativeFrameExit) {}
extern "C-unwind" fn complete(_: usize, _: usize) {}
extern "C-unwind" fn release(context: usize) {
    DESTROYED.set(DESTROYED.get() + 1);
    crate::native::frames::bray_runtime_frame_storage_release(context);
}
extern "C-unwind" fn reject(
    _: usize,
    _: &mut NativeInactiveFrame,
    _: &mut NativeBrayCallOutcome,
) -> NativeRuntimeStatus {
    NativeRuntimeStatus::ALLOCATION_FAILURE
}

extern "C-unwind" fn unregistered(
    _: usize,
    frame: &mut NativeInactiveFrame,
    _: &mut NativeBrayCallOutcome,
) -> NativeRuntimeStatus {
    *frame = NativeInactiveFrame::new(7, adapter);

    NativeRuntimeStatus::SUCCESS
}

#[test]
fn result_admission_rolls_back_every_fallible_step_and_unused_release_unpins_product() {
    let capacity = crate::test_support::cleanup_capacity_binding();
    assert!(initialize(NativeRuntimeConfiguration::new(1, 1)).is_success());

    let descriptor =
        NativeProductHostDescriptor::new(NativeProductIdentity::new([177; 32]), no_statics, 0);

    let control = |operation| crate::product::control(&descriptor, operation, Some(&capacity));

    assert_eq!(
        control(NativeProductHostOperation::FORM).state(),
        NativeProductHostState::OPEN
    );

    let product = std::ptr::from_ref(&descriptor).addr();

    with_runtime(|runtime| {
        let mut succeeded = false;

        for budget in 0..128 {
            let admitted = with_allocation_failure_after(budget, || {
                runtime.admit_returned_value(product, 32, 16, 16, None, cleanup(reject))
            });

            match admitted {
                Ok((handle, address)) => {
                    assert_eq!(address % 16, 0);

                    assert_eq!(
                        control(NativeProductHostOperation::OBSERVE).active_entries(),
                        1
                    );

                    assert!(
                        with_allocation_failure(|| runtime.release_returned_value(handle))
                            .is_success()
                    );

                    succeeded = true;
                }
                Err(status) => assert_eq!(status, NativeRuntimeStatus::ALLOCATION_FAILURE),
            }

            assert_eq!(
                control(NativeProductHostOperation::OBSERVE).active_entries(),
                0
            );

            assert!(runtime.tasks.lock().unwrap().is_empty());
            assert_eq!(runtime.scheduler.task_count().unwrap(), 0);

            if succeeded {
                break;
            }
        }

        assert!(succeeded);
    })
    .unwrap();

    assert_eq!(
        control(NativeProductHostOperation::CLOSE).state(),
        NativeProductHostState::CLOSED
    );

    assert!(shutdown().is_success());
}

#[test]
fn admitted_result_drives_broadcast_and_suspending_lifecycle_without_late_run_allocation() {
    let capacity = crate::test_support::cleanup_capacity_binding();
    assert!(initialize(NativeRuntimeConfiguration::new(1, 1)).is_success());

    let descriptor =
        NativeProductHostDescriptor::new(NativeProductIdentity::new([178; 32]), no_statics, 0);

    let control = |operation| crate::product::control(&descriptor, operation, Some(&capacity));

    assert_eq!(
        control(NativeProductHostOperation::FORM).state(),
        NativeProductHostState::OPEN
    );

    let product = std::ptr::from_ref(&descriptor).addr();
    let address = crate::native::frames::bray_runtime_frame_storage_admission(Some(&METADATA));
    assert_ne!(address, 0);
    FRAME.set(address);
    TRACE.set(0);

    let broadcast = NativeValueCleanup::new(
        NativeCleanupExecution::SYNCHRONOUS,
        None,
        broadcast,
        crate::test_support::panic_callbacks(unexpected_panic, unexpected_panic),
    );

    with_runtime(|runtime| {
        let (handle, result) = runtime
            .admit_returned_value(product, 32, 16, 16, Some(broadcast), cleanup(start))
            .unwrap();

        VALUE.set(result + 16);
        assert_eq!(runtime.scheduler.task_count().unwrap(), 0);

        assert_eq!(
            control(NativeProductHostOperation::CLOSE).state(),
            NativeProductHostState::CLOSING
        );

        assert!(with_allocation_failure(|| runtime.resolve_returned_value(handle)).is_success());
        assert_eq!(TRACE.get(), 4);
        assert!(runtime.tasks.lock().unwrap().is_empty());
    })
    .unwrap();

    assert!(!crate::native::frames::is_admitted(address));

    assert_eq!(
        control(NativeProductHostOperation::OBSERVE).state(),
        NativeProductHostState::CLOSED
    );

    assert!(shutdown().is_success());
}

#[test]
fn rejected_lifecycle_keeps_result_and_product_owned() {
    let capacity = crate::test_support::cleanup_capacity_binding();
    assert!(initialize(NativeRuntimeConfiguration::new(1, 1)).is_success());

    let descriptors = [179, 180].map(|identity| {
        NativeProductHostDescriptor::new(NativeProductIdentity::new([identity; 32]), no_statics, 0)
    });

    for (index, (descriptor, start)) in descriptors
        .iter()
        .zip([
            reject as bray_runtime_abi::NativeValueCleanupCallback,
            unregistered,
        ])
        .enumerate()
    {
        DESTROYED.set(0);
        let control = |operation| crate::product::control(descriptor, operation, Some(&capacity));

        assert_eq!(
            control(NativeProductHostOperation::FORM).state(),
            NativeProductHostState::OPEN
        );

        let product = std::ptr::from_ref(descriptor).addr();

        with_runtime(|runtime| {
            let (handle, _) = runtime
                .admit_returned_value(product, 8, 8, 0, None, cleanup(start))
                .unwrap();

            assert_eq!(
                runtime.resolve_returned_value(handle),
                NativeRuntimeStatus::RUNTIME_FAILURE
            );

            assert_eq!(
                control(NativeProductHostOperation::CLOSE).state(),
                NativeProductHostState::CLOSING
            );

            assert_eq!(
                control(NativeProductHostOperation::OBSERVE).active_entries(),
                1
            );

            assert_eq!(
                runtime.release_returned_value(handle),
                NativeRuntimeStatus::UNKNOWN_TASK
            );

            assert_eq!(
                runtime.destroy_task(handle),
                NativeRuntimeStatus::RUNTIME_FAILURE
            );

            assert_eq!(DESTROYED.get(), 0);

            // This fixture contains no actual value. Remove its deliberately retained failure outside locks.
            let retained = runtime.tasks.lock().unwrap().remove(&handle);
            drop(retained);
        })
        .unwrap();

        assert_eq!(
            control(NativeProductHostOperation::OBSERVE).state(),
            NativeProductHostState::CLOSED
        );

        assert_eq!(DESTROYED.get(), index);
    }

    assert!(shutdown().is_success());
}

#[test]
fn admitted_cleanup_dispatches_blocking_compute_and_main_lanes_without_allocation() {
    let capacity = crate::test_support::cleanup_capacity_binding();
    use std::sync::atomic::{AtomicUsize, Ordering};
    static SELECTED: AtomicUsize = AtomicUsize::new(0);
    static RESUMED: AtomicUsize = AtomicUsize::new(0);
    static RELEASED: AtomicUsize = AtomicUsize::new(0);

    static METADATA: [NativeFrameMetadata; 3] = [
        NativeFrameMetadata::new(
            [181; 32],
            1,
            8,
            8,
            0,
            1,
            crate::test_support::native_blocking_frame_state,
        ),
        NativeFrameMetadata::new(
            [182; 32],
            1,
            8,
            8,
            0,
            1,
            crate::test_support::native_compute_frame_state,
        ),
        NativeFrameMetadata::new(
            [183; 32],
            1,
            8,
            8,
            0,
            1,
            crate::test_support::native_main_frame_state,
        ),
    ];

    extern "C" fn metadata() -> Option<&'static NativeFrameMetadata> {
        Some(&METADATA[SELECTED.load(Ordering::SeqCst)])
    }

    extern "C-unwind" fn start(
        _: usize,
        output: &mut NativeInactiveFrame,
        _: &mut NativeBrayCallOutcome,
    ) -> NativeRuntimeStatus {
        *output = NativeInactiveFrame::new(FRAME.replace(0), adapter);

        NativeRuntimeStatus::SUCCESS
    }

    extern "C" fn adapter(context: usize, _: NativeFrameEntry) -> NativeProtectedFrame {
        NativeProtectedFrame::new(
            context,
            *metadata().unwrap(),
            resume,
            resume,
            ignore,
            resolve,
            complete,
            release,
        )
    }

    extern "C-unwind" fn resume(_: usize) -> NativeFrameProgress {
        let lane = crate::current_task_execution_context().unwrap().lane();

        match SELECTED.load(Ordering::SeqCst) {
            0 => assert_eq!(lane.workload(), crate::ExecutionWorkload::Blocking),
            1 => assert_eq!(lane.workload(), crate::ExecutionWorkload::Compute),
            _ => assert!(matches!(
                lane.placement(),
                crate::ExecutionLanePlacement::MainThread(_)
            )),
        }

        assert_eq!(RESUMED.fetch_add(1, Ordering::SeqCst), 0);

        NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 0)
    }

    extern "C-unwind" fn release(context: usize) {
        assert_eq!(RELEASED.fetch_add(1, Ordering::SeqCst), 0);
        crate::native::frames::bray_runtime_frame_storage_release(context);
    }

    assert!(initialize(NativeRuntimeConfiguration::new(1, 1)).is_success());

    let descriptor =
        NativeProductHostDescriptor::new(NativeProductIdentity::new([184; 32]), no_statics, 0);

    assert_eq!(
        crate::product::control(
            &descriptor,
            NativeProductHostOperation::FORM,
            Some(&capacity)
        )
        .state(),
        NativeProductHostState::OPEN
    );

    for selected in 0..3 {
        SELECTED.store(selected, Ordering::SeqCst);
        RESUMED.store(0, Ordering::SeqCst);
        RELEASED.store(0, Ordering::SeqCst);
        let frame = crate::native::frames::bray_runtime_frame_storage_admission(metadata());
        assert_ne!(frame, 0);
        FRAME.set(frame);

        let cleanup = NativeValueCleanup::new(
            NativeCleanupExecution::ASYNCHRONOUS,
            Some(metadata),
            start,
            crate::test_support::panic_callbacks(unexpected_panic, unexpected_panic),
        );

        with_runtime(|runtime| {
            let (handle, _) = runtime
                .admit_returned_value(
                    std::ptr::from_ref(&descriptor).addr(),
                    8,
                    8,
                    0,
                    None,
                    cleanup,
                )
                .unwrap();

            assert!(
                with_allocation_failure(|| runtime.resolve_returned_value(handle)).is_success()
            );
        })
        .unwrap();

        assert_eq!(RESUMED.load(Ordering::SeqCst), 1);
        assert_eq!(RELEASED.load(Ordering::SeqCst), 1);
    }

    assert_eq!(
        crate::product::control(
            &descriptor,
            NativeProductHostOperation::CLOSE,
            Some(&capacity)
        )
        .state(),
        NativeProductHostState::CLOSED
    );

    assert!(shutdown().is_success());
}
