use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use bray_platform::{MonotonicDeadline, NativeEvent, RuntimeThreadId};
use bray_runtime_model::{ProtectedFrameStateId, RuntimeCapability};

use crate::cancellation::CancellationWakeRegistration;
use crate::{
    CancellationContext, ExecutionLane, FrameExecutionState, SchedulerSnapshot, TaskId,
    TaskWakeCause,
};

use super::contract::{SchedulerError, SchedulerLimits};
use super::queue::{self, QueuedTask, ReadyQueue};
use super::dispatch::{pop_ready, queue_instant, scheduler_snapshot, select_state_lane};

/// Target-independent scheduler policy and ready-queue storage.
#[derive(Clone, Debug)]
pub struct Scheduler {
    pub(super) data: Arc<SchedulerData>,
}

#[derive(Debug)]
pub(super) struct SchedulerData {
    pub(super) capabilities: Arc<[RuntimeCapability]>,
    pub(super) main_thread: RuntimeThreadId,
    pub(super) limits: SchedulerLimits,
    pub(super) observe_queue_time: bool,
    pub(super) state: Mutex<SchedulerState>,
    pub(super) event: NativeEvent,
}

#[derive(Debug, Default)]
pub(super) struct SchedulerState {
    pub(super) tasks: BTreeMap<TaskId, RegisteredTask>,
    pub(super) queues: HashMap<ExecutionLane, ReadyQueue>,
    pub(super) timers: BTreeMap<MonotonicDeadline, BTreeMap<u64, TimerWake>>,
    next_timer: u64,
    pub(super) timer_count: usize,
    pub(super) running_count: usize,
}

#[derive(Debug)]
pub(super) struct RegisteredTask {
    pub(super) execution: FrameExecutionState,
    pub(super) origin: RuntimeThreadId,
    pub(super) cancellation: CancellationContext,
    pub(super) dispatch: DispatchState,
    pub(super) wake_count: u64,
    pub(super) native: Option<Arc<crate::native::NativeTaskServices>>,
    pub(super) queued: Option<QueuedTask>,
    pub(super) execution_slot: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DispatchState {
    Idle(ProtectedFrameStateId),
    Queued(ProtectedFrameStateId),
    Running {
        state: ProtectedFrameStateId,
        pending: Option<TaskWakeCause>,
    },
}

#[derive(Clone, Copy, Debug)]
pub(super) struct TimerWake {
    task: TaskId,
}

impl Scheduler {
    pub(crate) fn wake_waiters(&self) {
        let _ = self.data.event.wake_handle().wake();
    }

    /// Creates a scheduler from validated runtime capabilities and explicit limits.
    pub fn new(
        capabilities: impl IntoIterator<Item = RuntimeCapability>,
        main_thread: RuntimeThreadId,
        limits: SchedulerLimits,
    ) -> Self {
        Self::create(capabilities, main_thread, limits, false)
    }

    /// Creates a scheduler that records ready-queue timing for inspection.
    pub fn new_observed(
        capabilities: impl IntoIterator<Item = RuntimeCapability>,
        main_thread: RuntimeThreadId,
        limits: SchedulerLimits,
    ) -> Self {
        Self::create(capabilities, main_thread, limits, true)
    }

    fn create(
        capabilities: impl IntoIterator<Item = RuntimeCapability>,
        main_thread: RuntimeThreadId,
        limits: SchedulerLimits,
        observe_queue_time: bool,
    ) -> Self {
        let mut capabilities: Vec<_> = capabilities.into_iter().collect();

        capabilities.sort_unstable();
        capabilities.dedup();

        Self {
            data: Arc::new(SchedulerData {
                capabilities: capabilities.into(),
                main_thread,
                limits,
                observe_queue_time,
                state: Mutex::new(SchedulerState::default()),
                event: NativeEvent::new(),
            }),
        }
    }

    pub(crate) fn task_wake(&self, task: TaskId) -> TaskWakeHandle {
        TaskWakeHandle { task, scheduler: Arc::downgrade(&self.data) }
    }

