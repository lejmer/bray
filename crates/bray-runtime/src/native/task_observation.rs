use std::alloc::{Layout, alloc_zeroed, dealloc};
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use bray_runtime_abi::{
    NativeFrameAffinity, NativeFrameExit, NativeFrameProgress, NativeFrameProgressKind,
    NativeFrameState, NativeInactiveFrame, NativeLaneRequirements, NativeProtectedFrame,
    NativeRunOutcome, NativeRunResultLayout, NativeRunState, NativeRuntimeStatus, NativeTaskHandle,
};

use super::state::{current_native_task, runtime_failure, with_runtime};
use crate::RuntimePanic;

const TASK_OBSERVATION_FRAME_IDENTITY: [u8; 32] = *b"bray.task.observation.frame.v1\0\0";

pub(super) type NativeValueCleanupCallback = extern "C-unwind" fn(*mut u8);

struct TaskObservation {
    task: NativeTaskHandle,
    layout: NativeRunResultLayout,
    cancellation: Option<NativeValueCleanupCallback>,
    lifecycle: Option<NativeValueCleanupCallback>,
    request_cancellation: bool,
    owner: AtomicU64,
    consumed: AtomicBool,
    outcome: Mutex<Option<NativeRunOutcome>>,
}

impl TaskObservation {
    fn resume(&self, context: usize) -> NativeFrameProgress {
        if self.request_cancellation
            && !with_runtime(|runtime| runtime.request_cancellation(self.task))
                .is_ok_and(NativeRuntimeStatus::is_success)
        {
            return failure_progress();
        }

        let Some(owner) = current_native_task() else {
            return failure_progress();
        };

        self.owner.store(owner.raw(), Ordering::Release);

        let outcome = with_runtime(|runtime| runtime.join(self.task, wake_observer, context))
            .unwrap_or_else(runtime_failure);

        if outcome.state() == NativeRunState::PENDING {
            return NativeFrameProgress::new(NativeFrameProgressKind::SUSPENDED, 0, 0);
        }

        if matches!(
            outcome.state(),
            NativeRunState::COMPLETED | NativeRunState::PANICKED | NativeRunState::CANCELLED
        ) {
            *self
                .outcome
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(outcome);

            return NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 0);
        }

        failure_progress()
    }

    fn move_completion(&self, destination: usize) {
        let outcome = self
            .outcome
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
            .unwrap_or_else(|| panic!("task observation must retain one terminal outcome"));

        transfer_outcome(outcome, destination, self.layout)
            .unwrap_or_else(|_| panic!("task observation result transfer failed"));

        destroy_task(self.task).unwrap_or_else(|_| panic!("observed task destruction failed"));

        self.consumed.store(true, Ordering::Release);
    }

    fn resolve_owned_task(&self) {
        if self.consumed.load(Ordering::Acquire) {
            return;
        }

        if self
            .outcome
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_none()
        {
            let _ = with_runtime(|runtime| runtime.request_cancellation(self.task));

            let outcome = with_runtime(|runtime| runtime.resolve_task(self.task))
                .unwrap_or_else(runtime_failure);

            *self
                .outcome
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(outcome);
        }

        let Some(outcome) = self
            .outcome
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        else {
            return;
        };

        let mut incident = None;

        match TemporaryRunResult::new(self.layout) {
            Ok(storage) => match transfer_outcome(outcome, storage.address(), self.layout) {
                Ok(()) => {
                    for cleanup in [self.cancellation, self.lifecycle].into_iter().flatten() {
                        if let Err(found) =
                            catch_unwind(AssertUnwindSafe(|| cleanup(storage.pointer())))
                        {
                            RuntimePanic::record(&mut incident, found);
                        }
                    }
                }
                Err(status) => RuntimePanic::record(&mut incident, Box::new(status)),
            },
            Err(status) => RuntimePanic::record(&mut incident, Box::new(status)),
        }

        if let Err(found) = destroy_task(self.task) {
            RuntimePanic::record(&mut incident, Box::new(found));
        }

        self.consumed.store(true, Ordering::Release);

        if let Some(incident) = incident {
            resume_unwind(Box::new(incident));
        }
    }
}

