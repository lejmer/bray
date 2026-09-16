use std::panic::{AssertUnwindSafe, catch_unwind, panic_any};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use bray_platform::RuntimeThread;
use bray_runtime_model::ProtectedFrameStateId;

use crate::context::{with_run_cancellation_context, with_task_execution_context};
use crate::{
    CancellationContext, CleanupIncidentOrigin, CleanupReportSink, ExecutionLane,
    ExecutionLanePlacement, ExecutionWorkload, ProtectedFrame, ReadyTask, RunOutcome, Scheduler,
    SchedulerError, TaskControlBlock, TaskExecutionContext, TaskId, TaskObservationError,
    TaskResumeError, TaskResumeStatus,
};

/// Product-host authority to request cancellation of the executable root run.
#[derive(Clone, Debug)]
pub struct RootCancellationHandle {
    cancellation: CancellationContext,
    source: Arc<AtomicU8>,
}

impl RootCancellationHandle {
    pub(crate) fn new(cancellation: CancellationContext) -> Self {
        Self {
            cancellation,
            source: Arc::new(AtomicU8::new(0)),
        }
    }

    /// Requests cooperative cancellation of the executable root.
    pub fn request(&self) -> bool {
        self.request_with_source(RootCancellationSource::Explicit)
    }

    /// Requests cooperative cancellation because the host deadline elapsed.
    pub fn request_timeout(&self) -> bool {
        self.request_with_source(RootCancellationSource::Timeout)
    }

    /// Returns the first host request source when cancellation was requested.
    pub fn source(&self) -> Option<RootCancellationSource> {
        RootCancellationSource::from_code(self.source.load(Ordering::Acquire))
    }

    fn request_with_source(&self, source: RootCancellationSource) -> bool {
        let _ = self
            .source
            .compare_exchange(0, source.code(), Ordering::AcqRel, Ordering::Acquire);

        self.cancellation.request()
    }
}

/// First host action that requested root cancellation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RootCancellationSource {
    /// An explicit host or runner request.
    Explicit,
    /// The configured root deadline elapsed.
    Timeout,
}

impl RootCancellationSource {
    const fn code(self) -> u8 {
        match self {
            Self::Explicit => 1,
            Self::Timeout => 2,
        }
    }

    const fn from_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(Self::Explicit),
            2 => Some(Self::Timeout),
            _ => None,
        }
    }
}