    /// Registers one runtime-owned task without making it ready.
    pub fn register_task(
        &self,
        task: TaskId,
        execution: FrameExecutionState,
        origin: RuntimeThreadId,
        cancellation: &CancellationContext,
    ) -> Result<TaskRegistration, SchedulerError> {
        let initial_state = execution.state();
        let execution_origin = execution.origin().unwrap_or(origin);

        select_state_lane(&self.data, execution.descriptor(), execution_origin)?;

        {
            let mut state = self.lock_state()?;

            if state.tasks.contains_key(&task) {
                return Err(SchedulerError::TaskAlreadyRegistered(task));
            }

            let capacity = state.tasks.len().checked_add(1).expect("the registry length is representable");

            if state.queues.capacity() < capacity {
                let additional = capacity - state.queues.len();

                state.queues.try_reserve(additional).map_err(SchedulerError::ReadyStorageUnavailable)?;
            }

            state.tasks.insert(
                task,
                RegisteredTask {
                    execution,
                    origin,
                    // Dispatch release must observe cancellation after registration returns.
                    cancellation: cancellation.clone(),
                    dispatch: DispatchState::Idle(initial_state),
                    wake_count: 0,
                    native: None,
                    queued: None,
                    execution_slot: false,
                },
            );
        }

        let scheduler = Arc::downgrade(&self.data);

        let cancellation_wake = match cancellation
            .register_wake(|| Arc::new(move || wake_cancelled_task(&scheduler, task)))
        {
            Ok(registration) => registration,
            Err(error) => {
                unregister_task(&self.data, task);

                return Err(error.into());
            }
        };

        Ok(TaskRegistration {
            task,
            origin,
            scheduler: Arc::downgrade(&self.data),
            _cancellation_wake: cancellation_wake,
        })
    }

    /// Registers a timer that requests task dispatch after its deadline.
    pub fn register_timer(
        &self,
        deadline: MonotonicDeadline,
        wake: &TaskWakeHandle,
    ) -> Result<TimerRegistration, SchedulerError> {
        if !Weak::ptr_eq(&wake.scheduler, &Arc::downgrade(&self.data)) {
            return Err(SchedulerError::UnknownTask(wake.task));
        }

        let mut state = self.lock_state()?;

        if !state.tasks.contains_key(&wake.task) {
            return Err(SchedulerError::UnknownTask(wake.task));
        }

        if state.timer_count >= self.data.limits.timers().get() {
            return Err(SchedulerError::TimerCapacityReached);
        }

        let identity = state.next_timer;

        let Some(next_timer) = identity.checked_add(1) else {
            return Err(SchedulerError::TimerIdentityExhausted);
        };

        state.next_timer = next_timer;

        state
            .timers
            .entry(deadline)
            .or_default()
            .insert(identity, TimerWake { task: wake.task });

        state.timer_count += 1;

        drop(state);

        let registration = TimerRegistration {
            deadline,
            identity: Some(identity),
            scheduler: Arc::downgrade(&self.data),
        };

        if let Err(error) = self.data.event.wake_handle().wake() {
            drop(registration);

            return Err(error.into());
        }

        Ok(registration)
    }

    /// Takes the next task compatible with one exact worker lane.
    pub fn take_ready(&self, lane: ExecutionLane) -> Result<Option<ReadyTask>, SchedulerError> {
        let mut state = self.lock_state()?;

        self.promote_elapsed_timers(&mut state)?;

        Ok(pop_ready(&self.data, &mut state, lane))
    }

    /// Returns the current registered-task count.
    pub fn task_count(&self) -> Result<usize, SchedulerError> {
        self.lock_state().map(|state| state.tasks.len())
    }

    pub(crate) fn unregister_native_task(&self, task: TaskId) {
        unregister_task(&self.data, task);
    }

    pub(crate) fn install_native_task(
        &self,
        task: Arc<crate::native::NativeTaskServices>,
    ) -> Result<(), SchedulerError> {
        let mut state = self.lock_state()?;
        let registered = state.tasks.get_mut(&task.id()).ok_or(SchedulerError::UnknownTask(task.id()))?;

        assert!(registered.native.is_none(), "a registration accepts its native resources once");
        registered.native = Some(task);

        Ok(())
    }