struct TemporaryRunResult {
    pointer: *mut u8,
    layout: Layout,
}

impl TemporaryRunResult {
    #[expect(
        unsafe_code,
        reason = "the validated compiler layout determines this temporary native allocation"
    )]
    fn new(result: NativeRunResultLayout) -> Result<Self, NativeRuntimeStatus> {
        let layout = Layout::from_size_align(result.size(), result.alignment())
            .map_err(|_| NativeRuntimeStatus::INVALID_ARGUMENT)?;

        let pointer = unsafe { alloc_zeroed(layout) };

        if pointer.is_null() {
            return Err(NativeRuntimeStatus::RUNTIME_FAILURE);
        }

        Ok(Self { pointer, layout })
    }

    const fn pointer(&self) -> *mut u8 {
        self.pointer
    }

    fn address(&self) -> usize {
        self.pointer.addr()
    }
}

impl Drop for TemporaryRunResult {
    #[expect(
        unsafe_code,
        reason = "the temporary is released with the exact layout used to allocate it"
    )]
    fn drop(&mut self) {
        unsafe { dealloc(self.pointer, self.layout) };
    }
}

pub(super) fn create(
    task: NativeTaskHandle,
    request_cancellation: bool,
    layout: NativeRunResultLayout,
    cancellation: Option<NativeValueCleanupCallback>,
    lifecycle: Option<NativeValueCleanupCallback>,
) -> Option<NativeInactiveFrame> {
    if !layout.is_valid() {
        return None;
    }

    let context = Box::new(TaskObservation {
        task,
        layout,
        cancellation,
        lifecycle,
        request_cancellation,
        owner: AtomicU64::new(0),
        consumed: AtomicBool::new(false),
        outcome: Mutex::new(None),
    });

    Some(NativeInactiveFrame::new(
        Box::into_raw(context).addr(),
        move_before_start,
    ))
}

#[expect(
    unsafe_code,
    reason = "the observation factory transfers this exact context allocation"
)]
extern "C" fn move_before_start(context: usize) -> NativeProtectedFrame {
    let observation = unsafe { &*(context as *const TaskObservation) };

    NativeProtectedFrame::new(
        context,
        TASK_OBSERVATION_FRAME_IDENTITY,
        1,
        size_of::<TaskObservation>(),
        align_of::<TaskObservation>(),
        observation.layout.size(),
        observation.layout.alignment(),
        state,
        resume,
        cancel,
        broadcast_tasks,
        resolve_lifecycle,
        move_completion,
        destroy,
    )
}

extern "C" fn state(_: usize, _: u32) -> NativeFrameState {
    NativeFrameState::new(NativeFrameAffinity::MOVABLE, NativeLaneRequirements::NONE)
}

extern "C-unwind" fn resume(context: usize) -> NativeFrameProgress {
    observation(context).resume(context)
}

extern "C-unwind" fn cancel(context: usize) -> NativeFrameProgress {
    let observation = observation(context);
    let _ = with_runtime(|runtime| runtime.request_cancellation(observation.task));

    observation.resume(context)
}

extern "C-unwind" fn broadcast_tasks(context: usize) {
    let observation = observation(context);
    let _ = with_runtime(|runtime| runtime.request_cancellation(observation.task));
}

extern "C-unwind" fn resolve_lifecycle(context: usize, _: NativeFrameExit) {
    observation(context).resolve_owned_task();
}

extern "C-unwind" fn move_completion(context: usize, destination: usize) {
    observation(context).move_completion(destination);
}

#[expect(
    unsafe_code,
    reason = "protected-frame destruction consumes the exact transferred observation allocation"
)]
extern "C-unwind" fn destroy(context: usize) {
    let observation = unsafe { Box::from_raw(context as *mut TaskObservation) };

    observation.resolve_owned_task();
}

