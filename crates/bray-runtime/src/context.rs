use std::cell::RefCell;

use crate::{CancellationContext, ExecutionLane, TaskId};

thread_local! {
    static CURRENT_CONTEXT: RefCell<Option<TaskExecutionContext>> =
        const { RefCell::new(None) };
}

/// Task-local runtime context installed while one task is resumed.
#[derive(Clone, Debug)]
pub struct TaskExecutionContext {
    task: TaskId,
    cancellation: CancellationContext,
    lane: ExecutionLane,
}

impl TaskExecutionContext {
    /// Creates the context for one task resume.
    pub const fn new(task: TaskId, cancellation: CancellationContext, lane: ExecutionLane) -> Self {
        Self {
            task,
            cancellation,
            lane,
        }
    }

    /// Returns the currently executing task identity.
    pub const fn task(&self) -> TaskId {
        self.task
    }

    /// Returns this task's structured cancellation context.
    pub const fn cancellation(&self) -> &CancellationContext {
        &self.cancellation
    }

    /// Returns the lane selected for this resume.
    pub const fn lane(&self) -> ExecutionLane {
        self.lane
    }
}

/// Returns the current task-local execution context.
pub fn current_task_execution_context() -> Option<TaskExecutionContext> {
    CURRENT_CONTEXT.with(|context| context.borrow().clone())
}

/// Installs one task-local context for the duration of a resume operation.
pub fn with_task_execution_context<T>(
    context: TaskExecutionContext,
    callback: impl FnOnce() -> T,
) -> T {
    let previous = CURRENT_CONTEXT.with(|current| current.replace(Some(context)));
    let _guard = ContextGuard(previous);

    callback()
}

struct ContextGuard(Option<TaskExecutionContext>);

impl Drop for ContextGuard {
    fn drop(&mut self) {
        let previous = self.0.take();

        CURRENT_CONTEXT.with(|context| {
            context.replace(previous);
        });
    }
}

#[cfg(test)]
mod tests {
    use bray_platform::RuntimeThreadScope;

    use super::{
        TaskExecutionContext, current_task_execution_context, with_task_execution_context,
    };
    use crate::test_support::TestFrame;
    use crate::{
        CancellationContext, ExecutionLane, ExecutionLanePlacement, ExecutionWorkload,
        TaskControlBlock,
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

        let context = TaskExecutionContext::new(task.id(), CancellationContext::root(), lane);

        assert!(current_task_execution_context().is_none());

        with_task_execution_context(context, || {
            assert_eq!(
                current_task_execution_context()
                    .as_ref()
                    .map(TaskExecutionContext::task),
                Some(task.id())
            );
        });

        assert!(current_task_execution_context().is_none());
    }
}
