use bray_runtime_abi::{NativeFrameAffinity, NativeFrameState, NativeLaneRequirements};

use std::collections::VecDeque;
use std::num::NonZeroUsize;
use std::pin::Pin;
use std::sync::{Arc, Barrier};

use bray_platform::RuntimeThreadId;
use bray_runtime_model::{
    ExecutionLaneRequirement, ProtectedAsyncFrameId, ProtectedFrameAbiVersions,
    ProtectedFrameAffinity, ProtectedFrameDependencyId, ProtectedFrameDescriptor,
    ProtectedFrameLayout, ProtectedFrameStateDescriptor, ProtectedFrameStateId,
    ProtectedFrameStorageId, RuntimeAbiVersion,
};

use crate::{
    FrameContext, FrameExit, FrameProgress, FrameSuspension, ProtectedFrame, Scheduler,
    TaskControlBlock, TaskRegistration, current_task_execution_context,
};

thread_local! {
    static FAIL_ALLOCATION_AFTER: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}

pub(crate) fn allocation_should_fail() -> bool {
    match FAIL_ALLOCATION_AFTER.get() {
        None => false,
        Some(0) => true,
        Some(remaining) => {
            FAIL_ALLOCATION_AFTER.set(Some(remaining - 1));

            false
        }
    }
}

pub(crate) fn with_allocation_failure<T>(callback: impl FnOnce() -> T) -> T {
    with_allocation_failure_after(0, callback)
}

pub(crate) fn with_allocation_failure_after<T>(
    successful: usize,
    callback: impl FnOnce() -> T,
) -> T {
    struct Restore(Option<usize>);

    impl Drop for Restore {
        fn drop(&mut self) {
            FAIL_ALLOCATION_AFTER.set(self.0);
        }
    }

    let _restore = Restore(FAIL_ALLOCATION_AFTER.replace(Some(successful)));

    callback()
}

pub(crate) const fn panic_callbacks(
    report: extern "C-unwind" fn(usize) -> bray_runtime_abi::NativeRuntimeStatus,
    destroy: extern "C-unwind" fn(usize) -> bray_runtime_abi::NativeRuntimeStatus,
) -> bray_runtime_abi::NativePanicReportCallbacks {
    extern "C" fn unexpected_construction(_: &bray_runtime_abi::NativeCleanupIncident) -> usize {
        panic!("this fixture must not construct a cleanup report")
    }

    extern "C" fn unexpected_suppression(_: usize, _: usize) -> usize {
        panic!("this fixture must not combine panic reports")
    }

    bray_runtime_abi::NativePanicReportCallbacks::new(
        report,
        destroy,
        unexpected_construction,
        unexpected_suppression,
    )
}

thread_local! {
    static TRANSFERRED_OUTCOME: std::cell::Cell<Option<(usize, bray_runtime_abi::NativeRunOutcome)>> = const { std::cell::Cell::new(None) };
}

pub(crate) extern "C" fn record_run_result_transfer(
    destination: usize,
    outcome: &bray_runtime_abi::NativeRunOutcome,
) -> bray_runtime_abi::NativeRuntimeStatus {
    if outcome.state() == bray_runtime_abi::NativeRunState::COMPLETED && outcome.payload() == 0 {
        return bray_runtime_abi::NativeRuntimeStatus::INVALID_ARGUMENT;
    }

    TRANSFERRED_OUTCOME.set(Some((destination, *outcome)));

    bray_runtime_abi::NativeRuntimeStatus::SUCCESS
}

pub(crate) fn take_run_result_transfer() -> Option<(usize, bray_runtime_abi::NativeRunOutcome)> {
    TRANSFERRED_OUTCOME.take()
}

/// Retains fixture values across native callbacks and transfers each token exactly once.
pub(crate) struct NativeTestValues<T> {
    values: std::sync::Mutex<(usize, std::collections::BTreeMap<usize, T>)>,
}

impl<T> NativeTestValues<T> {
    pub(crate) const fn new() -> Self {
        // Fixture report tokens share the ABI word with completion and cancellation.
        Self {
            values: std::sync::Mutex::new((
                bray_runtime_abi::NativeBrayCallOutcome::cancelled().raw(),
                std::collections::BTreeMap::new(),
            )),
        }
    }

