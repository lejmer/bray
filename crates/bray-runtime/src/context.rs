use std::cell::RefCell;

use bray_platform::{RunOutputContext, with_optional_run_output_context};
use bray_runtime_model::ProtectedFrameStateId;

use crate::{CancellationContext, ExecutionLane, TaskId, TaskStartSite, TaskWakeHandle};

thread_local! {
    static CURRENT_CONTEXT: RefCell<Option<TaskExecutionContext>> =
        const { RefCell::new(None) };
    static CURRENT_RUN_CANCELLATION: RefCell<Option<CancellationContext>> =
        const { RefCell::new(None) };
}

/// Task-local runtime context installed while one task is resumed.
#[derive(Clone, Debug)]
pub struct TaskExecutionContext {
    task: TaskId,
    state: ProtectedFrameStateId,
    cancellation: CancellationContext,
    output: Option<RunOutputContext>,
    lane: ExecutionLane,
    wake: TaskWakeHandle,
}

impl TaskExecutionContext {
    /// Creates the context for one task resume.
    pub(crate) const fn new(
        task: TaskId,
        state: ProtectedFrameStateId,
        cancellation: CancellationContext,
        output: Option<RunOutputContext>,
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
    CURRENT_CONTEXT.with(|context| context.borrow().clone())
}

pub(crate) fn current_task_start_site() -> Option<TaskStartSite> {
    CURRENT_CONTEXT.with(|context| {
        context
            .borrow()
            .as_ref()
            .map(|context| TaskStartSite::new(context.task, context.state))
    })
}

/// Returns whether cancellation is currently observable in the current run.
pub fn current_run_cancellation_observable() -> bool {
    CURRENT_RUN_CANCELLATION.with(|context| {
        context
            .borrow()
            .as_ref()
            .is_some_and(CancellationContext::is_requested)
    })
}

/// Returns whether cancellation was requested for the current run, including while shielded.
pub fn current_run_cancellation_requested() -> bool {
    CURRENT_RUN_CANCELLATION.with(|context| {
        context
            .borrow()
            .as_ref()
            .is_some_and(|context| context.observation().requested())
    })
}

/// Installs one task-local context for the duration of a resume operation.
pub(crate) fn with_task_execution_context<T>(
    context: TaskExecutionContext,
    callback: impl FnOnce() -> T,
) -> T {
    let cancellation = context.cancellation.clone();
    let output = context.output.clone();
    let previous = CURRENT_CONTEXT.with(|current| current.replace(Some(context)));

    let previous_cancellation =
        CURRENT_RUN_CANCELLATION.with(|current| current.replace(Some(cancellation)));

    let _guard = ContextGuard {
        task: previous,
        cancellation: previous_cancellation,
    };

    with_optional_run_output_context(output, callback)
}

pub(crate) fn with_run_cancellation_context<T>(
    cancellation: CancellationContext,
    callback: impl FnOnce() -> T,
) -> T {
    let previous = CURRENT_RUN_CANCELLATION.with(|current| current.replace(Some(cancellation)));

    let _guard = RunCancellationGuard(previous);

    callback()
}

struct ContextGuard {
    task: Option<TaskExecutionContext>,
    cancellation: Option<CancellationContext>,
}

impl Drop for ContextGuard {
    fn drop(&mut self) {
        let previous_task = self.task.take();
        let previous_cancellation = self.cancellation.take();

        CURRENT_CONTEXT.with(|context| {
            context.replace(previous_task);
        });

        CURRENT_RUN_CANCELLATION.with(|context| {
            context.replace(previous_cancellation);
        });
    }
}

struct RunCancellationGuard(Option<CancellationContext>);

impl Drop for RunCancellationGuard {
    fn drop(&mut self) {
        let previous = self.0.take();

        CURRENT_RUN_CANCELLATION.with(|context| {
            context.replace(previous);
        });
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;

    use bray_platform::RuntimeThreadScope;
    use bray_platform::{
        CapturedRunStream, RunOutputContext, RunOutputStream, with_run_output_context,
        write_current_run_output,
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

        let task = TaskControlBlock::start(TestFrame::completing(1))
            .unwrap_or_else(|error| panic!("test task must start: {error:?}"));

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
            task.output_context().cloned(),
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

    #[test]
    fn task_contexts_inherit_their_root_output_sinks() {
        let runtime = RuntimeThreadScope::enter()
            .unwrap_or_else(|error| panic!("runtime thread must initialize: {error:?}"));

        let output = RunOutputContext::captured(32, 64);

        let task = with_run_output_context(output.clone(), || {
            TaskControlBlock::start(TestFrame::completing(1))
                .unwrap_or_else(|error| panic!("test task must start: {error:?}"))
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
            task.output_context().cloned(),
            ExecutionLane::new(
                ExecutionLanePlacement::PinnedWorker(runtime.runtime().id()),
                ExecutionWorkload::Cooperative,
            ),
            registration.wake_handle(),
        );

        with_task_execution_context(context, || {
            assert_eq!(
                write_current_run_output(RunOutputStream::StandardOutput, b"child output"),
                Some(12)
            );
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

    fn nonzero(value: usize) -> NonZeroUsize {
        NonZeroUsize::new(value)
            .unwrap_or_else(|| panic!("test scheduler capacity must be nonzero"))
    }
}
