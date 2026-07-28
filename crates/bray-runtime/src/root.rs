use std::panic::{AssertUnwindSafe, catch_unwind};

use bray_platform::RuntimeThread;
use bray_runtime_interface::ProtectedFrameStateId;

use crate::{
    CancellationContext, ExecutionLane, ExecutionLanePlacement, ExecutionWorkload,
    ProtectedFrame, RunOutcome, Scheduler, SchedulerError, TaskControlBlock,
    TaskExecutionContext, TaskId,
    TaskObservationError, TaskResumeError, TaskResumeStatus, TaskStartError,
    with_task_execution_context,
};

/// Product-host authority to request cancellation of the executable root run.
#[derive(Clone, Debug)]
pub struct RootCancellationHandle {
    cancellation: CancellationContext,
}

impl RootCancellationHandle {
    /// Requests cooperative cancellation of the executable root.
    pub fn request(&self) -> bool {
        self.cancellation.request()
    }
}

/// Failure to enter, drive, or observe an executable root run.
#[derive(Debug)]
pub enum RootExecutionError {
    /// Stable root-task storage could not be created.
    TaskStart(TaskStartError),
    /// The selected scheduler rejected root registration or dispatch.
    Scheduler(SchedulerError),
    /// The protected root frame could not be resumed.
    TaskResume(TaskResumeError),
    /// The terminal root outcome could not be observed.
    TaskObservation(TaskObservationError),
    /// The root frame was not assigned to the distinguished main-thread lane.
    WrongExecutionLane(ExecutionLane),
    /// The scheduler produced a task other than the host-owned root.
    UnexpectedTask(TaskId),
}

impl From<TaskStartError> for RootExecutionError {
    fn from(error: TaskStartError) -> Self {
        Self::TaskStart(error)
    }
}

impl From<SchedulerError> for RootExecutionError {
    fn from(error: SchedulerError) -> Self {
        Self::Scheduler(error)
    }
}

impl From<TaskResumeError> for RootExecutionError {
    fn from(error: TaskResumeError) -> Self {
        Self::TaskResume(error)
    }
}

impl From<TaskObservationError> for RootExecutionError {
    fn from(error: TaskObservationError) -> Self {
        Self::TaskObservation(error)
    }
}

/// Executes a synchronous entrypoint directly as the executable root run.
pub fn execute_synchronous_root<T>(root: impl FnOnce() -> T) -> RunOutcome<T> {
    catch_unwind(AssertUnwindSafe(root)).map_or_else(
        |payload| RunOutcome::Panicked(crate::RuntimePanic::from_payload(payload)),
        RunOutcome::Completed,
    )
}

/// Transfers an async entry frame into a host-owned root task and drives it to completion.
pub fn execute_async_root<T, F>(
    scheduler: &Scheduler,
    main_thread: &RuntimeThread,
    frame: F,
    on_started: impl FnOnce(RootCancellationHandle),
) -> Result<RunOutcome<T>, RootExecutionError>
where
    T: 'static,
    F: ProtectedFrame<Output = T>,
{
    let root = TaskControlBlock::start_local(frame)?;
    let initial_state = ProtectedFrameStateId::new(0);

    let registration = scheduler.register_task(
        root.id(),
        root.descriptor().clone(),
        main_thread.id(),
        initial_state,
        root.cancellation_context(),
    )?;

    let wake = registration.wake_handle();

    let root_lane = ExecutionLane::new(
        ExecutionLanePlacement::MainThread(main_thread.id()),
        ExecutionWorkload::Cooperative,
    );

    let selected_lane = registration.lane(initial_state)?;

    if selected_lane != root_lane {
        return Err(RootExecutionError::WrongExecutionLane(selected_lane));
    }

    on_started(RootCancellationHandle {
        // Host cancellation authority must remain valid while root storage is driven.
        cancellation: root.cancellation_context().clone(),
    });

    wake.wake(initial_state)?;

    loop {
        let ready = scheduler
            .wait_ready(root_lane, None)?
            .ok_or(RootExecutionError::UnexpectedTask(root.id()))?;

        if ready.task() != root.id() {
            return Err(RootExecutionError::UnexpectedTask(ready.task()));
        }

        // The execution context owns an independent view for the duration of resume.
        let context = TaskExecutionContext::new(
            root.id(),
            root.cancellation_context().clone(),
            ready.lane(),
            wake.clone(),
        );

        let status = with_task_execution_context(context, || root.resume())?;

        match status {
            TaskResumeStatus::Suspended(suspension) => ready.suspend(suspension)?,
            TaskResumeStatus::Terminal(_) => {
                drop(ready);
                drop(registration);

                return root.take_outcome().map_err(Into::into);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;

    use bray_platform::RuntimeThreadScope;
    use bray_runtime_interface::RuntimeCapability;

    use super::{execute_async_root, execute_synchronous_root};
    use crate::test_support::TestFrame;
    use crate::{RunOutcome, Scheduler, SchedulerLimits};

    #[test]
    fn synchronous_roots_execute_directly_and_capture_panics() {
        assert!(matches!(
            execute_synchronous_root(|| 17),
            RunOutcome::Completed(17)
        ));

        let panicked = execute_synchronous_root(|| -> i32 {
            panic!("root panic");
        });

        assert!(matches!(panicked, RunOutcome::Panicked(_)));
    }

    #[test]
    fn async_roots_remain_on_the_cooperative_main_thread_lane() {
        let runtime = RuntimeThreadScope::enter()
            .unwrap_or_else(|error| panic!("main runtime thread must initialize: {error:?}"));

        let scheduler = Scheduler::new(
            [
                RuntimeCapability::CooperativeExecution,
                RuntimeCapability::MainThreadLane,
            ],
            runtime.runtime().id(),
            SchedulerLimits::new(nonzero(4), nonzero(4)),
        );

        let outcome = execute_async_root(
            &scheduler,
            runtime.runtime(),
            TestFrame::main_thread_self_waking(29),
            |_| {},
        )
        .unwrap_or_else(|error| panic!("async root must complete: {error:?}"));

        assert!(matches!(outcome, RunOutcome::Completed(29)));

        assert_eq!(
            scheduler
                .task_count()
                .unwrap_or_else(|error| panic!("task count must be available: {error:?}")),
            0
        );
    }

    #[test]
    fn host_cancellation_reaches_the_root_without_a_source_task_handle() {
        let runtime = RuntimeThreadScope::enter()
            .unwrap_or_else(|error| panic!("main runtime thread must initialize: {error:?}"));

        let scheduler = Scheduler::new(
            [
                RuntimeCapability::CooperativeExecution,
                RuntimeCapability::MainThreadLane,
            ],
            runtime.runtime().id(),
            SchedulerLimits::new(nonzero(4), nonzero(4)),
        );

        let outcome = execute_async_root(
            &scheduler,
            runtime.runtime(),
            TestFrame::main_thread_cancellation_aware(),
            |root| {
                assert!(root.request());
            },
        )
        .unwrap_or_else(|error| panic!("cancelled root must terminate: {error:?}"));

        assert!(matches!(outcome, RunOutcome::Cancelled));
    }

    fn nonzero(value: usize) -> NonZeroUsize {
        NonZeroUsize::new(value)
            .unwrap_or_else(|| panic!("test scheduler capacity must be nonzero"))
    }
}