    pub(crate) fn insert(&self, value: T) -> usize {
        let mut values = self
            .values
            .lock()
            .expect("fixture registry must remain available");

        let token = values
            .0
            .checked_add(1)
            .expect("fixture token capacity must suffice");

        values.0 = token;
        values.1.insert(token, value);

        token
    }

    pub(crate) fn take(&self, token: usize) -> Option<T> {
        self.values
            .lock()
            .expect("fixture registry must remain available")
            .1
            .remove(&token)
    }
}

#[test]
fn native_fixture_tokens_transfer_ownership_once_without_reuse() {
    let values = NativeTestValues::new();
    let first = values.insert(String::from("first"));

    assert!(bray_runtime_abi::NativeBrayCallOutcome::panicked(first).is_some());
    assert_eq!(values.take(first), Some(String::from("first")));
    assert_eq!(values.take(first), None);

    let second = values.insert(String::from("second"));

    assert_ne!(second, first);
    assert_eq!(values.take(second), Some(String::from("second")));
    assert_eq!(values.take(0), None);
}

pub(crate) fn register_task<T: 'static, F>(
    scheduler: &Scheduler,
    task: &TaskControlBlock<T, F>,
    origin: RuntimeThreadId,
) -> TaskRegistration
where
    F: ?Sized + ProtectedFrame<Output = T>,
{
    scheduler
        .register_task(
            task.id(),
            // Registration retains immutable frame metadata after this task borrow ends.
            task.descriptor().clone(),
            origin,
            ProtectedFrameStateId::new(0),
            task.cancellation_context(),
        )
        .unwrap_or_else(|error| panic!("test task must register: {error:?}"))
}

pub(crate) struct TestFrame {
    descriptor: ProtectedFrameDescriptor,
    behavior: TestFrameBehavior,
    cleanup_panics: bool,
    wake_on_suspension: bool,
}

enum TestFrameBehavior {
    Sequence(VecDeque<FrameProgress<i32>>),
    CancellationAware,
    Blocking {
        entered: Arc<Barrier>,
        release: Arc<Barrier>,
        value: i32,
    },
    Panics,
    PropagatesCancellation,
}

impl TestFrame {
    pub(crate) fn completing(value: i32) -> Self {
        Self::sequence([FrameProgress::Completed(value)], 1)
    }

    pub(crate) fn suspending_then_completing(value: i32) -> Self {
        Self::sequence(
            [
                FrameProgress::Suspended(FrameSuspension::new(ProtectedFrameStateId::new(1))),
                FrameProgress::Completed(value),
            ],
            2,
        )
    }

    pub(crate) fn yielding_then_completing(value: i32) -> Self {
        Self::sequence(
            [
                FrameProgress::Suspended(FrameSuspension::yielding(ProtectedFrameStateId::new(1))),
                FrameProgress::Completed(value),
            ],
            2,
        )
    }

    pub(crate) fn retaining_state(value: i32) -> Self {
        Self {
            descriptor: descriptor_from_states([
                state_descriptor(0, [], ProtectedFrameAffinity::Movable),
                state_descriptor_with_storage(
                    1,
                    [ProtectedFrameStorageId::new(4)],
                    [ProtectedFrameDependencyId::new(6)],
                ),
            ]),
            behavior: TestFrameBehavior::Sequence(
                [
                    FrameProgress::Suspended(FrameSuspension::new(ProtectedFrameStateId::new(1))),
                    FrameProgress::Completed(value),
                ]
                .into(),
            ),
            cleanup_panics: false,
            wake_on_suspension: false,
        }
    }

    pub(crate) fn cancellation_aware() -> Self {
        Self {
            descriptor: descriptor(1),
            behavior: TestFrameBehavior::CancellationAware,
            cleanup_panics: false,
            wake_on_suspension: false,
        }
    }

    pub(crate) fn panicking_with_cleanup_panic() -> Self {
        Self::panicking_with_cleanup(true)
    }

    pub(crate) fn propagating_cancellation(cleanup_panics: bool) -> Self {
        Self {
            descriptor: descriptor(1),
            behavior: TestFrameBehavior::PropagatesCancellation,
            cleanup_panics,
            wake_on_suspension: false,
        }
    }