/// Failure to enter, drive, or observe an executable root run.
#[derive(Debug)]
pub enum RootExecutionError {
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

#[derive(Debug)]
pub(crate) struct PropagatedCancellation;

pub(crate) fn propagate_current_run_cancellation() -> ! {
    panic_any(PropagatedCancellation)
}

pub(crate) fn is_propagated_cancellation(payload: &(dyn std::any::Any + Send)) -> bool {
    payload.is::<PropagatedCancellation>()
}

/// Executes a synchronous entrypoint directly as the executable root run.
pub fn execute_synchronous_root<T>(
    root: impl FnOnce() -> T,
    on_started: impl FnOnce(RootCancellationHandle),
) -> RunOutcome<T> {
    let cancellation = CancellationContext::root();
    on_started(RootCancellationHandle::new(cancellation.clone()));

    with_run_cancellation_context(cancellation, || {
        match catch_unwind(AssertUnwindSafe(root)) {
            Ok(value) => RunOutcome::Completed(value),
            Err(payload) if is_propagated_cancellation(payload.as_ref()) => RunOutcome::Cancelled,
            Err(payload) => RunOutcome::Panicked(crate::RuntimePanic::from_payload(payload)),
        }
    })
}

/// Transfers an async entry frame into a host-owned root task and drives it to completion.
pub fn execute_async_root<T, F>(
    admission: crate::TaskAdmission,
    scheduler: &Scheduler,
    main_thread: &RuntimeThread,
    frame: F,
    cleanup_reports: &CleanupReportSink,
    on_started: impl FnOnce(RootCancellationHandle),
    mut dispatch_child: impl FnMut(ReadyTask) -> Result<(), RootExecutionError>,
) -> Result<RunOutcome<T>, RootExecutionError>
where
    T: 'static,
    F: ProtectedFrame<Output = T>,
{
    let root = TaskControlBlock::start_local(admission, frame);
    let source = Arc::new(AtomicU8::new(0));
    let initial_state = ProtectedFrameStateId::new(0);

    let result = (|| {
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

        for state in root.descriptor().states() {
            let selected_lane = registration.lane(state.state())?;

            if selected_lane != root_lane {
                return Err(RootExecutionError::WrongExecutionLane(selected_lane));
            }
        }

        on_started(RootCancellationHandle {
            // Host cancellation authority must remain valid while root storage is driven.
            cancellation: root.cancellation_context().clone(),
            source,
        });

        wake.wake()?;

        loop {
            let ready = scheduler
                .wait_ready(root_lane, None)?
                .ok_or(RootExecutionError::UnexpectedTask(root.id()))?;

            if ready.task() != root.id() {
                dispatch_child(ready)?;

                continue;
            }

            // The execution context owns an independent view for the duration of resume.
            let context = TaskExecutionContext::new(
                root.id(),
                ready.state(),
                root.cancellation_context().clone(),
                root.output_context().clone(),
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
    })();

    if result.is_err() {
        let state = root.state_id_for_reporting();
        let panic = root.resolve_runtime_failure();

        if let Some(panic) = panic {
            root.transfer_cleanup_incident(
                cleanup_reports,
                CleanupIncidentOrigin::new(root.descriptor().frame(), state),
                panic,
            );
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;

    use bray_platform::RuntimeThreadScope;
    use bray_runtime_model::RuntimeCapability;

    use super::{
        RootCancellationSource, RootExecutionError, execute_async_root, execute_synchronous_root,
        propagate_current_run_cancellation,
    };
    use crate::context::with_task_execution_context;
    use crate::test_support::{TestFrame, register_task};
    use crate::{
        CleanupReportSink, RunOutcome, Scheduler, SchedulerLimits, TaskControlBlock,
        TaskExecutionContext, TaskResumeStatus, current_run_cancellation_observable,
    };

    #[test]
    fn synchronous_roots_execute_directly_and_capture_panics() {
        assert!(matches!(
            execute_synchronous_root(|| 17, |_| {}),
            RunOutcome::Completed(17)
        ));

        let panicked = execute_synchronous_root(|| -> i32 { panic!("root panic") }, |_| {});

        assert!(matches!(panicked, RunOutcome::Panicked(_)));
    }

    #[test]
    fn synchronous_roots_expose_host_cancellation_to_run_operations() {
        let outcome = execute_synchronous_root(current_run_cancellation_observable, |root| {
            assert!(root.request());
        });

        assert!(matches!(outcome, RunOutcome::Completed(true)));
    }

    #[test]
    fn synchronous_cancellation_entry_reaches_the_root_outcome() {
        let outcome = execute_synchronous_root(propagate_current_run_cancellation, |_| {});

        assert!(matches!(outcome, RunOutcome::Cancelled));
    }

    #[test]
    fn first_root_cancellation_source_is_stable() {
        let mut cancellation = None;

        let outcome = execute_synchronous_root(
            || (),
            |root| {
                assert!(root.request_timeout());
                assert!(!root.request());

                cancellation = Some(root);
            },
        );

        assert!(matches!(outcome, RunOutcome::Completed(())));

        assert_eq!(
            cancellation.and_then(|cancellation| cancellation.source()),
            Some(RootCancellationSource::Timeout)
        );
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

        let reports = CleanupReportSink::new();

        let outcome = execute_async_root(
            crate::test_support::admit_task(),
            &scheduler,
            runtime.runtime(),
            TestFrame::main_thread_self_waking(29),
            &reports,
            |_| {},
            |ready| Err(RootExecutionError::UnexpectedTask(ready.task())),
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
    fn every_async_root_state_must_remain_on_the_main_thread_lane() {
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

        let reports = CleanupReportSink::new();

        let result = execute_async_root(
            crate::test_support::admit_task(),
            &scheduler,
            runtime.runtime(),
            TestFrame::main_thread_then_movable(29),
            &reports,
            |_| {},
            |ready| Err(RootExecutionError::UnexpectedTask(ready.task())),
        );

        assert!(matches!(
            result,
            Err(RootExecutionError::WrongExecutionLane(_))
        ));
    }

    #[test]
    fn runtime_failure_cleanup_panics_reach_the_mandatory_report_sink() {
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

        let reports = CleanupReportSink::new();

        let result = execute_async_root(
            crate::test_support::admit_task(),
            &scheduler,
            runtime.runtime(),
            TestFrame::panicking_with_cleanup_panic(),
            &reports,
            |_| {},
            |ready| Err(RootExecutionError::UnexpectedTask(ready.task())),
        );

        assert!(matches!(
            result,
            Err(RootExecutionError::WrongExecutionLane(_))
        ));

        assert_eq!(reports.pending_count(), 1);
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

        let reports = CleanupReportSink::new();

        let outcome = execute_async_root(
            crate::test_support::admit_task(),
            &scheduler,
            runtime.runtime(),
            TestFrame::main_thread_cancellation_aware(),
            &reports,
            |root| {
                assert!(root.request());
            },
            |ready| Err(RootExecutionError::UnexpectedTask(ready.task())),
        )
        .unwrap_or_else(|error| panic!("cancelled root must terminate: {error:?}"));

        assert!(matches!(outcome, RunOutcome::Cancelled));
    }

    #[test]
    fn async_root_driver_runs_main_thread_child_tasks() {
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

        let child = TaskControlBlock::start_local(
            crate::test_support::admit_task(),
            TestFrame::main_thread_self_waking(11),
        );

        let child_registration = register_task(&scheduler, &child, runtime.runtime().id());

        let child_wake = child_registration.wake_handle();

        child_wake
            .wake()
            .unwrap_or_else(|error| panic!("child task must wake: {error:?}"));

        let reports = CleanupReportSink::new();

        let outcome = execute_async_root(
            crate::test_support::admit_task(),
            &scheduler,
            runtime.runtime(),
            TestFrame::main_thread_self_waking(29),
            &reports,
            |_| {},
            |ready| {
                assert_eq!(ready.task(), child.id());

                let context = TaskExecutionContext::new(
                    child.id(),
                    ready.state(),
                    child.cancellation_context().clone(),
                    child.output_context().clone(),
                    ready.lane(),
                    child_wake.clone(),
                );

                let status = with_task_execution_context(context, || child.resume())?;

                match status {
                    TaskResumeStatus::Suspended(suspension) => ready.suspend(suspension)?,
                    TaskResumeStatus::Terminal(_) => drop(ready),
                }

                Ok(())
            },
        )
        .unwrap_or_else(|error| panic!("async root must complete: {error:?}"));

        assert!(matches!(outcome, RunOutcome::Completed(29)));

        assert!(matches!(
            child
                .take_outcome()
                .unwrap_or_else(|error| panic!("child outcome must be available: {error:?}")),
            RunOutcome::Completed(11)
        ));

        drop(child_registration);
    }

    fn nonzero(value: usize) -> NonZeroUsize {
        NonZeroUsize::new(value)
            .unwrap_or_else(|| panic!("test scheduler capacity must be nonzero"))
    }
}
