use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, Weak};

use bray_platform::{MonotonicClock, RuntimeThread, RuntimeThreadId};
use bray_runtime_model::RuntimeCapability;

use crate::cancellation::CancellationWakeRegistration;
use crate::{
    CancellationContext, ExecutionLane, FrameExecutionState, SchedulerSnapshot, TaskId,
    TaskWakeCause,
};

use super::contract::{SchedulerError, SchedulerLimits};
use super::native::{self, BrayScheduler};
use super::deadline::BrayDeadlines;

/// One Bray scheduler and the native resources retained by its registrations.
#[derive(Clone, Debug)]
pub struct Scheduler {
    pub(super) data: Arc<SchedulerData>,
}

#[derive(Debug)]
pub(super) struct SchedulerData {
    pub(super) limits: SchedulerLimits,
    pub(super) observe_queue_time: bool,
    pub(super) state: Mutex<SchedulerState>,
    pub(super) deadlines: OnceLock<BrayDeadlines>,
    pub(super) source: BrayScheduler,
}

#[derive(Debug, Default)]
pub(super) struct SchedulerState {
    pub(super) tasks: HashMap<TaskId, RegisteredTask>,
}

// These bindings own callback resources and checked metadata, never readiness,
// links, terminal results or execution quota. Those belong to Bray.
#[derive(Debug)]
pub(super) struct RegisteredTask {
    pub(super) registration: u64,
    pub(super) execution: FrameExecutionState,
    pub(super) origin: RuntimeThreadId,
    pub(super) cancellation: Option<CancellationContext>,
    pub(super) native: Option<Arc<crate::native::NativeTaskServices>>,
}

impl Scheduler {
    pub(crate) fn wake_waiters(&self) {
        native::bray_runtime_scheduler_notification(&self.data.source.reference);
    }

    /// Creates a scheduler without starting workers or optional services.
    /// Returns an error if scheduler backing cannot be admitted.
    pub fn new(
        capabilities: impl IntoIterator<Item = RuntimeCapability>,
        main_thread: RuntimeThreadId,
        limits: SchedulerLimits,
    ) -> Result<Self, SchedulerError> {
        Self::create(capabilities, main_thread, limits, false)
    }

    /// Creates a scheduler that records queue timestamps for inspection.
    /// Returns an error if scheduler backing cannot be admitted.
    pub fn new_observed(
        capabilities: impl IntoIterator<Item = RuntimeCapability>,
        main_thread: RuntimeThreadId,
        limits: SchedulerLimits,
    ) -> Result<Self, SchedulerError> {
        Self::create(capabilities, main_thread, limits, true)
    }

    fn create(
        capabilities: impl IntoIterator<Item = RuntimeCapability>,
        main_thread: RuntimeThreadId,
        limits: SchedulerLimits,
        observe_queue_time: bool,
    ) -> Result<Self, SchedulerError> {
        let source = BrayScheduler::new(capabilities, main_thread, limits.tasks().get())?;

        Ok(Self {
            data: Arc::new(SchedulerData {
                limits,
                observe_queue_time,
                state: Mutex::new(SchedulerState::default()),
                deadlines: OnceLock::new(),
                source,
            }),
        })
    }

    /// Registers an independent host run without making it ready.
    pub fn register_task(
        &self,
        task: TaskId,
        execution: FrameExecutionState,
        origin: RuntimeThreadId,
        cancellation: &CancellationContext,
    ) -> Result<TaskRegistration, SchedulerError> {
        let mut registration =
            self.register(task, execution, origin, Some(cancellation.clone()))?;

        let wake = registration.wake_handle();

        let cancellation_wake = cancellation.register_wake(|| {
            Arc::new(move || {
                let _ = wake.wake_cancellation();
            })
        })?;

        registration._cancellation_wake = cancellation_wake;

        Ok(registration)
    }

    pub(crate) fn register_native(
        &self,
        task: TaskId,
        execution: FrameExecutionState,
        origin: RuntimeThreadId,
    ) -> Result<TaskRegistration, SchedulerError> {
        self.register(task, execution, origin, None)
    }

    fn register(
        &self,
        task: TaskId,
        execution: FrameExecutionState,
        origin: RuntimeThreadId,
        cancellation: Option<CancellationContext>,
    ) -> Result<TaskRegistration, SchedulerError> {
        let lane = self
            .data
            .source
            .select(execution.descriptor(), execution.origin().unwrap_or(origin))?;

        let mut registration = 0;

        let status = native::bray_runtime_scheduler_admission(
            &self.data.source.reference,
            task.raw(),
            &native::to_native(lane),
            &mut registration,
        );

        match status {
            0 => {}
            1 => return Err(SchedulerError::BackingUnavailable),
            2 => return Err(SchedulerError::RegistrationIdentityExhausted),
            _ => panic!("Bray admission returned unknown status {status}"),
        }

        // This guard also rolls back Source admission if native binding backing
        // cannot be prepared. No frame or callback has transferred yet.
        let owner = TaskRegistration {
            task,
            origin,
            registration,
            scheduler: Arc::downgrade(&self.data),
            _cancellation_wake: None,
        };

        let mut prepared = HashMap::new();

        loop {
            let mut state = self.lock_state()?;

            if state.tasks.contains_key(&task) {
                return Err(SchedulerError::TaskAlreadyRegistered(task));
            }

            if state.tasks.len() < state.tasks.capacity() {
                state.tasks.insert(
                    task,
                    RegisteredTask {
                        registration,
                        execution,
                        origin,
                        cancellation,
                        native: None,
                    },
                );

                return Ok(owner);
            }

            let required = state
                .tasks
                .len()
                .checked_add(1)
                .expect("binding capacity is representable");

            if prepared.capacity() >= required {
                prepared.extend(state.tasks.drain());
                std::mem::swap(&mut prepared, &mut state.tasks);

                state.tasks.insert(
                    task,
                    RegisteredTask {
                        registration,
                        execution,
                        origin,
                        cancellation,
                        native: None,
                    },
                );

                return Ok(owner);
            }

            drop(state);

            prepared
                .try_reserve(required)
                .map_err(SchedulerError::ReadyStorageUnavailable)?;
        }
    }

