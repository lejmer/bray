use std::cell::{Cell, RefCell};

use bray_runtime_abi::NativeThreadCancellationCallback;

#[cfg(feature = "test-output")]
use bray_platform::{
    RunOutputContext, current_run_output_context, with_optional_run_output_context,
};
use bray_runtime_model::ProtectedFrameStateId;

use crate::{CancellationContext, ExecutionLane, TaskId, TaskStartSite, TaskWakeHandle};

#[cfg(feature = "test-output")]
pub(crate) type TaskOutput = Option<RunOutputContext>;
#[cfg(not(feature = "test-output"))]
pub(crate) type TaskOutput = ();

thread_local! {
    static CURRENT_CONTEXT: RefCell<Option<TaskExecutionContext>> =
        const { RefCell::new(None) };
    static CURRENT_RUN_CANCELLATION: RefCell<Option<CancellationContext>> =
        const { RefCell::new(None) };
    static NATIVE_THREAD_CANCELLATION: Cell<Option<NativeThreadCancellation>> =
        const { Cell::new(None) };
}

#[derive(Clone, Copy)]
struct NativeThreadCancellation {
    callback: NativeThreadCancellationCallback,
    context: usize,
}

impl NativeThreadCancellation {
    fn requested(self) -> bool {
        (self.callback)(self.context) != 0
    }
}

/// Task-local runtime context installed while one task is resumed.
#[derive(Clone, Debug)]
pub struct TaskExecutionContext {
    task: TaskId,
    state: ProtectedFrameStateId,
    cancellation: CancellationContext,
    output: TaskOutput,
    lane: ExecutionLane,
    wake: TaskWakeHandle,
}

impl TaskExecutionContext {
    /// Creates the context for one task resume.
    pub(crate) const fn new(
        task: TaskId,
        state: ProtectedFrameStateId,
        cancellation: CancellationContext,
        output: TaskOutput,
        lane: ExecutionLane,
        wake: TaskWakeHandle,
    ) -> Self {
        Self {
            task,
            state,
            cancellation,
            output,
            lane,
            wake,
        }
    }

    /// Returns the currently executing task identity.
    pub const fn task(&self) -> TaskId {
        self.task
    }

    /// Returns the protected-frame state active for this resume.
    pub const fn state(&self) -> ProtectedFrameStateId {
        self.state
    }

    /// Returns this task's structured cancellation context.
    pub const fn cancellation(&self) -> &CancellationContext {
        &self.cancellation
    }

    /// Returns the lane selected for this resume.
    pub const fn lane(&self) -> ExecutionLane {
        self.lane
    }

    /// Returns authority to wake the current suspended continuation.
    pub const fn wake_handle(&self) -> &TaskWakeHandle {
        &self.wake
    }
}

/// Returns the current task-local execution context.
pub fn current_task_execution_context() -> Option<TaskExecutionContext> {
    CURRENT_CONTEXT.with_borrow(Clone::clone)
}

pub(crate) fn current_task_start_site() -> Option<TaskStartSite> {
    CURRENT_CONTEXT.with_borrow(|context| {
        context
            .as_ref()
            .map(|context| TaskStartSite::new(context.task, context.state))
    })
}

pub(crate) fn current_task_execution_lane() -> Option<ExecutionLane> {
    CURRENT_CONTEXT.with_borrow(|context| context.as_ref().map(TaskExecutionContext::lane))
}

pub(crate) fn with_task_frame_execution<T>(
    execution: &crate::FrameExecutionState,
    lane: ExecutionLane,
    callback: impl FnOnce() -> T,
) -> Option<T> {
    let mut context = CURRENT_CONTEXT.with_borrow(Clone::clone)?;
    context.state = execution.state();
    context.lane = lane;

    let previous = CURRENT_CONTEXT.replace(Some(context));
    let _guard = TaskContextGuard(previous);

    Some(callback())
}

/// Returns whether cancellation is currently observable in the current run.
pub fn current_run_cancellation_observable() -> bool {
    CURRENT_RUN_CANCELLATION.with_borrow(|context| {
        context
            .as_ref()
            .is_some_and(CancellationContext::is_requested)
    })
}

pub(crate) fn enter_current_run_cleanup_shield() {
    CURRENT_RUN_CANCELLATION.with_borrow(|context| {
        if let Some(context) = context.as_ref() {
            context.enter_shield();
        }
    });
}

pub(crate) fn leave_current_run_cleanup_shield() {
    CURRENT_RUN_CANCELLATION.with_borrow(|context| {
        if let Some(context) = context.as_ref() {
            context.leave_shield();
        }
    });
}

