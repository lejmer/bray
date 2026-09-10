use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, OnceLock, Weak};

use bray_runtime_abi::{
    NativeFrameEntry, NativeFrameMetadata, NativeProtectedFrame, NativeRuntimeStatus,
};

use super::state::{NativeRuntimeCore, NativeTaskReservation};
use super::storage::NativeStorage;

// Inactive generated frames can move between product runtimes and native threads. This runtime artifact's
// registry owns their opaque native storage until generated destruction releases it. Runtime
// membership is weak, so retaining an inactive value does not keep an execution runtime alive.
static FRAMES: OnceLock<Mutex<FrameRegistry>> = OnceLock::new();

const TASK_ENTRIES: [NativeFrameEntry; 3] = [
    NativeFrameEntry::Body,
    NativeFrameEntry::CaptureCleanup,
    NativeFrameEntry::CaptureQuiescence,
];

#[derive(Default)]
struct FrameRegistry {
    frames: HashMap<usize, FrameStorage>,
    runtimes: Vec<Weak<NativeRuntimeCore>>,
    tasks: usize,
    lanes: usize,
}

struct FrameStorage {
    _bytes: NativeStorage,
    metadata: NativeFrameMetadata,
    tasks: [Option<NativeTaskReservation>; 3],
    lanes: usize,
}

pub(super) struct FrameTaskClaim {
    owner: Option<(usize, NativeFrameEntry)>,
    reservation: Option<NativeTaskReservation>,
}

fn registry() -> &'static Mutex<FrameRegistry> {
    FRAMES.get_or_init(|| Mutex::new(FrameRegistry::default()))
}

/// Reserves a generated context before any captures transfer into it.
pub extern "C" fn bray_runtime_frame_storage_admission(
    metadata: Option<&NativeFrameMetadata>,
) -> usize {
    std::panic::catch_unwind(|| {
        metadata
            .and_then(|metadata| admit(*metadata).ok())
            .unwrap_or(0)
    })
    .unwrap_or(0)
}

/// Releases a resolved context and all remaining unused task reservations.
pub extern "C" fn bray_runtime_frame_storage_release(context: usize) {
    release(context);
}

pub(super) fn admit(metadata: NativeFrameMetadata) -> Result<usize, NativeRuntimeStatus> {
    let bytes = NativeStorage::new(metadata.size().max(1), metadata.alignment())?;
    let address = bytes.address();
    let mut tasks = std::array::from_fn(|_| None);

    for (slot, entry) in tasks.iter_mut().zip(TASK_ENTRIES) {
        *slot = Some(NativeTaskReservation::prepare(
            &metadata.for_entry(entry),
            crate::task::TaskAdmissionKind::Continuation,
        )?);
    }

    let lanes = usize::try_from(metadata.state_count())
        .ok()
        .and_then(|states| states.checked_mul(TASK_ENTRIES.len()))
        .ok_or(NativeRuntimeStatus::ALLOCATION_FAILURE)?;

    let storage = FrameStorage {
        _bytes: bytes,
        metadata,
        tasks,
        lanes,
    };

    let mut retained = Vec::new();

    let mut state = registry()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let result = (|| {
        let tasks = state
            .tasks
            .checked_add(TASK_ENTRIES.len())
            .ok_or(NativeRuntimeStatus::ALLOCATION_FAILURE)?;

        let lanes = state
            .lanes
            .checked_add(lanes)
            .ok_or(NativeRuntimeStatus::ALLOCATION_FAILURE)?;

        state.runtimes.retain(|runtime| runtime.strong_count() != 0);

        crate::allocation::reserve_vec_entries(&mut retained, state.runtimes.len())
            .map_err(|_| NativeRuntimeStatus::ALLOCATION_FAILURE)?;

        crate::allocation::reserve_map_entries(&mut state.frames, 1)
            .map_err(|_| NativeRuntimeStatus::ALLOCATION_FAILURE)?;

        for runtime in &state.runtimes {
            if let Some(runtime) = runtime.upgrade() {
                retained.push(runtime);
            }
        }

        for runtime in &retained {
            reserve_runtime(runtime, tasks, lanes)?;
        }

        let mut storage = storage;

        for task in storage.tasks.iter_mut().flatten() {
            task.admit_cleanup();
        }

        assert!(
            !state.frames.contains_key(&address),
            "live frame allocations must have distinct addresses"
        );

        state.frames.insert(address, storage);
        state.tasks = tasks;
        state.lanes = lanes;

        Ok(address)
    })();

    // A last runtime reference can release workers or callbacks. Never drop it under the registry.
    drop(state);
    drop(retained);

    result
}