    /// Takes the next ready task with fair turns across the acquired lanes.
    /// The caller retains the current thread's attachment through this operation.
    pub fn take_ready(
        &self,
        thread: &RuntimeThread,
        lanes: impl IntoIterator<Item = ExecutionLane>,
    ) -> Result<Option<super::dispatch::ReadyTask>, SchedulerError> {
        self.promote_elapsed_timers()?;

        let thread = thread.id();

        let lanes = lanes
            .into_iter()
            .fold(0, |mask, lane| mask | native::lane_bit(lane, thread));

        super::dispatch::pop_ready(&self.data, thread, lanes)
    }

    /// Returns the number of registered independent runs.
    pub fn task_count(&self) -> Result<usize, SchedulerError> {
        Ok(self.lock_state()?.tasks.len())
    }

    // Keep the creating attachment and any checked live frame origin through
    // registration retirement. This cold resource query owns no lane policy.
    pub(crate) fn has_attachment_users(&self, thread: &RuntimeThread) -> bool {
        let state = self
            .lock_state()
            .expect("worker retirement retains scheduler bindings");

        state.tasks.values().any(|binding| {
            binding.origin == thread.id() || binding.execution.origin() == Some(thread.id())
        })
    }

    pub(crate) fn unregister_native_task(&self, task: TaskId) {
        unregister_task(&self.data, task, None);
    }

    pub(crate) fn install_native_task(
        &self,
        task: Arc<crate::native::NativeTaskServices>,
    ) -> Result<(), SchedulerError> {
        let mut state = self.lock_state()?;

        let registered = state
            .tasks
            .get_mut(&task.id())
            .ok_or(SchedulerError::UnknownTask(task.id()))?;

        assert!(
            registered.native.is_none(),
            "a registration accepts native resources once"
        );

        registered.cancellation = Some(task.cancellation.clone());
        registered.native = Some(task);

        Ok(())
    }

    pub(crate) fn native_task(
        &self,
        task: TaskId,
    ) -> Result<Arc<crate::native::NativeTaskServices>, SchedulerError> {
        self.lock_state()?
            .tasks
            .get(&task)
            .and_then(|registered| registered.native.as_ref())
            .map(Arc::clone)
            .ok_or(SchedulerError::UnknownTask(task))
    }

    /// Captures current task metadata in deterministic identity order.
    pub fn snapshot(&self) -> Result<SchedulerSnapshot, SchedulerError> {
        super::dispatch::scheduler_snapshot(&self.data)
    }

    pub(super) fn lock_state(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, SchedulerState>, SchedulerError> {
        self.data
            .state
            .lock()
            .map_err(|_| SchedulerError::SynchronizationPoisoned)
    }
}

impl SchedulerData {
    pub(super) fn ticks(&self) -> u64 {
        if self.observe_queue_time {
            MonotonicClock
                .ticks()
                .expect("observed queue time remains representable")
        } else {
            0
        }
    }
}

impl Drop for SchedulerData {
    fn drop(&mut self) {
        // No concurrent operation survives the final Arc. Retire links before
        // dropping the native bindings; their registrations may reenter Drop.
        let state = self
            .state
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        for registered in state.tasks.values() {
            native::bray_runtime_scheduler_withdrawal(
                &self.source.reference,
                registered.registration,
            );
        }
    }
}

/// Registration retaining this run's membership in the scheduler.
#[derive(Debug)]
pub struct TaskRegistration {
    pub(super) task: TaskId,
    pub(super) origin: RuntimeThreadId,
    pub(super) registration: u64,
    pub(super) scheduler: Weak<SchedulerData>,
    _cancellation_wake: Option<CancellationWakeRegistration>,
}

impl TaskRegistration {
    /// Returns the independent run identity.
    pub const fn task(&self) -> TaskId {
        self.task
    }

    /// Returns a transferable wake for this exact admitted registration.
    pub fn wake_handle(&self) -> TaskWakeHandle {
        TaskWakeHandle {
            task: self.task,
            registration: self.registration,
            scheduler: self.scheduler.clone(),
        }
    }

    /// Returns the lane selected for the registered continuation.
    pub fn lane(&self) -> Result<ExecutionLane, SchedulerError> {
        let scheduler = self
            .scheduler
            .upgrade()
            .ok_or(SchedulerError::UnknownTask(self.task))?;

        let mut observation = bray_runtime_abi::NativeSchedulerObservation::default();

        let status = native::bray_runtime_scheduler_snapshot(
            &scheduler.source.reference,
            self.registration,
            &mut observation,
        );

        if status != 0 {
            return Err(SchedulerError::UnknownTask(self.task));
        }

        Ok(native::from_native(observation.lane))
    }

    pub(crate) fn execution_lane(
        &self,
        execution: &FrameExecutionState,
    ) -> Result<ExecutionLane, SchedulerError> {
        let scheduler = self
            .scheduler
            .upgrade()
            .ok_or(SchedulerError::UnknownTask(self.task))?;

        scheduler.source.select(
            execution.descriptor(),
            execution.origin().unwrap_or(self.origin),
        )
    }
}

impl Drop for TaskRegistration {
    fn drop(&mut self) {
        if let Some(scheduler) = self.scheduler.upgrade() {
            unregister_task(&scheduler, self.task, Some(self.registration));
        }
    }
}

fn unregister_task(scheduler: &SchedulerData, task: TaskId, registration: Option<u64>) {
    let removed = {
        let mut state = scheduler
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let matches = state.tasks.get(&task).is_some_and(|binding| {
            registration.is_none_or(|registration| registration == binding.registration)
        });

        if !matches {
            // Admission can fail before publishing a native binding.
            drop(state);

            if let Some(registration) = registration {
                native::bray_runtime_scheduler_withdrawal(
                    &scheduler.source.reference,
                    registration,
                );
            }

            return;
        }

        let removed = state
            .tasks
            .remove(&task)
            .expect("the matching binding remains present");

        native::bray_runtime_scheduler_withdrawal(
            &scheduler.source.reference,
            removed.registration,
        );

        removed
    };

    if let Some(deadlines) = scheduler.deadlines.get() {
        deadlines.withdraw_scope(removed.registration);
    }

    drop(removed);
}

/// Transferable wake for one generation-checked Bray registration.
#[derive(Clone, Debug)]
pub struct TaskWakeHandle {
    pub(super) task: TaskId,
    pub(super) registration: u64,
    pub(super) scheduler: Weak<SchedulerData>,
}

impl TaskWakeHandle {
    /// Coalesces a wake and reports whether it adds a new ready entry.
    pub fn wake(&self) -> Result<bool, SchedulerError> {
        self.wake_with_cause(TaskWakeCause::Explicit)
    }
    pub(crate) fn wake_cancellation(&self) -> Result<bool, SchedulerError> {
        self.wake_with_cause(TaskWakeCause::Cancellation)
    }