/// Returns whether cancellation was requested for the current run, including while shielded.
pub fn current_run_cancellation_requested() -> bool {
    let requested = CURRENT_RUN_CANCELLATION.with_borrow(|context| {
        context
            .as_ref()
            .is_some_and(|context| context.observation().requested())
    });

    requested || native_thread_cancellation_requested()
}

fn native_thread_cancellation_requested() -> bool {
    NATIVE_THREAD_CANCELLATION
        .get()
        .is_some_and(NativeThreadCancellation::requested)
}

pub(crate) fn with_native_thread_cancellation<T>(
    callback: NativeThreadCancellationCallback,
    context: usize,
    operation: impl FnOnce() -> T,
) -> T {
    let previous =
        NATIVE_THREAD_CANCELLATION.replace(Some(NativeThreadCancellation { callback, context }));

    let _guard = NativeThreadCancellationGuard(previous);

    operation()
}

/// Installs one task-local context for the duration of a resume operation.
pub(crate) fn with_task_execution_context<T>(
    context: TaskExecutionContext,
    callback: impl FnOnce() -> T,
) -> T {
    let cancellation = context.cancellation.clone();
    let output = context.output.clone();
    let previous = CURRENT_CONTEXT.replace(Some(context));

    let previous_cancellation = CURRENT_RUN_CANCELLATION.replace(Some(cancellation));

    let _guard = ContextGuard {
        task: previous,
        cancellation: previous_cancellation,
    };

    with_task_output(output, callback)
}

#[cfg(feature = "test-output")]
pub(crate) fn current_task_output() -> TaskOutput {
    current_run_output_context()
}

#[cfg(not(feature = "test-output"))]
pub(crate) const fn current_task_output() -> TaskOutput {}

#[cfg(feature = "test-output")]
pub(crate) fn with_task_output<T>(output: TaskOutput, callback: impl FnOnce() -> T) -> T {
    with_optional_run_output_context(output, callback)
}

#[cfg(not(feature = "test-output"))]
pub(crate) fn with_task_output<T>(_output: TaskOutput, callback: impl FnOnce() -> T) -> T {
    callback()
}

pub(crate) fn with_run_cancellation_context<T>(
    cancellation: CancellationContext,
    callback: impl FnOnce() -> T,
) -> T {
    let previous = CURRENT_RUN_CANCELLATION.replace(Some(cancellation));

    let _guard = RunCancellationGuard(previous);

    callback()
}

struct ContextGuard {
    task: Option<TaskExecutionContext>,
    cancellation: Option<CancellationContext>,
}

struct TaskContextGuard(Option<TaskExecutionContext>);

impl Drop for ContextGuard {
    fn drop(&mut self) {
        let previous_task = self.task.take();
        let previous_cancellation = self.cancellation.take();

        CURRENT_CONTEXT.set(previous_task);

        CURRENT_RUN_CANCELLATION.set(previous_cancellation);
    }
}

impl Drop for TaskContextGuard {
    fn drop(&mut self) {
        CURRENT_CONTEXT.set(self.0.take());
    }
}

struct RunCancellationGuard(Option<CancellationContext>);

struct NativeThreadCancellationGuard(Option<NativeThreadCancellation>);

impl Drop for NativeThreadCancellationGuard {
    fn drop(&mut self) {
        NATIVE_THREAD_CANCELLATION.set(self.0.take());
    }
}