pub(super) fn register_runtime(
    runtime: &Arc<NativeRuntimeCore>,
) -> Result<(), NativeRuntimeStatus> {
    let mut state = registry()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    state.runtimes.retain(|runtime| runtime.strong_count() != 0);

    crate::allocation::reserve_vec_entries(&mut state.runtimes, 1)
        .map_err(|_| NativeRuntimeStatus::ALLOCATION_FAILURE)?;

    reserve_runtime(runtime, state.tasks, state.lanes)?;
    state.runtimes.push(Arc::downgrade(runtime));

    Ok(())
}

fn reserve_runtime(
    runtime: &NativeRuntimeCore,
    tasks: usize,
    lanes: usize,
) -> Result<(), NativeRuntimeStatus> {
    // Native task publication may acquire the scheduler lock while holding the task table.
    // Admission follows the same order, with the frame registry outermost.
    let mut table = runtime
        .tasks
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let tasks = tasks.max(runtime.cleanup_task_capacity.load(Ordering::Relaxed));
    let count = u64::try_from(tasks).map_err(|_| NativeRuntimeStatus::ALLOCATION_FAILURE)?;

    runtime
        .next_task
        .load(Ordering::Relaxed)
        .checked_add(count)
        .ok_or(NativeRuntimeStatus::RUNTIME_FAILURE)?;

    crate::allocation::reserve_map_entries(&mut table, tasks)
        .map_err(|_| NativeRuntimeStatus::ALLOCATION_FAILURE)?;

    runtime
        .scheduler
        .reserve_cleanup_capacity(tasks, lanes)
        .map_err(|error| match error {
            crate::SchedulerError::AdmissionAllocation(_) => {
                NativeRuntimeStatus::ALLOCATION_FAILURE
            }
            _ => NativeRuntimeStatus::RUNTIME_FAILURE,
        })?;

    runtime
        .cleanup_task_capacity
        .store(tasks, Ordering::Relaxed);

    Ok(())
}

pub(super) fn release(address: usize) {
    let storage = {
        let mut state = registry()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let storage = state.frames.remove(&address);

        if let Some(storage) = &storage {
            state.tasks -= TASK_ENTRIES.len();
            state.lanes -= storage.lanes;
        }

        storage
    };

    drop(storage);
}

pub(super) fn is_admitted(address: usize) -> bool {
    registry()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .frames
        .contains_key(&address)
}

pub(super) fn claim(
    address: usize,
    entry: NativeFrameEntry,
    metadata: &NativeFrameMetadata,
) -> Result<Option<FrameTaskClaim>, NativeRuntimeStatus> {
    let mut state = registry()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let Some(storage) = state.frames.get_mut(&address) else {
        return Ok(None);
    };

    let expected = storage.metadata.for_entry(entry);

    if expected.identity() != metadata.identity()
        || expected.state_count() != metadata.state_count()
        || expected.size() != metadata.size()
        || expected.alignment() != metadata.alignment()
        || expected.completion_size() != metadata.completion_size()
        || expected.completion_alignment() != metadata.completion_alignment()
        || !std::ptr::fn_addr_eq(expected.state(), metadata.state())
    {
        return Err(NativeRuntimeStatus::INVALID_ARGUMENT);
    }

    let reservation = storage
        .tasks
        .get_mut(usize::from(entry.code()))
        .and_then(Option::take)
        .ok_or(NativeRuntimeStatus::INVALID_ARGUMENT)?;

    Ok(Some(FrameTaskClaim {
        owner: Some((address, entry)),
        reservation: Some(reservation),
    }))
}