    pub(crate) fn native_task(
        &self,
        task: TaskId,
    ) -> Result<Arc<crate::native::NativeTaskServices>, SchedulerError> {
        self.lock_state()?.tasks.get(&task)
            .and_then(|registered| registered.native.as_ref())
            .map(Arc::clone)
            .ok_or(SchedulerError::UnknownTask(task))
    }

    /// Captures registered tasks and timers in deterministic identity order.
    pub fn snapshot(&self) -> Result<SchedulerSnapshot, SchedulerError> {
        let state = self.lock_state()?;

        scheduler_snapshot(&self.data, &state)
    }

    pub(super) fn promote_elapsed_timers(
        &self,
        scheduler: &mut SchedulerState,
    ) -> Result<(), SchedulerError> {
        while let Some(deadline) = scheduler.timers.keys().next().copied() {
            if !deadline.has_elapsed() {
                break;
            }

            let Some(wakes) = scheduler.timers.remove(&deadline) else {
                continue;
            };

            let mut wakes = wakes.into_iter();

            while let Some((identity, wake)) = wakes.next() {
                match enqueue_task(&self.data, scheduler, wake.task, TaskWakeCause::Timer) {
                    Ok(_) | Err(SchedulerError::UnknownTask(_)) => {
                        scheduler.timer_count = scheduler.timer_count.saturating_sub(1);
                    }
                    Err(error) => {
                        let retained = scheduler.timers.entry(deadline).or_default();

                        retained.insert(identity, wake);
                        retained.extend(wakes);

                        return Err(error);
                    }
                }
            }
        }

        Ok(())
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

/// Cancellation-safe ownership of one pending scheduler timer.
#[derive(Debug)]
pub struct TimerRegistration {
    deadline: MonotonicDeadline,
    identity: Option<u64>,
    scheduler: Weak<SchedulerData>,
}

impl Drop for TimerRegistration {
    fn drop(&mut self) {
        let Some(identity) = self.identity.take() else {
            return;
        };

        let Some(scheduler) = self.scheduler.upgrade() else {
            return;
        };

        let mut state = scheduler
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let mut removed = false;

        if let Some(timers) = state.timers.get_mut(&self.deadline) {
            removed = timers.remove(&identity).is_some();

            if timers.is_empty() {
                state.timers.remove(&self.deadline);
            }
        }

        if removed {
            state.timer_count = state.timer_count.saturating_sub(1);
        }
    }
}

/// Registration whose lifetime keeps one task known to the scheduler.
#[derive(Debug)]
pub struct TaskRegistration {
    pub(super) task: TaskId,
    origin: RuntimeThreadId,
    pub(super) scheduler: Weak<SchedulerData>,
    _cancellation_wake: Option<CancellationWakeRegistration>,
}

impl TaskRegistration {
    /// Returns the registered task identity.
    pub const fn task(&self) -> TaskId {
        self.task
    }

    /// Returns a transferable handle that can make this task ready.
    pub fn wake_handle(&self) -> TaskWakeHandle {
        TaskWakeHandle {
            task: self.task,
            scheduler: self.scheduler.clone(),
        }
    }

    /// Returns the compatible execution lane for the registered activation state.
    pub fn lane(&self) -> Result<ExecutionLane, SchedulerError> {
        let Some(scheduler) = self.scheduler.upgrade() else {
            return Err(SchedulerError::UnknownTask(self.task));
        };

        let state = scheduler
            .state
            .lock()
            .map_err(|_| SchedulerError::SynchronizationPoisoned)?;

        let Some(task) = state.tasks.get(&self.task) else {
            return Err(SchedulerError::UnknownTask(self.task));
        };

        let origin = task.execution.origin().unwrap_or(task.origin);

        select_state_lane(&scheduler, task.execution.descriptor(), origin)
    }

    pub(crate) fn execution_lane(
        &self,
        execution: &FrameExecutionState,
    ) -> Result<ExecutionLane, SchedulerError> {
        let Some(scheduler) = self.scheduler.upgrade() else {
            return Err(SchedulerError::UnknownTask(self.task));
        };

        let origin = execution.origin().unwrap_or(self.origin);

        select_state_lane(&scheduler, execution.descriptor(), origin)
    }
}

impl Drop for TaskRegistration {
    fn drop(&mut self) {
        if let Some(scheduler) = self.scheduler.upgrade() {
            unregister_task(&scheduler, self.task);
        }
    }
}

fn unregister_task(scheduler: &SchedulerData, task: TaskId) {
    let removed = {
        let mut state = scheduler.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);

        queue::remove(&mut state, task);

        let removed = state.tasks.remove(&task);

        if removed.as_ref().is_some_and(|task| task.execution_slot) {
            state.running_count -= 1;
        }

        let mut removed_timers = 0;

        state.timers.retain(|_, wakes| {
            let previous = wakes.len();

            wakes.retain(|_, wake| wake.task != task);
            removed_timers += previous - wakes.len();

            !wakes.is_empty()
        });

        state.timer_count = state.timer_count.saturating_sub(removed_timers);

        removed
    };

    // Native resources include their registration. Drop them after releasing
    // the registry lock so final resource cleanup can reenter the scheduler.
    drop(removed);

    let _ = scheduler.event.wake_handle().wake();
}

/// Transferable authority to request dispatch of one registered task.
#[derive(Clone, Debug)]
pub struct TaskWakeHandle {
    task: TaskId,
    scheduler: Weak<SchedulerData>,
}

impl TaskWakeHandle {
    /// Requests dispatch at the task's saved suspension state.
    ///
    /// Returns whether this call added a new ready-queue entry.
    pub fn wake(&self) -> Result<bool, SchedulerError> {
        self.wake_with_cause(TaskWakeCause::Explicit)
    }