    pub(crate) fn blocking(entered: Arc<Barrier>, release: Arc<Barrier>, value: i32) -> Self {
        Self {
            descriptor: descriptor(1),
            behavior: TestFrameBehavior::Blocking {
                entered,
                release,
                value,
            },
            cleanup_panics: false,
            wake_on_suspension: false,
        }
    }

    pub(crate) fn invalid_suspension() -> Self {
        Self::sequence(
            [FrameProgress::Suspended(FrameSuspension::new(
                ProtectedFrameStateId::new(9),
            ))],
            1,
        )
    }

    pub(crate) fn main_thread_self_waking(value: i32) -> Self {
        Self {
            descriptor: descriptor_with(
                2,
                [ExecutionLaneRequirement::MainThread],
                ProtectedFrameAffinity::MainThread,
            ),
            behavior: TestFrameBehavior::Sequence(
                [
                    FrameProgress::Suspended(FrameSuspension::new(ProtectedFrameStateId::new(1))),
                    FrameProgress::Completed(value),
                ]
                .into(),
            ),
            cleanup_panics: false,
            wake_on_suspension: true,
        }
    }

    pub(crate) fn panicking() -> Self {
        Self::panicking_with_cleanup(false)
    }

    fn panicking_with_cleanup(cleanup_panics: bool) -> Self {
        Self {
            descriptor: descriptor(1),
            behavior: TestFrameBehavior::Panics,
            cleanup_panics,
            wake_on_suspension: false,
        }
    }

    pub(crate) fn requiring(
        requirements: impl IntoIterator<Item = ExecutionLaneRequirement> + Clone,
        value: i32,
    ) -> Self {
        Self {
            descriptor: descriptor_with(1, requirements, ProtectedFrameAffinity::Movable),
            behavior: TestFrameBehavior::Sequence([FrameProgress::Completed(value)].into()),
            cleanup_panics: false,
            wake_on_suspension: false,
        }
    }

    pub(crate) fn main_thread_cancellation_aware() -> Self {
        Self {
            descriptor: descriptor_with(
                1,
                [ExecutionLaneRequirement::MainThread],
                ProtectedFrameAffinity::MainThread,
            ),
            behavior: TestFrameBehavior::CancellationAware,
            cleanup_panics: false,
            wake_on_suspension: false,
        }
    }

    pub(crate) fn main_thread_then_movable(value: i32) -> Self {
        Self {
            descriptor: descriptor_from_states([
                state_descriptor(
                    0,
                    [ExecutionLaneRequirement::MainThread],
                    ProtectedFrameAffinity::MainThread,
                ),
                state_descriptor(1, [], ProtectedFrameAffinity::Movable),
            ]),
            behavior: TestFrameBehavior::Sequence(
                [
                    FrameProgress::Suspended(FrameSuspension::new(ProtectedFrameStateId::new(1))),
                    FrameProgress::Completed(value),
                ]
                .into(),
            ),
            cleanup_panics: false,
            wake_on_suspension: true,
        }
    }

    fn sequence<const N: usize>(progress: [FrameProgress<i32>; N], state_count: u32) -> Self {
        Self {
            descriptor: descriptor(state_count),
            behavior: TestFrameBehavior::Sequence(progress.into()),
            cleanup_panics: false,
            wake_on_suspension: false,
        }
    }
}

impl ProtectedFrame for TestFrame {
    type Output = i32;

    fn descriptor(&self) -> &ProtectedFrameDescriptor {
        &self.descriptor
    }

    fn resume(self: Pin<&mut Self>, context: FrameContext) -> FrameProgress<Self::Output> {
        let frame = self.get_mut();

        let progress = match &mut frame.behavior {
            TestFrameBehavior::Sequence(progress) => progress
                .pop_front()
                .unwrap_or_else(|| panic!("test frame must have another transition")),
            TestFrameBehavior::CancellationAware if context.cancellation_requested() => {
                FrameProgress::Cancelled
            }
            TestFrameBehavior::CancellationAware => {
                FrameProgress::Suspended(FrameSuspension::new(ProtectedFrameStateId::new(0)))
            }
            TestFrameBehavior::Blocking {
                entered,
                release,
                value,
            } => {
                entered.wait();
                release.wait();

                FrameProgress::Completed(*value)
            }
            TestFrameBehavior::Panics => panic!("primary test panic"),
            TestFrameBehavior::PropagatesCancellation => {
                crate::root::propagate_current_run_cancellation()
            }
        };

        if frame.wake_on_suspension
            && let FrameProgress::Suspended(suspension) = &progress
        {
            current_task_execution_context()
                .unwrap_or_else(|| panic!("self-waking frame must have a task context"))
                .wake_handle()
                .wake(suspension.state())
                .unwrap_or_else(|error| panic!("self-waking frame must wake: {error:?}"));
        }

        progress
    }