impl FrameTaskClaim {
    pub(super) fn fresh(reservation: NativeTaskReservation) -> Self {
        Self {
            owner: None,
            reservation: Some(reservation),
        }
    }

    pub(super) fn reservation(&mut self) -> &mut NativeTaskReservation {
        self.reservation
            .as_mut()
            .unwrap_or_else(|| unreachable!("frame task claim must retain its reservation"))
    }

    pub(super) fn install(
        mut self,
        frame: NativeProtectedFrame,
    ) -> triomphe::Arc<super::state::StartedTask> {
        let reservation = self
            .reservation
            .take()
            .unwrap_or_else(|| unreachable!("frame task claim must install once"));

        self.owner = None;

        reservation.install(frame)
    }
}

impl Drop for FrameTaskClaim {
    fn drop(&mut self) {
        let Some((address, entry)) = self.owner else {
            return;
        };

        let Some(reservation) = self.reservation.take() else {
            return;
        };

        let mut state = registry()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let storage = state
            .frames
            .get_mut(&address)
            .unwrap_or_else(|| unreachable!("rejected admission must retain its frame owner"));

        let slot = &mut storage.tasks[usize::from(entry.code())];

        assert!(
            slot.is_none(),
            "frame task reservation must return to its own vacant slot"
        );

        *slot = Some(reservation);
    }
}

#[cfg(test)]
mod tests {
    use crate::native::frame::NativeFrameTransfer;
    use crate::native::state::{initialize, shutdown, test_runtime_isolation, with_runtime};
    use crate::test_support::{
        native_origin_frame_state, with_allocation_failure, with_allocation_failure_after,
    };
    use bray_runtime_abi::{
        NativeFrameEntry, NativeFrameExit, NativeFrameMetadata, NativeFrameProgress,
        NativeFrameProgressKind, NativeInactiveFrame, NativeProtectedFrame, NativeRootHandle,
        NativeRunState, NativeRuntimeConfiguration, NativeRuntimeStatus,
    };
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static BODIES: AtomicUsize = AtomicUsize::new(0);
    static QUIESCENCE: AtomicUsize = AtomicUsize::new(0);
    static CLEANUP: AtomicUsize = AtomicUsize::new(0);

    fn metadata() -> NativeFrameMetadata {
        NativeFrameMetadata::new([173; 32], 2, 256, 64, 0, 1, native_origin_frame_state)
    }

    extern "C" fn adapter(context: usize, entry: NativeFrameEntry) -> NativeProtectedFrame {
        let resume = match entry {
            NativeFrameEntry::Body => body,
            NativeFrameEntry::CaptureCleanup => cleanup,
            NativeFrameEntry::CaptureQuiescence => quiesce,
            NativeFrameEntry::CaptureDestruction => destroy_captures,
        };

        NativeProtectedFrame::new(
            context,
            metadata(),
            resume,
            cleanup,
            action,
            resolve,
            move_completion,
            destroy,
        )
    }