    pub(crate) fn wake_cancellation(&self) -> Result<bool, SchedulerError> {
        self.wake_with_cause(TaskWakeCause::Cancellation)
    }

    fn wake_with_cause(&self, cause: TaskWakeCause) -> Result<bool, SchedulerError> {
        let Some(scheduler) = self.scheduler.upgrade() else {
            return Err(SchedulerError::UnknownTask(self.task));
        };

        let mut state = scheduler
            .state
            .lock()
            .map_err(|_| SchedulerError::SynchronizationPoisoned)?;

        let changed = enqueue_task(&scheduler, &mut state, self.task, cause)?;

        drop(state);

        if changed {
            scheduler.event.wake_handle().wake()?;
        }

        Ok(changed)
    }
}

fn wake_cancelled_task(scheduler: &Weak<SchedulerData>, task_id: TaskId) {
    let Some(scheduler) = scheduler.upgrade() else {
        return;
    };

    let Ok(mut state) = scheduler.state.lock() else {
        return;
    };

    let Some(DispatchState::Idle(_)) = state.tasks.get(&task_id).map(|task| task.dispatch) else {
        return;
    };

    let Ok(changed) = enqueue_task(&scheduler, &mut state, task_id, TaskWakeCause::Cancellation)
    else {
        return;
    };

    drop(state);

    if changed {
        let _ = scheduler.event.wake_handle().wake();
    }
}

/// One ready dispatch borrowed from a scheduler lane.
#[derive(Debug)]
pub struct ReadyTask {
    pub(super) task: TaskId,
    pub(super) execution: FrameExecutionState,
    pub(super) lane: ExecutionLane,
    pub(super) wake_cause: TaskWakeCause,
    pub(super) queue_latency: Option<Duration>,
    pub(super) scheduler: Weak<SchedulerData>,
    pub(super) released: bool,
    pub(super) native: Option<Arc<crate::native::NativeTaskServices>>,
}

impl ReadyTask {
    pub(crate) fn take_native_task(&mut self) -> Option<Arc<crate::native::NativeTaskServices>> {
        self.native.take()
    }