extern "C" fn wake_observer(context: usize) {
    let observation = observation(context);

    let Some(owner) = NativeTaskHandle::new(observation.owner.load(Ordering::Acquire)) else {
        return;
    };

    let _ = with_runtime(|runtime| runtime.wake(owner, 0));
}

#[expect(
    unsafe_code,
    reason = "native frame callbacks borrow the live transferred observation context"
)]
fn observation(context: usize) -> &'static TaskObservation {
    unsafe { &*(context as *const TaskObservation) }
}

fn destroy_task(task: NativeTaskHandle) -> Result<(), NativeRuntimeStatus> {
    let status = with_runtime(|runtime| runtime.destroy_task(task))?;

    if status.is_success() {
        Ok(())
    } else {
        Err(status)
    }
}

#[expect(
    unsafe_code,
    reason = "the compiler-provided validated layout governs the exact terminal payload move"
)]
pub(super) fn transfer_outcome(
    outcome: NativeRunOutcome,
    destination: usize,
    layout: NativeRunResultLayout,
) -> Result<(), NativeRuntimeStatus> {
    if destination == 0 || !layout.is_valid() {
        return Err(NativeRuntimeStatus::INVALID_ARGUMENT);
    }

    let destination = destination as *mut u8;

    unsafe { destination.write_bytes(0, layout.size()) };

    let panic_payload = outcome.payload();

    let (tag, payload) = if outcome.state() == NativeRunState::COMPLETED {
        (
            layout.completed_tag(),
            Some((
                layout.completed_offset(),
                outcome.payload(),
                layout.completed_size(),
            )),
        )
    } else if outcome.state() == NativeRunState::PANICKED {
        (
            layout.panicked_tag(),
            Some((
                layout.panicked_offset(),
                (&raw const panic_payload).addr(),
                size_of::<usize>(),
            )),
        )
    } else if outcome.state() == NativeRunState::CANCELLED {
        (layout.cancelled_tag(), None)
    } else if outcome.state() == NativeRunState::PENDING {
        return Err(NativeRuntimeStatus::PENDING);
    } else {
        return Err(NativeRuntimeStatus::RUNTIME_FAILURE);
    };

    unsafe {
        match layout.tag_size() {
            1 => destination.cast::<u8>().write(tag as u8),
            2 => destination.cast::<u16>().write_unaligned(tag as u16),
            4 => destination.cast::<u32>().write_unaligned(tag as u32),
            8 => destination.cast::<u64>().write_unaligned(tag),
            _ => return Err(NativeRuntimeStatus::INVALID_ARGUMENT),
        }

        if let Some((offset, source, size)) = payload
            && size != 0
        {
            if source == 0 {
                return Err(NativeRuntimeStatus::INVALID_ARGUMENT);
            }

            std::ptr::copy_nonoverlapping(source as *const u8, destination.add(offset), size);
        }
    }

    Ok(())
}

