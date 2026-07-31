use std::collections::VecDeque;
use std::num::NonZeroUsize;
use std::pin::Pin;
use std::sync::{Arc, Barrier};

use bray_platform::RuntimeThreadId;
use bray_runtime_interface::{
    BinarySymbolName, ExecutionLaneRequirement, ProtectedAsyncFrameId, ProtectedFrameAbiVersions,
    ProtectedFrameAffinity, ProtectedFrameDependencyId, ProtectedFrameDescriptor,
    ProtectedFrameLayout, ProtectedFrameOperations, ProtectedFrameStateDescriptor,
    ProtectedFrameStateId, ProtectedFrameStorageId, RuntimeAbiVersion,
};

use crate::{
    FrameContext, FrameExit, FrameProgress, FrameSuspension, ProtectedFrame,
    Scheduler, TaskControlBlock, TaskRegistration, current_task_execution_context,
};

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

fn descriptor_from_states(
    states: impl IntoIterator<Item = ProtectedFrameStateDescriptor>,
) -> ProtectedFrameDescriptor {
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
        operations(),
        states,
    )
    .unwrap_or_else(|error| panic!("test frame descriptor must be valid: {error:?}"))
}

fn operations() -> ProtectedFrameOperations {
    ProtectedFrameOperations::new(
        symbol("__bray_test_move"),
        symbol("__bray_test_state"),
        symbol("__bray_test_resume"),
        symbol("__bray_test_cancel"),
        symbol("__bray_test_broadcast"),
        symbol("__bray_test_resolve_lifecycle"),
        symbol("__bray_test_move_completion"),
        symbol("__bray_test_destroy"),
    )
}

fn symbol(name: &'static str) -> BinarySymbolName {
    BinarySymbolName::try_new(name)
        .unwrap_or_else(|| panic!("test operation symbol must be nonempty"))
}