    /// Returns the task selected for this dispatch.
    pub const fn task(&self) -> TaskId {
        self.task
    }

    /// Returns the protected-frame state selected for resumption.
    pub const fn state(&self) -> ProtectedFrameStateId {
        self.execution.state()
    }

    /// Returns the active frame identity and checked execution metadata.
    pub const fn execution_state(&self) -> &FrameExecutionState {
        &self.execution
    }

    /// Returns the exact compatible lane that produced this dispatch.
    pub const fn lane(&self) -> ExecutionLane {
        self.lane
    }

    /// Returns why this dispatch became ready.
    pub const fn wake_cause(&self) -> TaskWakeCause {
        self.wake_cause
    }

    /// Returns queue latency when scheduler observation is enabled.
    pub const fn queue_latency(&self) -> Option<Duration> {
        self.queue_latency
    }

    /// Releases this dispatch at the frame's newly suspended state.
    pub fn suspend(
        mut self,
        execution: FrameExecutionState,
    ) -> Result<ExecutionLane, SchedulerError> {
        let Some(scheduler) = self.scheduler.upgrade() else {
            return Err(SchedulerError::UnknownTask(self.task));
        };

        let mut state = scheduler
            .state
            .lock()
            .map_err(|_| SchedulerError::SynchronizationPoisoned)?;

        let (changed, lane) =
            release_dispatch(&scheduler, &mut state, self.task, self.state(), execution)?;

        self.released = true;

        drop(state);

        if changed {
            scheduler.event.wake_handle().wake()?;
        }

        Ok(lane)
    }
}

impl Drop for ReadyTask {
    fn drop(&mut self) {
        if self.released {
            return;
        }

        let Some(scheduler) = self.scheduler.upgrade() else {
            return;
        };

        let Ok(mut state) = scheduler.state.lock() else {
            return;
        };

        let Ok((changed, _)) = release_dispatch(
            &scheduler,
            &mut state,
            self.task,
            self.state(),
            self.execution.clone(),
        ) else {
            return;
        };

        drop(state);

        if changed {
            let _ = scheduler.event.wake_handle().wake();
        }
    }
}

fn release_dispatch(
    scheduler: &SchedulerData,
    state: &mut SchedulerState,
    task_id: TaskId,
    running_state: ProtectedFrameStateId,
    execution: FrameExecutionState,
) -> Result<(bool, ExecutionLane), SchedulerError> {
    let Some(task) = state.tasks.get(&task_id) else {
        return Err(SchedulerError::UnknownTask(task_id));
    };

    let DispatchState::Running {
        state: retained,
        pending,
    } = task.dispatch
    else {
        panic!("dispatch release requires the exclusively running task");
    };

    assert_eq!(
        retained, running_state,
        "dispatch release retains its original running state"
    );

    let origin = execution.origin().unwrap_or(task.origin);
    let lane = select_state_lane(scheduler, execution.descriptor(), origin)?;
    let suspended_state = execution.state();

    let next = pending.or_else(|| {
        task.cancellation
            .recheck_dispatch()
            .then_some(TaskWakeCause::Cancellation)
    });

    let task = state.tasks.get_mut(&task_id).expect("the running task remains registered");

    assert!(task.execution_slot, "dispatch return reacquires its temporarily parked execution slot");

    task.execution_slot = false;
    state.running_count -= 1;

    if let Some(next) = next {
        queue::push(state, task_id, lane, suspended_state, next, queue_instant(scheduler));

        let Some(task) = state.tasks.get_mut(&task_id) else {
            return Err(SchedulerError::UnknownTask(task_id));
        };

        task.execution = execution;
        task.dispatch = DispatchState::Queued(suspended_state);
    } else {
        let Some(task) = state.tasks.get_mut(&task_id) else {
            return Err(SchedulerError::UnknownTask(task_id));
        };

        task.execution = execution;
        task.dispatch = DispatchState::Idle(suspended_state);
    }

    Ok((true, lane))
}

fn enqueue_task(
    scheduler: &SchedulerData,
    state: &mut SchedulerState,
    task_id: TaskId,
    cause: TaskWakeCause,
) -> Result<bool, SchedulerError> {
    let Some(task) = state.tasks.get(&task_id) else {
        return Err(SchedulerError::UnknownTask(task_id));
    };

    let state_id = match task.dispatch {
        DispatchState::Queued(_) => return Ok(false),
        DispatchState::Running {
            state: retained,
            pending,
        } => {
            if pending.is_some() {
                return Ok(false);
            }

            let Some(task) = state.tasks.get_mut(&task_id) else {
                return Err(SchedulerError::UnknownTask(task_id));
            };

            task.wake_count = task.wake_count.saturating_add(1);

            task.dispatch = DispatchState::Running {
                state: retained,
                pending: Some(cause),
            };

            return Ok(false);
        }
        DispatchState::Idle(state_id) => state_id,
    };

    let origin = task.execution.origin().unwrap_or(task.origin);
    let lane = select_state_lane(scheduler, task.execution.descriptor(), origin)?;

    queue::push(state, task_id, lane, state_id, cause, queue_instant(scheduler));

    let Some(task) = state.tasks.get_mut(&task_id) else {
        return Err(SchedulerError::UnknownTask(task_id));
    };

    task.wake_count = task.wake_count.saturating_add(1);
    task.dispatch = DispatchState::Queued(state_id);

    Ok(true)
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;
    use std::time::Duration;

    use crate::bootstrap::RuntimeThreadScope;
    use bray_platform::MonotonicClock;
    use bray_runtime_model::{ProtectedFrameStateId, RuntimeCapability};

    use super::{Scheduler, SchedulerError, SchedulerLimits};
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
                .take_ready(cooperative_lane())
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
        );

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
            .take_ready(cooperative_lane())
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
            .take_ready(lane)
            .unwrap_or_else(|error| panic!("ready queue must be available: {error:?}"))
            .unwrap_or_else(|| panic!("first task must be ready"));