impl Drop for RunCancellationGuard {
    fn drop(&mut self) {
        CURRENT_RUN_CANCELLATION.set(self.0.take());
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;

    use bray_platform::RuntimeThreadScope;
    #[cfg(feature = "test-output")]
    use bray_platform::{
        CapturedRunStream, RunOutputContext, RunOutputStream, begin_current_run_output_operation,
        with_run_output_context,
    };
    use bray_runtime_model::{ProtectedFrameStateId, RuntimeCapability};

    use super::{
        TaskExecutionContext, current_run_cancellation_observable,
        current_run_cancellation_requested, current_task_execution_context,
        with_task_execution_context,
    };
    use crate::test_support::TestFrame;
    use crate::{
        CancellationContext, ExecutionLane, ExecutionLanePlacement, ExecutionWorkload, Scheduler,
        SchedulerLimits, TaskControlBlock,
    };

    #[test]
    fn task_contexts_are_scoped_and_restore_the_previous_context() {
        let runtime = RuntimeThreadScope::enter()
            .unwrap_or_else(|error| panic!("runtime thread must initialize: {error:?}"));

        let task =
            TaskControlBlock::start(crate::test_support::admit_task(), TestFrame::completing(1));

        let lane = ExecutionLane::new(
            ExecutionLanePlacement::PinnedWorker(runtime.runtime().id()),
            ExecutionWorkload::Cooperative,
        );

        let scheduler = Scheduler::new(
            [
                RuntimeCapability::CooperativeExecution,
                RuntimeCapability::MainThreadLane,
            ],
            runtime.runtime().id(),
            SchedulerLimits::new(nonzero(1), nonzero(1)),
        );

        let cancellation = CancellationContext::root();

        let registration = scheduler
            .register_task(
                task.id(),
                task.descriptor().clone(),
                runtime.runtime().id(),
                ProtectedFrameStateId::new(0),
                &cancellation,
            )
            .unwrap_or_else(|error| panic!("task must register: {error:?}"));

        let context = TaskExecutionContext::new(
            task.id(),
            ProtectedFrameStateId::new(0),
            cancellation,
            task.output_context().clone(),
            lane,
            registration.wake_handle(),
        );

        assert!(current_task_execution_context().is_none());

        with_task_execution_context(context, || {
            assert!(!current_run_cancellation_observable());
            assert!(!current_run_cancellation_requested());

            assert_eq!(
                current_task_execution_context()
                    .as_ref()
                    .map(TaskExecutionContext::task),
                Some(task.id())
            );

            assert_eq!(
                current_task_execution_context()
                    .as_ref()
                    .map(TaskExecutionContext::state),
                Some(ProtectedFrameStateId::new(0))
            );
        });

        assert!(current_task_execution_context().is_none());
    }

    #[cfg(feature = "test-output")]
    #[test]
    fn task_contexts_inherit_their_root_output_sinks() {
        let runtime = RuntimeThreadScope::enter()
            .unwrap_or_else(|error| panic!("runtime thread must initialize: {error:?}"));

        let output = RunOutputContext::captured(32, 64);

        let task = with_run_output_context(output.clone(), || {
            TaskControlBlock::start(crate::test_support::admit_task(), TestFrame::completing(1))
        });

        let scheduler = Scheduler::new(
            [RuntimeCapability::CooperativeExecution],
            runtime.runtime().id(),
            SchedulerLimits::new(nonzero(1), nonzero(1)),
        );

        let cancellation = CancellationContext::root();

        let registration = scheduler
            .register_task(
                task.id(),
                task.descriptor().clone(),
                runtime.runtime().id(),
                ProtectedFrameStateId::new(0),
                &cancellation,
            )
            .unwrap_or_else(|error| panic!("task must register: {error:?}"));

        let context = TaskExecutionContext::new(
            task.id(),
            ProtectedFrameStateId::new(0),
            cancellation,
            task.output_context().clone(),
            ExecutionLane::new(
                ExecutionLanePlacement::PinnedWorker(runtime.runtime().id()),
                ExecutionWorkload::Cooperative,
            ),
            registration.wake_handle(),
        );

        with_task_execution_context(context, || {
            let operation = begin_current_run_output_operation(RunOutputStream::StandardOutput)
                .unwrap_or_else(|error| panic!("child operation must begin: {error:?}"))
                .unwrap_or_else(|| panic!("child output must be redirected"));

            assert_eq!(operation.write(b"child output"), 12);
        });

        assert_eq!(
            output
                .captured_stream(RunOutputStream::StandardOutput)
                .as_ref()
                .map(CapturedRunStream::bytes),
            Some(&b"child output"[..])
        );
    }

    #[test]
    fn requested_state_remains_visible_while_delivery_is_shielded() {
        let cancellation = CancellationContext::root();
        let shield = cancellation.shield();

        assert!(cancellation.request());

        super::with_run_cancellation_context(cancellation, || {
            assert!(!current_run_cancellation_observable());
            assert!(current_run_cancellation_requested());
        });

        drop(shield);
    }

    #[test]
    fn generated_cleanup_shields_nest_and_restore_delivery() {
        let cancellation = CancellationContext::root();

        super::with_run_cancellation_context(cancellation, || {
            super::enter_current_run_cleanup_shield();
            super::enter_current_run_cleanup_shield();

            super::CURRENT_RUN_CANCELLATION.with_borrow(|context| {
                assert!(context.as_ref().unwrap().request());
            });

            assert!(current_run_cancellation_requested());
            assert!(!current_run_cancellation_observable());
            super::leave_current_run_cleanup_shield();
            assert!(!current_run_cancellation_observable());
            super::leave_current_run_cleanup_shield();
            assert!(current_run_cancellation_observable());
        });
    }

    fn nonzero(value: usize) -> NonZeroUsize {
        NonZeroUsize::new(value)
            .unwrap_or_else(|| panic!("test scheduler capacity must be nonzero"))
    }
}