fn failure_progress() -> NativeFrameProgress {
    NativeFrameProgress::new(NativeFrameProgressKind::RUNTIME_FAILURE, 0, 0)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::panic::{AssertUnwindSafe, catch_unwind, panic_any};
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

    use bray_runtime_abi::{
        NativeRunOutcome, NativeRunResultLayout, NativeRunState, NativeRuntimeStatus,
    };

    use super::{TaskObservation, transfer_outcome};

    thread_local! {
        static CLEANUP_EVENTS: RefCell<Vec<(&'static str, u8)>> = const { RefCell::new(Vec::new()) };
    }

    struct CleanupFailure(u8);

    impl Drop for CleanupFailure {
        fn drop(&mut self) {
            CLEANUP_EVENTS.with_borrow_mut(|events| events.push(("release", self.0)));
        }
    }

    extern "C-unwind" fn failing_cancellation(_: *mut u8) {
        CLEANUP_EVENTS.with_borrow_mut(|events| events.push(("cleanup", 1)));
        panic_any(CleanupFailure(1));
    }

    extern "C-unwind" fn failing_lifecycle(_: *mut u8) {
        CLEANUP_EVENTS.with_borrow_mut(|events| events.push(("cleanup", 2)));
        panic_any(CleanupFailure(2));
    }

    #[test]
    fn cleanup_retains_secondary_failures_until_the_original_is_disposed() {
        let completed = 42_u64;

        let observation = TaskObservation {
            task: bray_runtime_abi::NativeTaskHandle::new(1).unwrap(),
            layout: result_layout(),
            cancellation: Some(failing_cancellation),
            lifecycle: Some(failing_lifecycle),
            request_cancellation: false,
            owner: AtomicU64::new(0),
            consumed: AtomicBool::new(false),
            outcome: Mutex::new(Some(NativeRunOutcome::new(
                NativeRunState::COMPLETED,
                (&raw const completed).addr(),
            ))),
        };

        let failure =
            catch_unwind(AssertUnwindSafe(|| observation.resolve_owned_task())).unwrap_err();

        let failure = failure.downcast::<crate::RuntimePanic>().unwrap();

        assert!(failure.primary_is::<CleanupFailure>());
        assert_eq!(failure.suppressed_count(), 2);
        assert!(observation.consumed.load(Ordering::Acquire));
        CLEANUP_EVENTS.with_borrow(|events| assert_eq!(*events, [("cleanup", 1), ("cleanup", 2)]));

        drop(failure);
        observation.resolve_owned_task();

        CLEANUP_EVENTS.with_borrow(|events| {
            assert_eq!(
                *events,
                [
                    ("cleanup", 1),
                    ("cleanup", 2),
                    ("release", 1),
                    ("release", 2),
                ]
            )
        });
    }

    const COMPLETED_TAG: u8 = 3;
    const PANICKED_TAG: u8 = 5;
    const CANCELLED_TAG: u8 = 7;

    #[repr(C, align(8))]
    struct ResultStorage([u8; 16]);

    #[test]
    fn terminal_outcomes_form_their_selected_run_result_variants() {
        let completed = 42_u64;

        let cases = [
            (
                NativeRunOutcome::new(NativeRunState::COMPLETED, (&raw const completed).addr()),
                COMPLETED_TAG,
                completed,
            ),
            (
                NativeRunOutcome::new(NativeRunState::PANICKED, 0x1234),
                PANICKED_TAG,
                0x1234,
            ),
            (
                NativeRunOutcome::new(NativeRunState::CANCELLED, 0),
                CANCELLED_TAG,
                0,
            ),
        ];

        for (outcome, expected_tag, expected_payload) in cases {
            let mut storage = ResultStorage([0xff; 16]);

            assert_eq!(
                transfer_outcome(outcome, storage.0.as_mut_ptr().addr(), result_layout()),
                Ok(())
            );

            assert_eq!(storage.0[0], expected_tag);

            assert_eq!(
                u64::from_ne_bytes(
                    storage.0[8..]
                        .try_into()
                        .unwrap_or_else(|_| panic!("payload must occupy eight bytes"))
                ),
                expected_payload
            );
        }
    }

    #[test]
    fn unresolved_and_failed_outcomes_keep_their_runtime_status() {
        let mut storage = ResultStorage([0xff; 16]);

        assert_eq!(
            transfer_outcome(
                NativeRunOutcome::new(NativeRunState::PENDING, 0),
                storage.0.as_mut_ptr().addr(),
                result_layout(),
            ),
            Err(NativeRuntimeStatus::PENDING)
        );

        assert_eq!(
            transfer_outcome(
                NativeRunOutcome::new(NativeRunState::RUNTIME_FAILURE, 0),
                storage.0.as_mut_ptr().addr(),
                result_layout(),
            ),
            Err(NativeRuntimeStatus::RUNTIME_FAILURE)
        );
    }

    const fn result_layout() -> NativeRunResultLayout {
        NativeRunResultLayout::new(
            16,
            8,
            1,
            COMPLETED_TAG as u64,
            8,
            8,
            8,
            PANICKED_TAG as u64,
            8,
            CANCELLED_TAG as u64,
        )
    }
}