    fn broadcast_tasks(self: Pin<&mut Self>) {}

    fn resolve_lifecycle(self: Pin<&mut Self>, _exit: FrameExit) {
        if self.cleanup_panics {
            panic!("cleanup test panic");
        }
    }
}

fn descriptor(state_count: u32) -> ProtectedFrameDescriptor {
    descriptor_with(state_count, [], ProtectedFrameAffinity::Movable)
}

fn descriptor_with(
    state_count: u32,
    requirements: impl IntoIterator<Item = ExecutionLaneRequirement> + Clone,
    affinity: ProtectedFrameAffinity,
) -> ProtectedFrameDescriptor {
    let states =
        (0..state_count).map(|state| state_descriptor(state, requirements.clone(), affinity));

    descriptor_from_states(states)
}

fn state_descriptor(
    state: u32,
    requirements: impl IntoIterator<Item = ExecutionLaneRequirement>,
    affinity: ProtectedFrameAffinity,
) -> ProtectedFrameStateDescriptor {
    ProtectedFrameStateDescriptor::new(
        ProtectedFrameStateId::new(state),
        requirements,
        [],
        [],
        affinity,
    )
}

fn state_descriptor_with_storage(
    state: u32,
    storage: impl IntoIterator<Item = ProtectedFrameStorageId>,
    dependencies: impl IntoIterator<Item = ProtectedFrameDependencyId>,
) -> ProtectedFrameStateDescriptor {
    ProtectedFrameStateDescriptor::new(
        ProtectedFrameStateId::new(state),
        [],
        storage,
        dependencies,
        ProtectedFrameAffinity::Movable,
    )
}

fn descriptor_from_states<S>(states: S) -> ProtectedFrameDescriptor
where
    S: IntoIterator<Item = ProtectedFrameStateDescriptor>,
    S::IntoIter: ExactSizeIterator,
{
    let Some(alignment) = NonZeroUsize::new(8) else {
        panic!("test frame alignment must be nonzero");
    };

    let Ok(layout) = ProtectedFrameLayout::try_new(64, alignment) else {
        panic!("test frame layout must be valid");
    };

    let abi = RuntimeAbiVersion::new(1, 0);

    ProtectedFrameDescriptor::try_new(
        ProtectedAsyncFrameId::new([7; 32]),
        abi,
        ProtectedFrameAbiVersions::uniform(abi),
        layout,
        layout,
        states,
    )
    .unwrap_or_else(|error| panic!("test frame descriptor must be valid: {error:?}"))
}

pub(crate) extern "C" fn native_movable_frame_state(_: u32) -> NativeFrameState {
    NativeFrameState::new(NativeFrameAffinity::MOVABLE, NativeLaneRequirements::NONE)
}

pub(crate) extern "C" fn native_origin_frame_state(_: u32) -> NativeFrameState {
    NativeFrameState::new(
        NativeFrameAffinity::ORIGIN_THREAD,
        NativeLaneRequirements::NONE,
    )
}

pub(crate) extern "C" fn native_main_frame_state(_: u32) -> NativeFrameState {
    NativeFrameState::new(
        NativeFrameAffinity::MAIN_THREAD,
        NativeLaneRequirements::MAIN_THREAD,
    )
}

pub(crate) extern "C" fn native_blocking_frame_state(_: u32) -> NativeFrameState {
    NativeFrameState::new(
        NativeFrameAffinity::MOVABLE,
        NativeLaneRequirements::BLOCKING,
    )
}

pub(crate) extern "C" fn native_compute_frame_state(_: u32) -> NativeFrameState {
    NativeFrameState::new(
        NativeFrameAffinity::MOVABLE,
        NativeLaneRequirements::COMPUTE,
    )
}