        assert_eq!(first_ready.task(), first_task.id());

        drop(first_ready);

        let second_ready = scheduler
            .take_ready(lane)
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
            .take_ready(cooperative_lane())
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
            .take_ready(cooperative_lane())
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
            .take_ready(lane)
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
                .take_ready(lane)
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
            .take_ready(lane)
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
            .take_ready(lane)
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
            .take_ready(cooperative_lane())
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

        let deadline = MonotonicClock
            .deadline_after(Duration::ZERO)
            .unwrap_or_else(|| panic!("zero deadline must be representable"));

        let _timer = scheduler
            .register_timer(deadline, &registration.wake_handle())
            .unwrap_or_else(|error| panic!("timer must register: {error:?}"));

        let ready = scheduler
            .wait_ready(
                cooperative_lane(),
                MonotonicClock.deadline_after(Duration::from_secs(1)),
            )
            .unwrap_or_else(|error| panic!("timer wait must succeed: {error:?}"));

        assert_eq!(ready.as_ref().map(super::ReadyTask::task), Some(task.id()));
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

        let deadline = MonotonicClock
            .deadline_after(Duration::ZERO)
            .unwrap_or_else(|| panic!("zero deadline must be representable"));

        let _timer = scheduler
            .register_timer(deadline, &registration.wake_handle())
            .unwrap_or_else(|error| panic!("timer must register: {error:?}"));

        let ready = scheduler
            .take_ready(cooperative_lane())
            .unwrap_or_else(|error| panic!("ready queue must be available: {error:?}"))
            .unwrap_or_else(|| panic!("task must remain ready"));

        assert_eq!(ready.task(), task.id());
        drop(ready);

        let state = scheduler
            .data
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        assert_eq!(state.timer_count, 0);
        assert!(!state.timers.contains_key(&deadline));
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

        let deadline = MonotonicClock
            .deadline_after(Duration::from_secs(60))
            .unwrap_or_else(|| panic!("timer deadline must be representable"));

        let timer = scheduler
            .register_timer(deadline, &registration.wake_handle())
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
            [RuntimeCapability::CooperativeExecution, RuntimeCapability::MigratableLanes],
            runtime.runtime().id(),
            SchedulerLimits::new(nonzero(1), nonzero(1)),
        );

