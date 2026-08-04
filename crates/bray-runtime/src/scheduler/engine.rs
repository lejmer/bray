use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use bray_platform::{
    MonotonicDeadline, MonotonicInstant, NativeEvent, NativeWaitOutcome, RuntimeThreadId,
};
use bray_runtime_interface::{ProtectedFrameDescriptor, ProtectedFrameStateId, RuntimeCapability};

use crate::cancellation::CancellationWakeRegistration;
use crate::lane::select_execution_lane;
use crate::{
    CancellationContext, ExecutionLane, FrameSuspension, SchedulerSnapshot, TaskId, TaskWakeCause,
};

use super::contract::{SchedulerError, SchedulerLimits};
use super::dispatch::{
    earliest_deadline, pop_ready, queue_instant, scheduler_snapshot, select_task_lane,
};

/// Target-independent scheduler policy and ready-queue storage.
#[derive(Clone, Debug)]
pub struct Scheduler {
    data: Arc<SchedulerData>,
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
    pub(super) queues: BTreeMap<ExecutionLane, VecDeque<QueuedTask>>,
    timers: BTreeMap<MonotonicDeadline, BTreeMap<u64, TimerWake>>,
    next_timer: u64,
    pub(super) timer_count: usize,
}

#[derive(Debug)]
pub(super) struct RegisteredTask {
    pub(super) descriptor: ProtectedFrameDescriptor,
    pub(super) origin: RuntimeThreadId,
    pub(super) cancellation: CancellationContext,
    pub(super) dispatch: DispatchState,
    pub(super) wake_count: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DispatchState {
    Idle(ProtectedFrameStateId),
    Queued(ProtectedFrameStateId),
    Running {
        state: ProtectedFrameStateId,
        pending: Option<PendingWake>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct PendingWake {
    pub(super) state: ProtectedFrameStateId,
    pub(super) lane: ExecutionLane,
    pub(super) cause: TaskWakeCause,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct QueuedTask {
    pub(super) task: TaskId,
    pub(super) state: ProtectedFrameStateId,
    pub(super) cause: TaskWakeCause,
    pub(super) queued_at: Option<MonotonicInstant>,
}

#[derive(Clone, Copy, Debug)]
struct TimerWake {
    task: TaskId,
    state: ProtectedFrameStateId,
}

impl Scheduler {
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

    /// Registers one runtime-owned task without making it ready.
    pub fn register_task(
        &self,
        task: TaskId,
        descriptor: ProtectedFrameDescriptor,
        origin: RuntimeThreadId,
        initial_state: ProtectedFrameStateId,
        cancellation: &CancellationContext,
    ) -> Result<TaskRegistration, SchedulerError> {
        let Some(frame_state) = descriptor.state(initial_state) else {
            return Err(SchedulerError::UnknownFrameState(initial_state));
        };

        select_execution_lane(
            frame_state.lane_requirements(),
            frame_state.affinity(),
            &self.data.capabilities,
            origin,
            self.data.main_thread,
        )?;

        {
            let mut state = self.lock_state()?;

            if state.tasks.contains_key(&task) {
                return Err(SchedulerError::TaskAlreadyRegistered(task));
            }

            if state.tasks.len() >= self.data.limits.tasks().get() {
                return Err(SchedulerError::TaskCapacityReached);
            }

            state.tasks.insert(
                task,
                RegisteredTask {
                    descriptor,
                    origin,
                    // Dispatch release must observe cancellation after registration returns.
                    cancellation: cancellation.clone(),
                    dispatch: DispatchState::Idle(initial_state),
                    wake_count: 0,
                },
            );
        }

        let scheduler = Arc::downgrade(&self.data);

        let cancellation_wake = match cancellation
            .register_wake(Arc::new(move || wake_cancelled_task(&scheduler, task)))
        {
            Ok(registration) => registration,
            Err(error) => {
                self.lock_state()?.tasks.remove(&task);

                return Err(error.into());
            }
        };

        Ok(TaskRegistration {
            task,
            scheduler: Arc::downgrade(&self.data),
            _cancellation_wake: cancellation_wake,
        })
    }

    /// Registers a timer that wakes one task state after its deadline.
    pub fn register_timer(
        &self,
        deadline: MonotonicDeadline,
        wake: &TaskWakeHandle,
        state_id: ProtectedFrameStateId,
    ) -> Result<TimerRegistration, SchedulerError> {
        if !Weak::ptr_eq(&wake.scheduler, &Arc::downgrade(&self.data)) {
            return Err(SchedulerError::UnknownTask(wake.task));
        }

        let mut state = self.lock_state()?;

        let Some(task) = state.tasks.get(&wake.task) else {
            return Err(SchedulerError::UnknownTask(wake.task));
        };

        select_task_lane(&self.data, task, state_id)?;

        if state.timer_count >= self.data.limits.timers().get() {
            return Err(SchedulerError::TimerCapacityReached);
        }

        let identity = state.next_timer;

        let Some(next_timer) = identity.checked_add(1) else {
            return Err(SchedulerError::TimerIdentityExhausted);
        };

        state.next_timer = next_timer;

        state.timers.entry(deadline).or_default().insert(
            identity,
            TimerWake {
                task: wake.task,
                state: state_id,
            },
        );

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

    /// Waits for the next compatible task or the supplied deadline.
    pub fn wait_ready(
        &self,
        lane: ExecutionLane,
        deadline: Option<MonotonicDeadline>,
    ) -> Result<Option<ReadyTask>, SchedulerError> {
        loop {
            let observed = self.data.event.observe()?;

            let wait_deadline = {
                let mut state = self.lock_state()?;

                self.promote_elapsed_timers(&mut state)?;

                if let Some(task) = pop_ready(&self.data, &mut state, lane) {
                    return Ok(Some(task));
                }

                earliest_deadline(deadline, state.timers.keys().next().copied())
            };

            match self.data.event.wait(observed, wait_deadline)? {
                NativeWaitOutcome::Woken(_) => {}
                NativeWaitOutcome::TimedOut(_) => {
                    let mut state = self.lock_state()?;

                    self.promote_elapsed_timers(&mut state)?;

                    if let Some(task) = pop_ready(&self.data, &mut state, lane) {
                        return Ok(Some(task));
                    }

                    if deadline.is_some_and(MonotonicDeadline::has_elapsed) {
                        return Ok(None);
                    }
                }
            }
        }
    }

    /// Returns the current registered-task count.
    pub fn task_count(&self) -> Result<usize, SchedulerError> {
        self.lock_state().map(|state| state.tasks.len())
    }

    /// Captures registered tasks and timers in deterministic identity order.
    pub fn snapshot(&self) -> Result<SchedulerSnapshot, SchedulerError> {
        let state = self.lock_state()?;

        scheduler_snapshot(&self.data, &state)
    }

    fn promote_elapsed_timers(&self, scheduler: &mut SchedulerState) -> Result<(), SchedulerError> {
        while let Some(deadline) = scheduler.timers.keys().next().copied() {
            if !deadline.has_elapsed() {
                break;
            }

            let Some(wakes) = scheduler.timers.remove(&deadline) else {
                continue;
            };

            let mut wakes = wakes.into_iter();

            while let Some((identity, wake)) = wakes.next() {
                match enqueue_task(
                    &self.data,
                    scheduler,
                    wake.task,
                    wake.state,
                    TaskWakeCause::Timer,
                ) {
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

    fn lock_state(&self) -> Result<std::sync::MutexGuard<'_, SchedulerState>, SchedulerError> {
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
    task: TaskId,
    scheduler: Weak<SchedulerData>,
    _cancellation_wake: CancellationWakeRegistration,
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

    /// Returns the compatible execution lane for one registered frame state.
    pub fn lane(&self, state_id: ProtectedFrameStateId) -> Result<ExecutionLane, SchedulerError> {
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

        select_task_lane(&scheduler, task, state_id)
    }
}

impl Drop for TaskRegistration {
    fn drop(&mut self) {
        let Some(scheduler) = self.scheduler.upgrade() else {
            return;
        };

        let Ok(mut state) = scheduler.state.lock() else {
            return;
        };

        state.tasks.remove(&self.task);

        for queue in state.queues.values_mut() {
            queue.retain(|queued| queued.task != self.task);
        }

        let mut removed_timers = 0;

        state.timers.retain(|_, wakes| {
            let previous = wakes.len();

            wakes.retain(|_, wake| wake.task != self.task);
            removed_timers += previous - wakes.len();

            !wakes.is_empty()
        });

        state.timer_count = state.timer_count.saturating_sub(removed_timers);
    }
}

/// Transferable authority to make one registered task state ready.
#[derive(Clone, Debug)]
pub struct TaskWakeHandle {
    task: TaskId,
    scheduler: Weak<SchedulerData>,
}

impl TaskWakeHandle {
    /// Makes one protected-frame state ready for compatible execution.
    ///
    /// Returns whether this call added a new ready-queue entry.
    pub fn wake(&self, state_id: ProtectedFrameStateId) -> Result<bool, SchedulerError> {
        let Some(scheduler) = self.scheduler.upgrade() else {
            return Err(SchedulerError::UnknownTask(self.task));
        };

        let mut state = scheduler
            .state
            .lock()
            .map_err(|_| SchedulerError::SynchronizationPoisoned)?;

        let changed = enqueue_task(
            &scheduler,
            &mut state,
            self.task,
            state_id,
            TaskWakeCause::Explicit,
        )?;

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

    let Some(DispatchState::Idle(state_id)) = state.tasks.get(&task_id).map(|task| task.dispatch)
    else {
        return;
    };

    let Ok(changed) = enqueue_task(
        &scheduler,
        &mut state,
        task_id,
        state_id,
        TaskWakeCause::Cancellation,
    ) else {
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
    pub(super) state: ProtectedFrameStateId,
    pub(super) lane: ExecutionLane,
    pub(super) wake_cause: TaskWakeCause,
    pub(super) queue_latency: Option<Duration>,
    pub(super) scheduler: Weak<SchedulerData>,
    pub(super) released: bool,
}

impl ReadyTask {
    /// Returns the task selected for this dispatch.
    pub const fn task(&self) -> TaskId {
        self.task
    }

    /// Returns the protected-frame state selected for resumption.
    pub const fn state(&self) -> ProtectedFrameStateId {
        self.state
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
    pub fn suspend(mut self, suspension: FrameSuspension) -> Result<(), SchedulerError> {
        let Some(scheduler) = self.scheduler.upgrade() else {
            return Err(SchedulerError::UnknownTask(self.task));
        };

        let mut state = scheduler
            .state
            .lock()
            .map_err(|_| SchedulerError::SynchronizationPoisoned)?;

        let changed = release_dispatch(
            &scheduler,
            &mut state,
            self.task,
            self.state,
            suspension.state(),
            true,
        )?;

        self.released = true;

        drop(state);

        if changed {
            scheduler.event.wake_handle().wake()?;
        }

        Ok(())
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

        let Ok(changed) = release_dispatch(
            &scheduler, &mut state, self.task, self.state, self.state, false,
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
    suspended_state: ProtectedFrameStateId,
    require_matching_pending: bool,
) -> Result<bool, SchedulerError> {
    let Some(task) = state.tasks.get(&task_id) else {
        return Err(SchedulerError::UnknownTask(task_id));
    };

    let DispatchState::Running {
        state: retained,
        pending,
    } = task.dispatch
    else {
        return Ok(false);
    };

    if retained != running_state {
        return Ok(false);
    }

    let lane = select_task_lane(scheduler, task, suspended_state)?;

    let next = match pending {
        Some(pending) if !require_matching_pending || pending.state == suspended_state => {
            Some(pending)
        }
        Some(pending) => {
            return Err(SchedulerError::ConflictingWakeState {
                retained: pending.state,
                requested: suspended_state,
            });
        }
        None if task.cancellation.is_requested() => Some(PendingWake {
            state: suspended_state,
            lane,
            cause: TaskWakeCause::Cancellation,
        }),
        None => None,
    };

    if let Some(next) = next {
        state
            .queues
            .entry(next.lane)
            .or_default()
            .push_back(QueuedTask {
                task: task_id,
                state: next.state,
                cause: next.cause,
                queued_at: queue_instant(scheduler),
            });

        let Some(task) = state.tasks.get_mut(&task_id) else {
            return Err(SchedulerError::UnknownTask(task_id));
        };

        task.dispatch = DispatchState::Queued(next.state);
    } else {
        let Some(task) = state.tasks.get_mut(&task_id) else {
            return Err(SchedulerError::UnknownTask(task_id));
        };

        task.dispatch = DispatchState::Idle(suspended_state);
    }

    Ok(next.is_some())
}

fn enqueue_task(
    scheduler: &SchedulerData,
    state: &mut SchedulerState,
    task_id: TaskId,
    state_id: ProtectedFrameStateId,
    cause: TaskWakeCause,
) -> Result<bool, SchedulerError> {
    let Some(task) = state.tasks.get(&task_id) else {
        return Err(SchedulerError::UnknownTask(task_id));
    };

    let lane = select_task_lane(scheduler, task, state_id)?;

    match task.dispatch {
        DispatchState::Queued(retained) => {
            if retained == state_id {
                return Ok(false);
            }

            return Err(SchedulerError::ConflictingWakeState {
                retained,
                requested: state_id,
            });
        }
        DispatchState::Running {
            state: retained,
            pending,
        } => {
            if let Some(pending) = pending {
                if retained == state_id || pending.state == state_id {
                    return Ok(false);
                }

                return Err(SchedulerError::ConflictingWakeState {
                    retained: pending.state,
                    requested: state_id,
                });
            }

            let Some(task) = state.tasks.get_mut(&task_id) else {
                return Err(SchedulerError::UnknownTask(task_id));
            };

            task.wake_count = task.wake_count.saturating_add(1);

            task.dispatch = DispatchState::Running {
                state: retained,
                pending: Some(PendingWake {
                    state: state_id,
                    lane,
                    cause,
                }),
            };

            return Ok(false);
        }
        DispatchState::Idle(retained) if retained != state_id => {
            return Err(SchedulerError::ConflictingWakeState {
                retained,
                requested: state_id,
            });
        }
        DispatchState::Idle(_) => {}
    }

    state.queues.entry(lane).or_default().push_back(QueuedTask {
        task: task_id,
        state: state_id,
        cause,
        queued_at: queue_instant(scheduler),
    });

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

    use bray_platform::{MonotonicClock, RuntimeThreadScope};
    use bray_runtime_interface::{ProtectedFrameStateId, RuntimeCapability};

    use super::{Scheduler, SchedulerError, SchedulerLimits};
    use crate::test_support::{TestFrame, register_task};
    use crate::{
        ExecutionLane, ExecutionLanePlacement, ExecutionWorkload, ScheduledTaskState,
        TaskControlBlock, TaskWakeCause,
    };

    #[test]
    fn observed_schedulers_publish_structured_queue_facts_on_demand() {
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

        let task = TaskControlBlock::start(TestFrame::completing(1))
            .unwrap_or_else(|error| panic!("test task must start: {error:?}"));

        let registration = register_task(&scheduler, &task, runtime.runtime().id());

        registration
            .wake_handle()
            .wake(ProtectedFrameStateId::new(0))
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
        let runtime = RuntimeThreadScope::enter()
            .unwrap_or_else(|error| panic!("runtime thread must initialize: {error:?}"));

        let scheduler = scheduler(runtime.runtime().id());

        let first_task = TaskControlBlock::start(TestFrame::completing(1))
            .unwrap_or_else(|error| panic!("first task must start: {error:?}"));

        let second_task = TaskControlBlock::start(TestFrame::completing(2))
            .unwrap_or_else(|error| panic!("second task must start: {error:?}"));

        let first = register_task(&scheduler, &first_task, runtime.runtime().id());

        let second = register_task(&scheduler, &second_task, runtime.runtime().id());

        first
            .wake_handle()
            .wake(ProtectedFrameStateId::new(0))
            .unwrap_or_else(|error| panic!("first task must wake: {error:?}"));

        second
            .wake_handle()
            .wake(ProtectedFrameStateId::new(0))
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
    fn duplicate_wakes_coalesce_and_running_wakes_are_not_lost() {
        let runtime = RuntimeThreadScope::enter()
            .unwrap_or_else(|error| panic!("runtime thread must initialize: {error:?}"));

        let scheduler = scheduler(runtime.runtime().id());

        let task = TaskControlBlock::start(TestFrame::completing(1))
            .unwrap_or_else(|error| panic!("test task must start: {error:?}"));

        let registration = register_task(&scheduler, &task, runtime.runtime().id());

        let wake = registration.wake_handle();

        assert!(
            wake.wake(ProtectedFrameStateId::new(0))
                .unwrap_or_else(|error| panic!("first wake must succeed: {error:?}"))
        );

        assert!(
            !wake
                .wake(ProtectedFrameStateId::new(0))
                .unwrap_or_else(|error| panic!("duplicate wake must succeed: {error:?}"))
        );

        let lane = cooperative_lane();

        let ready = scheduler
            .take_ready(lane)
            .unwrap_or_else(|error| panic!("ready queue must be available: {error:?}"))
            .unwrap_or_else(|| panic!("task must be ready"));

        assert!(
            !wake
                .wake(ProtectedFrameStateId::new(0))
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
    fn stale_running_wakes_do_not_replace_a_distinct_pending_state() {
        let runtime = RuntimeThreadScope::enter()
            .unwrap_or_else(|error| panic!("runtime thread must initialize: {error:?}"));

        let scheduler = scheduler(runtime.runtime().id());

        let task = TaskControlBlock::start(TestFrame::suspending_then_completing(1))
            .unwrap_or_else(|error| panic!("test task must start: {error:?}"));

        let registration = register_task(&scheduler, &task, runtime.runtime().id());

        let wake = registration.wake_handle();

        wake.wake(ProtectedFrameStateId::new(0))
            .unwrap_or_else(|error| panic!("initial wake must succeed: {error:?}"));

        let lane = cooperative_lane();

        let ready = scheduler
            .take_ready(lane)
            .unwrap_or_else(|error| panic!("ready queue must be available: {error:?}"))
            .unwrap_or_else(|| panic!("task must be ready"));

        wake.wake(ProtectedFrameStateId::new(1))
            .unwrap_or_else(|error| panic!("next-state wake must succeed: {error:?}"));

        wake.wake(ProtectedFrameStateId::new(0))
            .unwrap_or_else(|error| panic!("stale wake must coalesce: {error:?}"));

        drop(ready);

        let pending = scheduler
            .take_ready(lane)
            .unwrap_or_else(|error| panic!("pending wake must be available: {error:?}"))
            .unwrap_or_else(|| panic!("next state must be ready"));

        assert_eq!(pending.state(), ProtectedFrameStateId::new(1));
    }

    #[test]
    fn cancellation_makes_a_suspended_task_ready() {
        let runtime = RuntimeThreadScope::enter()
            .unwrap_or_else(|error| panic!("runtime thread must initialize: {error:?}"));

        let scheduler = scheduler(runtime.runtime().id());

        let task = TaskControlBlock::start(TestFrame::cancellation_aware())
            .unwrap_or_else(|error| panic!("test task must start: {error:?}"));

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
        let runtime = RuntimeThreadScope::enter()
            .unwrap_or_else(|error| panic!("runtime thread must initialize: {error:?}"));

        let scheduler = scheduler(runtime.runtime().id());

        let task = TaskControlBlock::start(TestFrame::completing(1))
            .unwrap_or_else(|error| panic!("test task must start: {error:?}"));

        let registration = register_task(&scheduler, &task, runtime.runtime().id());

        let deadline = MonotonicClock
            .deadline_after(Duration::ZERO)
            .unwrap_or_else(|| panic!("zero deadline must be representable"));

        let _timer = scheduler
            .register_timer(
                deadline,
                &registration.wake_handle(),
                ProtectedFrameStateId::new(0),
            )
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
    fn failed_timer_promotion_retains_the_failed_wake() {
        let runtime = RuntimeThreadScope::enter()
            .unwrap_or_else(|error| panic!("runtime thread must initialize: {error:?}"));

        let scheduler = scheduler(runtime.runtime().id());

        let task = TaskControlBlock::start(TestFrame::suspending_then_completing(1))
            .unwrap_or_else(|error| panic!("test task must start: {error:?}"));

        let registration = register_task(&scheduler, &task, runtime.runtime().id());

        registration
            .wake_handle()
            .wake(ProtectedFrameStateId::new(0))
            .unwrap_or_else(|error| panic!("task must wake: {error:?}"));

        let deadline = MonotonicClock
            .deadline_after(Duration::ZERO)
            .unwrap_or_else(|| panic!("zero deadline must be representable"));

        let _timer = scheduler
            .register_timer(
                deadline,
                &registration.wake_handle(),
                ProtectedFrameStateId::new(1),
            )
            .unwrap_or_else(|error| panic!("timer must register: {error:?}"));

        assert!(matches!(
            scheduler.take_ready(cooperative_lane()),
            Err(SchedulerError::ConflictingWakeState { .. })
        ));

        let state = scheduler
            .data
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        assert_eq!(state.timer_count, 1);

        assert_eq!(
            state.timers.get(&deadline).map(|timers| timers.len()),
            Some(1)
        );
    }

    #[test]
    fn dropping_timer_registration_withdraws_the_pending_wake() {
        let runtime = RuntimeThreadScope::enter()
            .unwrap_or_else(|error| panic!("runtime thread must initialize: {error:?}"));

        let scheduler = scheduler(runtime.runtime().id());

        let task = TaskControlBlock::start(TestFrame::completing(1))
            .unwrap_or_else(|error| panic!("test task must start: {error:?}"));

        let registration = register_task(&scheduler, &task, runtime.runtime().id());

        let deadline = MonotonicClock
            .deadline_after(Duration::from_secs(60))
            .unwrap_or_else(|| panic!("timer deadline must be representable"));

        let timer = scheduler
            .register_timer(
                deadline,
                &registration.wake_handle(),
                ProtectedFrameStateId::new(0),
            )
            .unwrap_or_else(|error| panic!("timer must register: {error:?}"));

        drop(timer);

        let snapshot = scheduler
            .snapshot()
            .unwrap_or_else(|error| panic!("scheduler must remain observable: {error:?}"));

        assert_eq!(snapshot.timer_count(), 0);
    }

    #[test]
    fn scheduler_limits_fail_without_partial_registration() {
        let runtime = RuntimeThreadScope::enter()
            .unwrap_or_else(|error| panic!("runtime thread must initialize: {error:?}"));

        let scheduler = Scheduler::new(
            [RuntimeCapability::CooperativeExecution],
            runtime.runtime().id(),
            SchedulerLimits::new(nonzero(1), nonzero(1)),
        );

        let first = TaskControlBlock::start(TestFrame::completing(1))
            .unwrap_or_else(|error| panic!("first task must start: {error:?}"));

        let second = TaskControlBlock::start(TestFrame::completing(2))
            .unwrap_or_else(|error| panic!("second task must start: {error:?}"));

        let _registration = register_task(&scheduler, &first, runtime.runtime().id());

        assert!(matches!(
            scheduler.register_task(
                second.id(),
                second.descriptor().clone(),
                runtime.runtime().id(),
                ProtectedFrameStateId::new(0),
                second.cancellation_context()
            ),
            Err(SchedulerError::TaskCapacityReached)
        ));

        assert_eq!(
            scheduler
                .task_count()
                .unwrap_or_else(|error| panic!("task count must be available: {error:?}")),
            1
        );
    }

    #[test]
    fn deregistration_reclaims_ready_and_timer_storage() {
        let runtime = RuntimeThreadScope::enter()
            .unwrap_or_else(|error| panic!("runtime thread must initialize: {error:?}"));

        let scheduler = Scheduler::new(
            [RuntimeCapability::CooperativeExecution],
            runtime.runtime().id(),
            SchedulerLimits::new(nonzero(1), nonzero(1)),
        );

        let task = TaskControlBlock::start(TestFrame::completing(1))
            .unwrap_or_else(|error| panic!("test task must start: {error:?}"));

        let registration = register_task(&scheduler, &task, runtime.runtime().id());

        registration
            .wake_handle()
            .wake(ProtectedFrameStateId::new(0))
            .unwrap_or_else(|error| panic!("task must wake: {error:?}"));

        let deadline = MonotonicClock
            .deadline_after(Duration::from_secs(60))
            .unwrap_or_else(|| panic!("timer deadline must be representable"));

        let _timer = scheduler
            .register_timer(
                deadline,
                &registration.wake_handle(),
                ProtectedFrameStateId::new(0),
            )
            .unwrap_or_else(|error| panic!("timer must register: {error:?}"));

        drop(registration);

        let state = scheduler
            .data
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        assert_eq!(state.timer_count, 0);
        assert!(state.queues.values().all(|queue| queue.is_empty()));
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