    extern "C-unwind" fn body(_: usize) -> NativeFrameProgress {
        BODIES.fetch_add(1, Ordering::Relaxed);

        NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 0)
    }

    extern "C-unwind" fn quiesce(_: usize) -> NativeFrameProgress {
        let visit = QUIESCENCE.fetch_add(1, Ordering::Relaxed);

        let progress = if visit % 2 == 0 {
            NativeFrameProgressKind::YIELDED
        } else {
            NativeFrameProgressKind::COMPLETED
        };

        NativeFrameProgress::new(progress, 1, 0)
    }

    extern "C-unwind" fn cleanup(_: usize) -> NativeFrameProgress {
        let visit = CLEANUP.fetch_add(1, Ordering::Relaxed);

        let progress = if visit % 2 == 0 {
            NativeFrameProgressKind::YIELDED
        } else {
            NativeFrameProgressKind::CANCELLED
        };

        NativeFrameProgress::new(progress, 1, 0)
    }

    extern "C-unwind" fn destroy_captures(_: usize) -> NativeFrameProgress {
        NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 0)
    }

    extern "C-unwind" fn action(_: usize) {}
    extern "C-unwind" fn resolve(_: usize, _: NativeFrameExit) {}
    extern "C-unwind" fn move_completion(_: usize, _: usize) {}
    extern "C-unwind" fn destroy(address: usize) {
        super::release(address);
    }

    fn run_entry(address: usize, entry: NativeFrameEntry, expected: NativeRunState) {
        let mut transfer =
            NativeFrameTransfer::from_inactive(NativeInactiveFrame::new(address, adapter), entry);

        with_runtime(|runtime| {
            let allocation = runtime.allocate_frame_continuation(&transfer);
            assert_eq!(allocation.status(), NativeRuntimeStatus::SUCCESS);
            let task = allocation.task().unwrap();

            assert_eq!(
                runtime.start(task, &mut transfer, None),
                NativeRuntimeStatus::SUCCESS
            );

            let root = NativeRootHandle::new(task.raw()).unwrap();
            assert_eq!(runtime.observe_root(root).state(), expected);

            assert_eq!(
                runtime.resolve_root_completion(root),
                NativeRuntimeStatus::SUCCESS
            );
        })
        .unwrap();
    }

    #[test]
    fn incompatible_metadata_does_not_consume_the_reserved_entry() {
        let _isolation = test_runtime_isolation();
        let address = super::admit(metadata()).unwrap();

        let invalid = [
            NativeFrameMetadata::new([174; 32], 2, 256, 64, 0, 1, native_origin_frame_state),
            NativeFrameMetadata::new([173; 32], 1, 256, 64, 0, 1, native_origin_frame_state),
            NativeFrameMetadata::new([173; 32], 2, 512, 64, 0, 1, native_origin_frame_state),
            NativeFrameMetadata::new([173; 32], 2, 256, 32, 0, 1, native_origin_frame_state),
            NativeFrameMetadata::new([173; 32], 2, 256, 64, 8, 1, native_origin_frame_state),
            NativeFrameMetadata::new([173; 32], 2, 256, 64, 0, 8, native_origin_frame_state),
        ];

        with_allocation_failure(|| {
            for incoming in invalid {
                assert!(matches!(
                    super::claim(address, NativeFrameEntry::Body, &incoming),
                    Err(NativeRuntimeStatus::INVALID_ARGUMENT)
                ));

                let claim = super::claim(address, NativeFrameEntry::Body, &metadata()).unwrap();
                assert!(claim.is_some());
                drop(claim);
            }

            super::release(address);
        });
    }

    #[test]
    fn failed_owner_admission_preserves_existing_storage_at_every_allocation() {
        let _isolation = test_runtime_isolation();
        let existing = super::admit(metadata()).unwrap();
        let mut completed = false;

        for allowed in 0..256 {
            assert_eq!(
                initialize(NativeRuntimeConfiguration::new(2, 1)),
                NativeRuntimeStatus::SUCCESS
            );

            let result = with_allocation_failure_after(allowed, || super::admit(metadata()));
            assert!(super::is_admitted(existing));

            match result {
                Ok(address) => {
                    assert_ne!(address, existing);
                    assert_eq!(address % 64, 0);
                    with_allocation_failure(|| super::release(address));
                    completed = true;
                }
                Err(status) => {
                    assert_eq!(status, NativeRuntimeStatus::ALLOCATION_FAILURE);
                    assert_eq!(super::registry().lock().unwrap().frames.len(), 1);
                }
            }

            assert_eq!(shutdown(), NativeRuntimeStatus::SUCCESS);

            if completed {
                break;
            }
        }

        with_allocation_failure(|| super::release(existing));
        assert!(completed, "allocation sweep must reach complete admission");
        assert!(super::registry().lock().unwrap().frames.is_empty());
    }

    #[test]
    fn owner_capacity_survives_runtime_replacement_and_ordinary_task_pressure() {
        let _isolation = test_runtime_isolation();
        BODIES.store(0, Ordering::Relaxed);
        QUIESCENCE.store(0, Ordering::Relaxed);
        CLEANUP.store(0, Ordering::Relaxed);

        assert_eq!(
            initialize(NativeRuntimeConfiguration::new(2, 1)),
            NativeRuntimeStatus::SUCCESS
        );

        let previous = with_runtime(|runtime| Arc::downgrade(&runtime.core)).unwrap();
        let owners: Vec<_> = (0..4).map(|_| super::admit(metadata()).unwrap()).collect();
        assert_eq!(shutdown(), NativeRuntimeStatus::SUCCESS);
        assert!(previous.upgrade().is_none());

        assert_eq!(
            initialize(NativeRuntimeConfiguration::new(2, 1)),
            NativeRuntimeStatus::SUCCESS
        );

        let ordinary: Vec<_> = with_runtime(|runtime| {
            (0..2)
                .map(|_| {
                    let task = runtime.allocate().task().unwrap();
                    let mut transfer = NativeFrameTransfer::new(adapter(0, NativeFrameEntry::Body));

                    assert_eq!(
                        runtime.start(task, &mut transfer, None),
                        NativeRuntimeStatus::SUCCESS
                    );

                    task
                })
                .collect()
        })
        .unwrap();

        with_allocation_failure(|| {
            assert!(with_runtime(|runtime| runtime.allocate().task().is_none()).unwrap());

            for (index, address) in owners.into_iter().enumerate() {
                run_entry(
                    address,
                    NativeFrameEntry::CaptureQuiescence,
                    NativeRunState::COMPLETED,
                );

                assert!(super::is_admitted(address));

                if index % 2 == 0 {
                    run_entry(
                        address,
                        NativeFrameEntry::CaptureCleanup,
                        NativeRunState::CANCELLED,
                    );
                } else {
                    let outcome =
                        crate::native::implementation::bray_runtime_inactive_capture_destruction(
                            NativeInactiveFrame::new(address, adapter),
                        );

                    assert_eq!(
                        outcome,
                        bray_runtime_abi::NativeBrayCallOutcome::completed().raw()
                    );
                }

                assert!(!super::is_admitted(address));
            }

            with_runtime(|runtime| {
                for task in ordinary {
                    let root = NativeRootHandle::new(task.raw()).unwrap();

                    assert_eq!(
                        runtime.observe_root(root).state(),
                        NativeRunState::COMPLETED
                    );

                    assert_eq!(
                        runtime.resolve_root_completion(root),
                        NativeRuntimeStatus::SUCCESS
                    );
                }

                assert_eq!(runtime.scheduler.task_count().unwrap(), 0);
            })
            .unwrap();
        });

        assert_eq!(BODIES.load(Ordering::Relaxed), 2);
        assert_eq!(QUIESCENCE.load(Ordering::Relaxed), 8);
        assert_eq!(CLEANUP.load(Ordering::Relaxed), 4);
        assert_eq!(shutdown(), NativeRuntimeStatus::SUCCESS);
    }

    #[test]
    fn rejected_start_returns_the_reserved_body_for_retry() {
        let _isolation = test_runtime_isolation();

        assert_eq!(
            initialize(NativeRuntimeConfiguration::new(1, 1)),
            NativeRuntimeStatus::SUCCESS
        );

        let address = super::admit(metadata()).unwrap();

        with_runtime(|runtime| {
            let blocker =
                crate::TaskControlBlock::start(crate::test_support::TestFrame::completing(1))
                    .unwrap();

            let registration = crate::test_support::register_task(
                &runtime.scheduler,
                &blocker,
                runtime.thread.runtime().id(),
            );

            let task = runtime.allocate().task().unwrap();

            let mut transfer =
                NativeFrameTransfer::borrowed(adapter(address, NativeFrameEntry::Body));

            assert_eq!(
                with_allocation_failure(|| runtime.start(task, &mut transfer, None)),
                NativeRuntimeStatus::RUNTIME_FAILURE
            );

            assert!(super::is_admitted(address));
            drop(registration);
            let task = runtime.allocate().task().unwrap();

            with_allocation_failure(|| {
                assert_eq!(
                    runtime.start(task, &mut transfer, None),
                    NativeRuntimeStatus::SUCCESS
                );

                let root = NativeRootHandle::new(task.raw()).unwrap();

                assert_eq!(
                    runtime.observe_root(root).state(),
                    NativeRunState::COMPLETED
                );

                assert_eq!(
                    runtime.resolve_root_completion(root),
                    NativeRuntimeStatus::SUCCESS
                );
            });
        })
        .unwrap();

        assert!(!super::is_admitted(address));
        assert_eq!(shutdown(), NativeRuntimeStatus::SUCCESS);
    }

    #[test]
    fn admission_covers_both_live_runtimes_and_cleanup_can_use_the_retained_one() {
        let _isolation = test_runtime_isolation();
        CLEANUP.store(0, Ordering::Relaxed);

        assert_eq!(
            initialize(NativeRuntimeConfiguration::new(2, 1)),
            NativeRuntimeStatus::SUCCESS
        );

        let retained = crate::native::retain_runtime().unwrap();
        assert_eq!(shutdown(), NativeRuntimeStatus::SUCCESS);

        assert_eq!(
            initialize(NativeRuntimeConfiguration::new(2, 1)),
            NativeRuntimeStatus::SUCCESS
        );

        let address = super::admit(metadata()).unwrap();
        assert!(retained.core.cleanup_task_capacity.load(Ordering::Relaxed) >= 3);

        with_runtime(|runtime| {
            assert!(!Arc::ptr_eq(&runtime.core, &retained.core));
            assert!(runtime.cleanup_task_capacity.load(Ordering::Relaxed) >= 3);
        })
        .unwrap();

        assert_eq!(shutdown(), NativeRuntimeStatus::SUCCESS);

        crate::native::state::with_cleanup_runtime(Some(&retained), || {
            with_allocation_failure(|| {
                run_entry(
                    address,
                    NativeFrameEntry::CaptureCleanup,
                    NativeRunState::CANCELLED,
                )
            });
        })
        .unwrap();

        assert!(!super::is_admitted(address));
        retained.release();
    }

    #[test]
    fn concurrent_owner_admission_and_release_do_not_deadlock_task_publication() {
        let _isolation = test_runtime_isolation();

        assert_eq!(
            initialize(NativeRuntimeConfiguration::new(2, 1)),
            NativeRuntimeStatus::SUCCESS
        );

        let barrier = Arc::new(std::sync::Barrier::new(5));

        let threads: Vec<_> = (0..4)
            .map(|_| {
                let barrier = Arc::clone(&barrier);

                std::thread::spawn(move || {
                    barrier.wait();

                    for _ in 0..32 {
                        let address = super::admit(metadata()).unwrap();
                        assert_eq!(address % 64, 0);
                        std::thread::yield_now();
                        super::release(address);
                    }
                })
            })
            .collect();

        barrier.wait();

        with_runtime(|runtime| {
            for _ in 0..64 {
                let task = runtime.allocate().task().unwrap();
                let mut frame = NativeFrameTransfer::new(adapter(0, NativeFrameEntry::Body));

                assert_eq!(
                    runtime.start(task, &mut frame, None),
                    NativeRuntimeStatus::SUCCESS
                );

                let root = NativeRootHandle::new(task.raw()).unwrap();

                assert_eq!(
                    runtime.observe_root(root).state(),
                    NativeRunState::COMPLETED
                );

                assert_eq!(
                    runtime.resolve_root_completion(root),
                    NativeRuntimeStatus::SUCCESS
                );
            }
        })
        .unwrap();

        for thread in threads {
            thread.join().unwrap();
        }

        let state = super::registry().lock().unwrap();
        assert!(state.frames.is_empty());
        assert_eq!((state.tasks, state.lanes), (0, 0));
        drop(state);
        assert_eq!(shutdown(), NativeRuntimeStatus::SUCCESS);
    }
}