    pub(crate) fn wake_with_cause(&self, cause: TaskWakeCause) -> Result<bool, SchedulerError> {
        let scheduler = self
            .scheduler
            .upgrade()
            .ok_or(SchedulerError::UnknownTask(self.task))?;

        let status = native::bray_runtime_scheduler_wake(
            &scheduler.source.reference,
            self.registration,
            native::cause(cause),
            scheduler.ticks(),
        );

        match status {
            0 => Ok(false),
            1 => Ok(true),
            2 => Err(SchedulerError::UnknownTask(self.task)),
            _ => panic!("Bray wake returned unknown status {status}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;
    use std::time::Duration;

    use crate::bootstrap::RuntimeThreadScope;
    use bray_platform::MonotonicClock;
    use bray_runtime_model::{ProtectedFrameStateId, RuntimeCapability};

    use super::super::contract::{SchedulerError, SchedulerLimits};
    use super::Scheduler;
    use crate::test_support::{TestFrame, register_task};
    use crate::{
        ExecutionLane, ExecutionLanePlacement, ExecutionWorkload, FrameSuspensionKind,
        ScheduledTaskState, TaskControlBlock, TaskResumeStatus, TaskWakeCause,
    };

    #[test]
    fn delayed_wake_cannot_reach_a_retired_task_or_its_replacement() {
        let _isolation = crate::native::test_runtime_isolation();

        let runtime =
            RuntimeThreadScope::enter().expect("fixture execution scope must admit entry");

        let scheduler = scheduler(runtime.runtime().id());

        let task =
            TaskControlBlock::start(crate::test_support::admit_task(), TestFrame::completing(1));

        let registration = register_task(&scheduler, &task, runtime.runtime().id());
        let wake = registration.wake_handle();
        let retired = task.id();

        drop(registration);
        drop(task);

        let replacement =
            TaskControlBlock::start(crate::test_support::admit_task(), TestFrame::completing(2));

        let _registration = register_task(&scheduler, &replacement, runtime.runtime().id());

        assert_ne!(retired, replacement.id());
        assert!(matches!(wake.wake(), Err(SchedulerError::UnknownTask(task)) if task == retired));

        assert!(
            scheduler
                .take_ready(runtime.runtime(), [cooperative_lane()])
                .expect("fixture `take_ready` call must succeed")
                .is_none()
        );
    }

    #[test]
    fn observed_schedulers_publish_structured_queue_observations_on_demand() {
        let _isolation = crate::native::test_runtime_isolation();

        let runtime = RuntimeThreadScope::enter()
            .unwrap_or_else(|error| panic!("runtime thread must initialize: {error:?}"));

        let scheduler = Scheduler::new_observed(
            [
                RuntimeCapability::CooperativeExecution,
                RuntimeCapability::MigratableLanes,
            ],
            runtime.runtime().id(),
            SchedulerLimits::new(nonzero(8), nonzero(8)),
        )
        .expect("scheduler backing must be admitted");

        let task =
            TaskControlBlock::start(crate::test_support::admit_task(), TestFrame::completing(1));

        let registration = register_task(&scheduler, &task, runtime.runtime().id());

        registration
            .wake_handle()
            .wake()
            .unwrap_or_else(|error| panic!("task must wake: {error:?}"));

        let snapshot = scheduler
            .snapshot()
            .unwrap_or_else(|error| panic!("scheduler snapshot must succeed: {error:?}"));

        let [scheduled] = snapshot.tasks() else {
            panic!("snapshot must contain one task");
        };

        assert_eq!(scheduled.task(), task.id());
        assert_eq!(scheduled.dispatch(), ScheduledTaskState::Queued);
        assert_eq!(scheduled.wake_count(), 1);
        assert_eq!(scheduled.wake_cause(), Some(TaskWakeCause::Explicit));
        assert!(scheduled.queue_age().is_some());

        let ready = scheduler
            .take_ready(runtime.runtime(), [cooperative_lane()])
            .unwrap_or_else(|error| panic!("ready queue must be available: {error:?}"))
            .unwrap_or_else(|| panic!("task must be ready"));

        assert_eq!(ready.wake_cause(), TaskWakeCause::Explicit);
        assert!(ready.queue_latency().is_some());
    }

    #[test]
    fn ready_tasks_are_distributed_in_stable_lane_order() {
        let _isolation = crate::native::test_runtime_isolation();

        let runtime = RuntimeThreadScope::enter()
            .unwrap_or_else(|error| panic!("runtime thread must initialize: {error:?}"));

        let scheduler = scheduler(runtime.runtime().id());

        let first_task =
            TaskControlBlock::start(crate::test_support::admit_task(), TestFrame::completing(1));

        let second_task =
            TaskControlBlock::start(crate::test_support::admit_task(), TestFrame::completing(2));

        let first = register_task(&scheduler, &first_task, runtime.runtime().id());

        let second = register_task(&scheduler, &second_task, runtime.runtime().id());

        first
            .wake_handle()
            .wake()
            .unwrap_or_else(|error| panic!("first task must wake: {error:?}"));

        second
            .wake_handle()
            .wake()
            .unwrap_or_else(|error| panic!("second task must wake: {error:?}"));

        let lane = cooperative_lane();

        let first_ready = scheduler
            .take_ready(runtime.runtime(), [lane])
            .unwrap_or_else(|error| panic!("ready queue must be available: {error:?}"))
            .unwrap_or_else(|| panic!("first task must be ready"));

        assert_eq!(first_ready.task(), first_task.id());

        drop(first_ready);

        let second_ready = scheduler
            .take_ready(runtime.runtime(), [lane])
            .unwrap_or_else(|error| panic!("ready queue must be available: {error:?}"))
            .unwrap_or_else(|| panic!("second task must be ready"));

        assert_eq!(second_ready.task(), second_task.id());
    }

    #[test]
    fn cooperative_yield_places_the_current_task_behind_ready_work() {
        let _isolation = crate::native::test_runtime_isolation();

        let runtime = RuntimeThreadScope::enter()
            .unwrap_or_else(|error| panic!("runtime thread must initialize: {error:?}"));

        let scheduler = scheduler(runtime.runtime().id());

        let yielding = TaskControlBlock::start(
            crate::test_support::admit_task(),
            TestFrame::yielding_then_completing(1),
        );

        let ready =
            TaskControlBlock::start(crate::test_support::admit_task(), TestFrame::completing(2));

        let yielding_registration = register_task(&scheduler, &yielding, runtime.runtime().id());
        let ready_registration = register_task(&scheduler, &ready, runtime.runtime().id());

        yielding_registration
            .wake_handle()
            .wake()
            .unwrap_or_else(|error| panic!("yielding task must wake: {error:?}"));

        ready_registration
            .wake_handle()
            .wake()
            .unwrap_or_else(|error| panic!("ready task must wake: {error:?}"));

        let dispatch = scheduler
            .take_ready(runtime.runtime(), [cooperative_lane()])
            .unwrap_or_else(|error| panic!("ready queue must be available: {error:?}"))
            .unwrap_or_else(|| panic!("yielding task must be ready first"));

        assert_eq!(dispatch.task(), yielding.id());

        let TaskResumeStatus::Suspended(suspension, execution) = yielding
            .resume()
            .unwrap_or_else(|error| panic!("yielding task must suspend: {error:?}"))
        else {
            panic!("yielding task must suspend");
        };

        assert_eq!(suspension.kind(), FrameSuspensionKind::Yield);

        dispatch
            .suspend(execution)
            .unwrap_or_else(|error| panic!("yielding task must retain its state: {error:?}"));

        yielding_registration
            .wake_handle()
            .wake()
            .unwrap_or_else(|error| panic!("yielding task must requeue: {error:?}"));

        let next = scheduler
            .take_ready(runtime.runtime(), [cooperative_lane()])
            .unwrap_or_else(|error| panic!("ready queue must remain available: {error:?}"))
            .unwrap_or_else(|| panic!("ready task must remain queued"));

        assert_eq!(next.task(), ready.id());
    }

    #[test]
    fn duplicate_wakes_coalesce_and_running_wakes_are_not_lost() {
        let _isolation = crate::native::test_runtime_isolation();

        let runtime = RuntimeThreadScope::enter()
            .unwrap_or_else(|error| panic!("runtime thread must initialize: {error:?}"));

        let scheduler = scheduler(runtime.runtime().id());

        let task =
            TaskControlBlock::start(crate::test_support::admit_task(), TestFrame::completing(1));

        let registration = register_task(&scheduler, &task, runtime.runtime().id());

        let wake = registration.wake_handle();

        assert!(
            wake.wake()
                .unwrap_or_else(|error| panic!("first wake must succeed: {error:?}"))
        );

        assert!(
            !wake
                .wake()
                .unwrap_or_else(|error| panic!("duplicate wake must succeed: {error:?}"))
        );

        let lane = cooperative_lane();

        let ready = scheduler
            .take_ready(runtime.runtime(), [lane])
            .unwrap_or_else(|error| panic!("ready queue must be available: {error:?}"))
            .unwrap_or_else(|| panic!("task must be ready"));

        assert!(
            !wake
                .wake()
                .unwrap_or_else(|error| panic!("running wake must succeed: {error:?}"))
        );

        drop(ready);

        assert!(
            scheduler
                .take_ready(runtime.runtime(), [lane])
                .unwrap_or_else(|error| panic!("pending wake must be available: {error:?}"))
                .is_some()
        );
    }

    #[test]
    fn running_wakes_dispatch_the_newly_suspended_state() {
        let _isolation = crate::native::test_runtime_isolation();

        let runtime = RuntimeThreadScope::enter()
            .unwrap_or_else(|error| panic!("runtime thread must initialize: {error:?}"));

        let scheduler = scheduler(runtime.runtime().id());

        let task = TaskControlBlock::start(
            crate::test_support::admit_task(),
            TestFrame::suspending_then_completing(1),
        );

        let registration = register_task(&scheduler, &task, runtime.runtime().id());

        let wake = registration.wake_handle();

        wake.wake()
            .unwrap_or_else(|error| panic!("initial wake must succeed: {error:?}"));

        let lane = cooperative_lane();

        let ready = scheduler
            .take_ready(runtime.runtime(), [lane])
            .unwrap_or_else(|error| panic!("ready queue must be available: {error:?}"))
            .unwrap_or_else(|| panic!("task must be ready"));

        wake.wake()
            .unwrap_or_else(|error| panic!("running wake must succeed: {error:?}"));

        let TaskResumeStatus::Suspended(_, execution) =
            task.resume().expect("fixture `resume` call must succeed")
        else {
            panic!("test frame must suspend into state one");
        };

        ready
            .suspend(execution)
            .unwrap_or_else(|error| panic!("new suspension state must be retained: {error:?}"));

        let pending = scheduler
            .take_ready(runtime.runtime(), [lane])
            .unwrap_or_else(|error| panic!("pending wake must be available: {error:?}"))
            .unwrap_or_else(|| panic!("next state must be ready"));

        assert_eq!(pending.state(), ProtectedFrameStateId::new(1));
    }

    #[test]
    fn cancellation_makes_a_suspended_task_ready() {
        let _isolation = crate::native::test_runtime_isolation();

        let runtime = RuntimeThreadScope::enter()
            .unwrap_or_else(|error| panic!("runtime thread must initialize: {error:?}"));

        let scheduler = scheduler(runtime.runtime().id());

        let task = TaskControlBlock::start(
            crate::test_support::admit_task(),
            TestFrame::cancellation_aware(),
        );

        let _registration = register_task(&scheduler, &task, runtime.runtime().id());

        assert!(task.request_cancellation());

        let ready = scheduler
            .take_ready(runtime.runtime(), [cooperative_lane()])
            .unwrap_or_else(|error| panic!("ready queue must be available: {error:?}"))
            .unwrap_or_else(|| panic!("cancelled task must be ready"));

        assert_eq!(ready.task(), task.id());
    }

    #[test]
    fn elapsed_timers_make_tasks_ready_without_worker_count_assumptions() {
        let _isolation = crate::native::test_runtime_isolation();

        let runtime = RuntimeThreadScope::enter()
            .unwrap_or_else(|error| panic!("runtime thread must initialize: {error:?}"));

        let scheduler = scheduler(runtime.runtime().id());

        let task =
            TaskControlBlock::start(crate::test_support::admit_task(), TestFrame::completing(1));

        let registration = register_task(&scheduler, &task, runtime.runtime().id());

        let duration = Duration::ZERO;

        let _timer = scheduler
            .register_timer(duration, &registration.wake_handle())
            .unwrap_or_else(|error| panic!("timer must register: {error:?}"));

        let ready = scheduler
            .wait_ready(
                runtime.runtime(),
                [cooperative_lane()],
                MonotonicClock.deadline_after(Duration::from_secs(1)),
            )
            .unwrap_or_else(|error| panic!("timer wait must succeed: {error:?}"));

        assert_eq!(ready.as_ref().map(crate::ReadyTask::task), Some(task.id()));
    }

    #[test]
    fn elapsed_timer_coalesces_with_an_already_queued_dispatch() {
        let _isolation = crate::native::test_runtime_isolation();

        let runtime = RuntimeThreadScope::enter()
            .unwrap_or_else(|error| panic!("runtime thread must initialize: {error:?}"));

        let scheduler = scheduler(runtime.runtime().id());

        let task = TaskControlBlock::start(
            crate::test_support::admit_task(),
            TestFrame::suspending_then_completing(1),
        );

        let registration = register_task(&scheduler, &task, runtime.runtime().id());

        registration
            .wake_handle()
            .wake()
            .unwrap_or_else(|error| panic!("task must wake: {error:?}"));

        let duration = Duration::ZERO;

        let _timer = scheduler
            .register_timer(duration, &registration.wake_handle())
            .unwrap_or_else(|error| panic!("timer must register: {error:?}"));

        let ready = scheduler
            .take_ready(runtime.runtime(), [cooperative_lane()])
            .unwrap_or_else(|error| panic!("ready queue must be available: {error:?}"))
            .unwrap_or_else(|| panic!("task must remain ready"));

        assert_eq!(ready.task(), task.id());
        drop(ready);

        assert_eq!(scheduler.data.deadlines.get().expect("timer service was demanded").count(), 0);
    }

    #[test]
    fn dropping_timer_registration_withdraws_the_pending_wake() {
        let _isolation = crate::native::test_runtime_isolation();

        let runtime = RuntimeThreadScope::enter()
            .unwrap_or_else(|error| panic!("runtime thread must initialize: {error:?}"));

        let scheduler = scheduler(runtime.runtime().id());

        let task =
            TaskControlBlock::start(crate::test_support::admit_task(), TestFrame::completing(1));

        let registration = register_task(&scheduler, &task, runtime.runtime().id());

        let duration = Duration::from_secs(60);

        let timer = scheduler
            .register_timer(duration, &registration.wake_handle())
            .unwrap_or_else(|error| panic!("timer must register: {error:?}"));

        drop(timer);

        let snapshot = scheduler
            .snapshot()
            .unwrap_or_else(|error| panic!("scheduler must remain observable: {error:?}"));

        assert_eq!(snapshot.timer_count(), 0);
    }

    #[test]
    fn task_capacity_limits_execution_without_rejecting_registrations() {
        let _isolation = crate::native::test_runtime_isolation();
        let runtime = RuntimeThreadScope::enter().expect("fixture thread enters");

        let scheduler = Scheduler::new(
            [
                RuntimeCapability::CooperativeExecution,
                RuntimeCapability::MigratableLanes,
            ],
            runtime.runtime().id(),
            SchedulerLimits::new(nonzero(1), nonzero(1)),
        )
        .expect("scheduler backing must be admitted");

        let first =
            TaskControlBlock::start(crate::test_support::admit_task(), TestFrame::completing(1));

        let second =
            TaskControlBlock::start(crate::test_support::admit_task(), TestFrame::completing(2));

        let first_registration = register_task(&scheduler, &first, runtime.runtime().id());
        let second_registration = register_task(&scheduler, &second, runtime.runtime().id());

        first_registration
            .wake_handle()
            .wake()
            .expect("first task wakes");

        second_registration
            .wake_handle()
            .wake()
            .expect("second task wakes");

        assert_eq!(
            scheduler.task_count().expect("registry remains available"),
            2
        );

        let running = scheduler
            .take_ready(runtime.runtime(), [cooperative_lane()])
            .expect("first dispatch succeeds")
            .expect("first task is ready");

        assert_eq!(running.task(), first.id());

        assert!(
            scheduler
                .take_ready(runtime.runtime(), [cooperative_lane()])
                .expect("quota check succeeds")
                .is_none()
        );

        drop(running);

        assert_eq!(
            scheduler
                .take_ready(runtime.runtime(), [cooperative_lane()])
                .expect("released quota admits work")
                .expect("queued second task is ready")
                .task(),
            second.id()
        );
    }

    #[test]
    fn parked_dispatch_lends_capacity_without_releasing_its_exclusive_claim() {
        let _isolation = crate::native::test_runtime_isolation();
        let runtime = RuntimeThreadScope::enter().expect("fixture thread enters");

        let scheduler = Scheduler::new(
            [
                RuntimeCapability::CooperativeExecution,
                RuntimeCapability::MigratableLanes,
            ],
            runtime.runtime().id(),
            SchedulerLimits::new(nonzero(1), nonzero(1)),
        )
        .expect("scheduler backing must be admitted");

        let first =
            TaskControlBlock::start(crate::test_support::admit_task(), TestFrame::completing(1));

        let second =
            TaskControlBlock::start(crate::test_support::admit_task(), TestFrame::completing(2));

        let first_registration = register_task(&scheduler, &first, runtime.runtime().id());
        let second_registration = register_task(&scheduler, &second, runtime.runtime().id());

        first_registration
            .wake_handle()
            .wake()
            .expect("first task wakes");

        second_registration
            .wake_handle()
            .wake()
            .expect("second task wakes");

        let first_dispatch = scheduler
            .take_ready(runtime.runtime(), [cooperative_lane()])
            .expect("first dispatch succeeds")
            .expect("first task is ready");

        let parked = first_registration
            .park_execution()
            .expect("the running dispatch owns capacity");

        assert!(first_registration.park_execution().is_none());

        assert!(
            !first_registration
                .wake_handle()
                .wake()
                .expect("wake retains exclusive claim")
        );

        let second_dispatch = scheduler
            .take_ready(runtime.runtime(), [cooperative_lane()])
            .expect("parked capacity admits work")
            .expect("second task is ready");

        assert_eq!(second_dispatch.task(), second.id());

        assert!(
            scheduler
                .take_ready(runtime.runtime(), [cooperative_lane()])
                .expect("quota remains bounded")
                .is_none()
        );

        drop(second_dispatch);
        drop(parked);

        assert!(
            scheduler
                .take_ready(runtime.runtime(), [cooperative_lane()])
                .expect("the parent retains capacity")
                .is_none()
        );

        drop(first_dispatch);

        assert_eq!(
            scheduler
                .take_ready(runtime.runtime(), [cooperative_lane()])
                .expect("parent wake is preserved")
                .expect("the parent can run after dispatch return")
                .task(),
            first.id()
        );
    }

    #[test]
    fn deregistration_reclaims_ready_and_timer_storage() {
        let _isolation = crate::native::test_runtime_isolation();

        let runtime = RuntimeThreadScope::enter()
            .unwrap_or_else(|error| panic!("runtime thread must initialize: {error:?}"));

        let scheduler = Scheduler::new(
            [RuntimeCapability::CooperativeExecution],
            runtime.runtime().id(),
            SchedulerLimits::new(nonzero(1), nonzero(1)),
        )
        .expect("scheduler backing must be admitted");

        let task =
            TaskControlBlock::start(crate::test_support::admit_task(), TestFrame::completing(1));

        let registration = register_task(&scheduler, &task, runtime.runtime().id());

        registration
            .wake_handle()
            .wake()
            .unwrap_or_else(|error| panic!("task must wake: {error:?}"));

        let duration = Duration::from_secs(60);

        let _timer = scheduler
            .register_timer(duration, &registration.wake_handle())
            .unwrap_or_else(|error| panic!("timer must register: {error:?}"));

        drop(registration);

        let state = scheduler
            .data
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        assert!(state.tasks.is_empty());
        assert_eq!(scheduler.data.deadlines.get().expect("timer service was demanded").count(), 0);
    }

    #[test]
    fn deadline_rejection_preserves_task_and_service_ownership() {
        let _isolation = crate::native::test_runtime_isolation();
        let runtime = RuntimeThreadScope::enter().expect("runtime thread initializes");

        let scheduler = Scheduler::new(
            [RuntimeCapability::CooperativeExecution, RuntimeCapability::MigratableLanes],
            runtime.runtime().id(),
            SchedulerLimits::new(nonzero(1), nonzero(1)),
        ).expect("scheduler backing is admitted");

        let task = TaskControlBlock::start(crate::test_support::admit_task(), TestFrame::completing(1));
        let registration = register_task(&scheduler, &task, runtime.runtime().id());
        let wake = registration.wake_handle();

        assert!(matches!(scheduler.register_timer(Duration::MAX, &wake), Err(SchedulerError::DeadlineOverflow)));
        assert!(scheduler.data.deadlines.get().is_none());

        let timer = scheduler.register_timer(Duration::from_secs(60), &wake).expect("first timer is admitted");

        assert!(matches!(scheduler.register_timer(Duration::ZERO, &wake), Err(SchedulerError::TimerCapacityReached)));
        scheduler.close_timers();
        assert!(matches!(scheduler.register_timer(Duration::ZERO, &wake), Err(SchedulerError::TimersClosed)));
        assert!(wake.wake().expect("rejection retains the task's readiness owner"));
        assert_eq!(scheduler.snapshot().expect("scheduler remains observable").timer_count(), 1);

        drop(timer);
        assert_eq!(scheduler.snapshot().expect("withdrawal remains available after closure").timer_count(), 0);
    }

    #[test]
    fn closure_before_first_deadline_rejects_lazy_service_formation() {
        let _isolation = crate::native::test_runtime_isolation();
        let runtime = RuntimeThreadScope::enter().expect("runtime thread initializes");
        let scheduler = scheduler(runtime.runtime().id());
        let task = TaskControlBlock::start(crate::test_support::admit_task(), TestFrame::completing(1));
        let registration = register_task(&scheduler, &task, runtime.runtime().id());

        scheduler.close_timers();

        assert!(matches!(scheduler.register_timer(Duration::ZERO, &registration.wake_handle()), Err(SchedulerError::TimersClosed)));
        assert!(scheduler.data.deadlines.get().is_none());
    }

    #[test]
    fn pending_timer_retains_its_scheduler_until_withdrawal() {
        let _isolation = crate::native::test_runtime_isolation();
        let runtime = RuntimeThreadScope::enter().expect("runtime thread initializes");
        let scheduler = scheduler(runtime.runtime().id());
        let task = TaskControlBlock::start(crate::test_support::admit_task(), TestFrame::completing(1));
        let registration = register_task(&scheduler, &task, runtime.runtime().id());
        let wake = registration.wake_handle();
        let timer = scheduler.register_timer(Duration::from_secs(60), &wake).expect("timer is admitted");

        drop(scheduler);
        assert!(wake.wake().expect("the timer retains its scheduler"));
        drop(timer);
        assert!(matches!(wake.wake(), Err(SchedulerError::UnknownTask(_))));
    }

    fn scheduler(main_thread: bray_platform::RuntimeThreadId) -> Scheduler {
        Scheduler::new(
            [
                RuntimeCapability::CooperativeExecution,
                RuntimeCapability::MigratableLanes,
            ],
            main_thread,
            SchedulerLimits::new(nonzero(8), nonzero(8)),
        )
        .expect("scheduler backing must be admitted")
    }

    fn cooperative_lane() -> ExecutionLane {
        ExecutionLane::new(
            ExecutionLanePlacement::Migratable,
            ExecutionWorkload::Cooperative,
        )
    }

    fn nonzero(value: usize) -> NonZeroUsize {
        NonZeroUsize::new(value)
            .unwrap_or_else(|| panic!("test scheduler capacity must be nonzero"))
    }

    use crate::ExecutionLaneSelectionError;
    use bray_runtime_model::{
        ExecutionLaneRequirement, ProtectedFrameAffinity, ProtectedFrameStateDescriptor,
    };

    #[test]
    fn lane_selection_preserves_workload_and_affinity_requirements() {
        let _isolation = crate::native::test_runtime_isolation();

        let main = RuntimeThreadScope::enter()
            .unwrap_or_else(|error| panic!("main runtime thread must initialize: {error:?}"));

        let thread = main.runtime().id();

        let capabilities = [
            RuntimeCapability::CooperativeExecution,
            RuntimeCapability::LocalLanes,
            RuntimeCapability::MigratableLanes,
            RuntimeCapability::BlockingLanes,
            RuntimeCapability::ComputeLanes,
            RuntimeCapability::MainThreadLane,
        ];

        let blocking = select_execution_lane(
            &[ExecutionLaneRequirement::Blocking],
            ProtectedFrameAffinity::OriginThread,
            &capabilities,
            thread,
            thread,
        )
        .unwrap_or_else(|error| panic!("blocking lane must be available: {error:?}"));

        assert_eq!(blocking.workload(), ExecutionWorkload::Blocking);

        assert_eq!(
            blocking.placement(),
            ExecutionLanePlacement::OriginThread(thread)
        );

        let compute = select_execution_lane(
            &[ExecutionLaneRequirement::Compute],
            ProtectedFrameAffinity::Movable,
            &capabilities,
            thread,
            thread,
        )
        .unwrap_or_else(|error| panic!("compute lane must be available: {error:?}"));

        assert_eq!(compute.workload(), ExecutionWorkload::Compute);
        assert_eq!(compute.placement(), ExecutionLanePlacement::Migratable);
    }

    #[test]
    fn incompatible_or_unavailable_lanes_are_rejected() {
        let _isolation = crate::native::test_runtime_isolation();

        let main = RuntimeThreadScope::enter()
            .unwrap_or_else(|error| panic!("main runtime thread must initialize: {error:?}"));

        let thread = main.runtime().id();

        assert_eq!(
            select_execution_lane(
                &[
                    ExecutionLaneRequirement::Blocking,
                    ExecutionLaneRequirement::Compute
                ],
                ProtectedFrameAffinity::Movable,
                &[RuntimeCapability::CooperativeExecution],
                thread,
                thread,
            ),
            Err(ExecutionLaneSelectionError::MissingCapability(
                RuntimeCapability::BlockingLanes
            ))
        );

        assert_eq!(
            select_execution_lane(
                &[ExecutionLaneRequirement::Blocking],
                ProtectedFrameAffinity::Movable,
                &[RuntimeCapability::CooperativeExecution],
                thread,
                thread,
            ),
            Err(ExecutionLaneSelectionError::MissingCapability(
                RuntimeCapability::BlockingLanes
            ))
        );
    }

    fn select_execution_lane(
        requirements: &[ExecutionLaneRequirement],
        affinity: ProtectedFrameAffinity,
        capabilities: &[RuntimeCapability],
        origin: bray_platform::RuntimeThreadId,
        main: bray_platform::RuntimeThreadId,
    ) -> Result<ExecutionLane, ExecutionLaneSelectionError> {
        let source =
            super::super::native::BrayScheduler::new(capabilities.iter().copied(), main, 1)
                .expect("fixture scheduler backing is admitted");

        let descriptor = ProtectedFrameStateDescriptor::new(
            ProtectedFrameStateId::new(0),
            requirements.iter().copied(),
            [],
            [],
            affinity,
        );

        source
            .select(&descriptor, origin)
            .map_err(|error| match error {
                SchedulerError::LaneSelection(error) => error,
                error => panic!("fixture lane selection failed: {error:?}"),
            })
    }

    #[test]
    fn compatible_lanes_rotate_without_starving_main_work() {
        let _isolation = crate::native::test_runtime_isolation();
        let runtime = RuntimeThreadScope::enter().expect("fixture attachment is available");
        let thread = runtime.runtime().id();

        let scheduler = Scheduler::new(
            RuntimeCapability::ALL,
            thread,
            SchedulerLimits::new(nonzero(1), nonzero(8)),
        )
        .expect("fixture backing is available");

        let tasks: Vec<_> = (0..64)
            .map(|value| {
                TaskControlBlock::start(
                    crate::test_support::admit_task(),
                    TestFrame::completing(value),
                )
            })
            .collect();

        let registrations: Vec<_> = tasks
            .iter()
            .map(|task| register_task(&scheduler, task, thread))
            .collect();

        for registration in &registrations {
            registration
                .wake_handle()
                .wake()
                .expect("the admitted run accepts a wake");
        }

        let main_task = TaskControlBlock::start(
            crate::test_support::admit_task(),
            TestFrame::requiring([ExecutionLaneRequirement::MainThread], 64),
        );

        let main = register_task(&scheduler, &main_task, thread);

        main.wake_handle().wake().expect("main work accepts a wake");

        let lanes = [
            cooperative_lane(),
            ExecutionLane::new(
                ExecutionLanePlacement::MainThread(thread),
                ExecutionWorkload::Cooperative,
            ),
        ];

        let first = scheduler
            .wait_ready(runtime.runtime(), lanes, None)
            .expect("ready work is available")
            .expect("the first lane is ready");

        assert_eq!(first.task(), tasks[0].id());

        assert!(
            scheduler
                .take_ready(runtime.runtime(), [lanes[1]])
                .expect("quota is observable")
                .is_none()
        );

        drop(first);

        let second = scheduler
            .wait_ready(runtime.runtime(), lanes, None)
            .expect("ready work is available")
            .expect("main receives the next turn");

        assert_eq!(second.task(), main_task.id());
        drop(second);

        for task in &tasks[1..] {
            let ready = scheduler
                .take_ready(runtime.runtime(), [lanes[0]])
                .expect("the cooperative queue remains available")
                .expect("the queued run progresses");

            assert_eq!(ready.task(), task.id());
            drop(ready);
        }
    }

    #[test]
    fn another_attached_thread_cannot_drive_main_or_origin_work() {
        let _isolation = crate::native::test_runtime_isolation();
        let runtime = RuntimeThreadScope::enter().expect("fixture attachment is available");
        let thread = runtime.runtime().id();

        let scheduler = Scheduler::new(
            RuntimeCapability::ALL,
            thread,
            SchedulerLimits::new(nonzero(1), nonzero(8)),
        )
        .expect("fixture backing is available");

        let task =
            TaskControlBlock::start(crate::test_support::admit_task(), TestFrame::completing(1));

        for affinity in [
            ProtectedFrameAffinity::MainThread,
            ProtectedFrameAffinity::OriginThread,
        ] {
            let execution = task.execution_state();

            let descriptor = execution
                .descriptor()
                .clone()
                .with_execution_requirements([], affinity);

            let execution = crate::FrameExecutionState::new(execution.frame(), descriptor);

            let registration = scheduler
                .register_task(task.id(), execution, thread, task.cancellation_context())
                .expect("the exact-thread run is admitted");

            let lane = registration.lane().expect("the exact lane is selected");

            registration
                .wake_handle()
                .wake()
                .expect("the run becomes ready");

            let shared = scheduler.clone();

            std::thread::spawn(move || {
                let worker =
                    RuntimeThreadScope::enter().expect("another native attachment is available");

                assert_ne!(worker.runtime().id(), thread);

                assert!(
                    shared
                        .take_ready(worker.runtime(), [lane])
                        .expect("incompatible dispatch is rejected")
                        .is_none()
                );
            })
            .join()
            .expect("the incompatible consumer returns");

            let ready = scheduler
                .take_ready(runtime.runtime(), [lane])
                .expect("the owning attachment can drive")
                .expect("the wake remains queued");

            assert_eq!(ready.task(), task.id());
            drop(ready);
            drop(registration);
        }
    }

    #[test]
    fn combined_workloads_keep_both_requirements_through_dispatch() {
        let _isolation = crate::native::test_runtime_isolation();
        let runtime = RuntimeThreadScope::enter().expect("fixture attachment is available");
        let thread = runtime.runtime().id();

        let scheduler = Scheduler::new(
            RuntimeCapability::ALL,
            thread,
            SchedulerLimits::new(nonzero(1), nonzero(1)),
        )
        .expect("fixture backing is available");

        let task = TaskControlBlock::start(
            crate::test_support::admit_task(),
            TestFrame::requiring(
                [
                    ExecutionLaneRequirement::Blocking,
                    ExecutionLaneRequirement::Compute,
                ],
                1,
            ),
        );

        let registration = register_task(&scheduler, &task, thread);

        assert_eq!(
            registration
                .lane()
                .expect("both capabilities are provided")
                .workload(),
            ExecutionWorkload::BlockingCompute
        );

        registration
            .wake_handle()
            .wake()
            .expect("the admitted run accepts a wake");

        for workload in [
            ExecutionWorkload::Cooperative,
            ExecutionWorkload::Blocking,
            ExecutionWorkload::Compute,
        ] {
            assert!(
                scheduler
                    .take_ready(
                        runtime.runtime(),
                        [ExecutionLane::new(
                            ExecutionLanePlacement::Migratable,
                            workload
                        )]
                    )
                    .expect("individual lanes remain available")
                    .is_none()
            );
        }

        let ready = scheduler
            .take_ready(
                runtime.runtime(),
                [ExecutionLane::new(
                    ExecutionLanePlacement::Migratable,
                    ExecutionWorkload::BlockingCompute,
                )],
            )
            .expect("the combined lane is available")
            .expect("the run is ready");

        assert_eq!(ready.task(), task.id());

        assert_eq!(
            ready.execution().descriptor().lane_requirements(),
            &[
                ExecutionLaneRequirement::Blocking,
                ExecutionLaneRequirement::Compute
            ]
        );
    }

    #[test]
    fn concurrent_wake_and_dispatch_return_preserve_one_next_resume() {
        let _isolation = crate::native::test_runtime_isolation();
        let runtime = RuntimeThreadScope::enter().expect("fixture attachment is available");
        let scheduler = scheduler(runtime.runtime().id());

        let task =
            TaskControlBlock::start(crate::test_support::admit_task(), TestFrame::completing(1));

        let registration = register_task(&scheduler, &task, runtime.runtime().id());
        let wake = registration.wake_handle();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));

        wake.wake().expect("the first wake commits");

        std::thread::scope(|scope| {
            let barrier = std::sync::Arc::clone(&barrier);
            let worker_barrier = std::sync::Arc::clone(&barrier);

            scope.spawn(move || {
                for _ in 0..256 {
                    worker_barrier.wait();

                    wake.wake()
                        .expect("the retained registration accepts the racing wake");

                    worker_barrier.wait();
                }
            });

            let mut ready = scheduler
                .take_ready(runtime.runtime(), [cooperative_lane()])
                .expect("the first dispatch is available")
                .expect("the first run is ready");

            for _ in 0..256 {
                barrier.wait();
                drop(ready);
                barrier.wait();

                ready = scheduler
                    .take_ready(runtime.runtime(), [cooperative_lane()])
                    .expect("the remembered wake is available")
                    .expect("dispatch return cannot lose the wake");

                assert_eq!(ready.task(), task.id());

                assert!(
                    scheduler
                        .take_ready(runtime.runtime(), [cooperative_lane()])
                        .expect("exclusive ownership remains observable")
                        .is_none()
                );
            }
        });
    }

    #[test]
    fn outstanding_dispatch_retains_the_scheduler_and_its_binding() {
        let _isolation = crate::native::test_runtime_isolation();
        let runtime = RuntimeThreadScope::enter().expect("fixture attachment is available");
        let scheduler = scheduler(runtime.runtime().id());

        let task =
            TaskControlBlock::start(crate::test_support::admit_task(), TestFrame::completing(1));

        let registration = register_task(&scheduler, &task, runtime.runtime().id());
        let wake = registration.wake_handle();

        wake.wake().expect("the first wake commits");

        let ready = scheduler
            .take_ready(runtime.runtime(), [cooperative_lane()])
            .expect("the dispatch is available")
            .expect("the run is ready");

        drop(scheduler);

        assert_eq!(
            registration
                .lane()
                .expect("the callback retains the domain"),
            cooperative_lane()
        );

        assert!(
            !wake
                .wake()
                .expect("the callback retains its pending-wake target")
        );

        drop(ready);
        assert!(matches!(wake.wake(), Err(SchedulerError::UnknownTask(_))));
    }

    #[test]
    fn host_cancellation_commits_a_pending_wake_during_dispatch() {
        let _isolation = crate::native::test_runtime_isolation();
        let runtime = RuntimeThreadScope::enter().expect("fixture attachment is available");
        let scheduler = scheduler(runtime.runtime().id());

        let task = TaskControlBlock::start(
            crate::test_support::admit_task(),
            TestFrame::cancellation_aware(),
        );

        let registration = register_task(&scheduler, &task, runtime.runtime().id());

        registration
            .wake_handle()
            .wake()
            .expect("the initial wake commits");

        let ready = scheduler
            .take_ready(runtime.runtime(), [cooperative_lane()])
            .expect("the dispatch is available")
            .expect("the run is ready");

        task.cancellation_context().request();

        let snapshot = scheduler
            .snapshot()
            .expect("the running continuation is observable");

        assert_eq!(snapshot.tasks()[0].dispatch(), ScheduledTaskState::Running);

        assert_eq!(
            snapshot.tasks()[0].wake_cause(),
            Some(TaskWakeCause::Cancellation)
        );

        assert!(
            !registration
                .wake_handle()
                .wake()
                .expect("the pending cause coalesces")
        );

        drop(ready);

        let cancelled = scheduler
            .take_ready(runtime.runtime(), [cooperative_lane()])
            .expect("the cancellation wake is available")
            .expect("the cancelled run progresses");

        assert_eq!(cancelled.wake_cause(), TaskWakeCause::Cancellation);
    }
}