        let first = TaskControlBlock::start(crate::test_support::admit_task(), TestFrame::completing(1));
        let second = TaskControlBlock::start(crate::test_support::admit_task(), TestFrame::completing(2));
        let first_registration = register_task(&scheduler, &first, runtime.runtime().id());
        let second_registration = register_task(&scheduler, &second, runtime.runtime().id());

        first_registration.wake_handle().wake().expect("first task wakes");
        second_registration.wake_handle().wake().expect("second task wakes");

        assert_eq!(scheduler.task_count().expect("registry remains available"), 2);

        let running = scheduler.take_ready(cooperative_lane()).expect("first dispatch succeeds")
            .expect("first task is ready");

        assert_eq!(running.task(), first.id());
        assert!(scheduler.take_ready(cooperative_lane()).expect("quota check succeeds").is_none());

        drop(running);

        assert_eq!(scheduler.take_ready(cooperative_lane()).expect("released quota admits work")
            .expect("queued second task is ready").task(), second.id());
    }

    #[test]
    fn parked_dispatch_lends_capacity_without_releasing_its_exclusive_claim() {
        let _isolation = crate::native::test_runtime_isolation();
        let runtime = RuntimeThreadScope::enter().expect("fixture thread enters");

        let scheduler = Scheduler::new(
            [RuntimeCapability::CooperativeExecution, RuntimeCapability::MigratableLanes],
            runtime.runtime().id(),
            SchedulerLimits::new(nonzero(1), nonzero(1)),
        );

        let first = TaskControlBlock::start(crate::test_support::admit_task(), TestFrame::completing(1));
        let second = TaskControlBlock::start(crate::test_support::admit_task(), TestFrame::completing(2));
        let first_registration = register_task(&scheduler, &first, runtime.runtime().id());
        let second_registration = register_task(&scheduler, &second, runtime.runtime().id());

        first_registration.wake_handle().wake().expect("first task wakes");
        second_registration.wake_handle().wake().expect("second task wakes");

        let first_dispatch = scheduler.take_ready(cooperative_lane()).expect("first dispatch succeeds")
            .expect("first task is ready");

        let parked = first_registration.park_execution().expect("the running dispatch owns capacity");

        assert!(first_registration.park_execution().is_none());
        assert!(!first_registration.wake_handle().wake().expect("wake retains exclusive claim"));

        let second_dispatch = scheduler.take_ready(cooperative_lane()).expect("parked capacity admits work")
            .expect("second task is ready");

        assert_eq!(second_dispatch.task(), second.id());
        assert!(scheduler.take_ready(cooperative_lane()).expect("quota remains bounded").is_none());

        drop(second_dispatch);
        drop(parked);

        assert_eq!(scheduler.lock_state().expect("quota remains observable").running_count, 1);
        assert!(scheduler.take_ready(cooperative_lane()).expect("the parent retains capacity").is_none());

        drop(first_dispatch);

        assert_eq!(scheduler.take_ready(cooperative_lane()).expect("parent wake is preserved")
            .expect("the parent can run after dispatch return").task(), first.id());
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
        );

        let task =
            TaskControlBlock::start(crate::test_support::admit_task(), TestFrame::completing(1));

        let registration = register_task(&scheduler, &task, runtime.runtime().id());

        registration
            .wake_handle()
            .wake()
            .unwrap_or_else(|error| panic!("task must wake: {error:?}"));

        let deadline = MonotonicClock
            .deadline_after(Duration::from_secs(60))
            .unwrap_or_else(|| panic!("timer deadline must be representable"));

        let _timer = scheduler
            .register_timer(deadline, &registration.wake_handle())
            .unwrap_or_else(|error| panic!("timer must register: {error:?}"));

        drop(registration);

        let state = scheduler
            .data
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        assert_eq!(state.timer_count, 0);
        assert!(state.queues.is_empty());
        assert!(state.timers.is_empty());
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
}
