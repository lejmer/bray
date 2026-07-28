use std::collections::{BTreeMap, VecDeque};
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex, Weak};

use bray_platform::{
    MonotonicDeadline, NativeEvent, NativeWaitOutcome, PlatformError, RuntimeThreadId,
};
use bray_runtime_interface::{ProtectedFrameDescriptor, ProtectedFrameStateId, RuntimeCapability};

use crate::cancellation::{CancellationWakeRegistration, CancellationWakeRegistrationError};
use crate::lane::select_execution_lane;
use crate::{
    CancellationContext, ExecutionLane, ExecutionLaneSelectionError, FrameSuspension, TaskId,
};

/// Hard scheduler capacities selected by the product host.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SchedulerLimits {
    tasks: NonZeroUsize,
    timers: NonZeroUsize,
}

impl SchedulerLimits {
    /// Creates explicit task and timer capacities.
    pub const fn new(tasks: NonZeroUsize, timers: NonZeroUsize) -> Self {
        Self { tasks, timers }
    }

    /// Returns the maximum registered task count.
    pub const fn tasks(self) -> NonZeroUsize {
        self.tasks
    }

    /// Returns the maximum pending timer count.
    pub const fn timers(self) -> NonZeroUsize {
        self.timers
    }
}

/// A scheduler operation that could not preserve its runtime contract.
#[derive(Debug)]
pub enum SchedulerError {
    /// The hard registered-task capacity was reached.
    TaskCapacityReached,
    /// The hard timer capacity was reached.
    TimerCapacityReached,
    /// The task identity is already registered.
    TaskAlreadyRegistered(TaskId),
    /// The task is no longer registered with this scheduler.
    UnknownTask(TaskId),
    /// A wake named a state absent from the task's protected-frame descriptor.
    UnknownFrameState(ProtectedFrameStateId),
    /// A queued or running task was woken for a different suspension state.
    ConflictingWakeState {
        /// State already retained by the scheduler.
        retained: ProtectedFrameStateId,
        /// State supplied by the conflicting wake.
        requested: ProtectedFrameStateId,
    },
    /// Checked requirements could not select a compatible runtime lane.
    LaneSelection(ExecutionLaneSelectionError),
    /// Cancellation-wake identities were exhausted.
    CancellationWakeIdentityExhausted,
    /// A native wait or wake mechanism failed.
    Platform(PlatformError),
    /// Scheduler state was poisoned by an unexpected runtime panic.
    SynchronizationPoisoned,
}

impl From<ExecutionLaneSelectionError> for SchedulerError {
    fn from(error: ExecutionLaneSelectionError) -> Self {
        Self::LaneSelection(error)
    }
}

impl From<CancellationWakeRegistrationError> for SchedulerError {
    fn from(_: CancellationWakeRegistrationError) -> Self {
        Self::CancellationWakeIdentityExhausted
    }
}

impl From<PlatformError> for SchedulerError {
    fn from(error: PlatformError) -> Self {
        Self::Platform(error)
    }
}

/// Target-independent scheduler policy and ready-queue storage.
#[derive(Clone, Debug)]
pub struct Scheduler {
    data: Arc<SchedulerData>,
}

#[derive(Debug)]
struct SchedulerData {
    capabilities: Arc<[RuntimeCapability]>,
    main_thread: RuntimeThreadId,
    limits: SchedulerLimits,
    state: Mutex<SchedulerState>,
    event: NativeEvent,
}

#[derive(Debug, Default)]
struct SchedulerState {
    tasks: BTreeMap<TaskId, RegisteredTask>,
    queues: BTreeMap<ExecutionLane, VecDeque<QueuedTask>>,
    timers: BTreeMap<MonotonicDeadline, Vec<TimerWake>>,
    timer_count: usize,
}

#[derive(Debug)]
struct RegisteredTask {
    descriptor: ProtectedFrameDescriptor,
    origin: RuntimeThreadId,
    cancellation: CancellationContext,
    dispatch: DispatchState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DispatchState {
    Idle(ProtectedFrameStateId),
    Queued(ProtectedFrameStateId),
    Running {
        state: ProtectedFrameStateId,
        pending: Option<PendingWake>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PendingWake {
    state: ProtectedFrameStateId,
    lane: ExecutionLane,
}

#[derive(Clone, Copy, Debug)]
struct QueuedTask {
    task: TaskId,
    state: ProtectedFrameStateId,
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
        let mut capabilities: Vec<_> = capabilities.into_iter().collect();

        capabilities.sort_unstable();
        capabilities.dedup();

        Self {
            data: Arc::new(SchedulerData {
                capabilities: capabilities.into(),
                main_thread,
                limits,
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

            if state.tasks.len() >= self.data.limits.tasks.get() {
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
                },
            );
        }

        let scheduler = Arc::downgrade(&self.data);

        let cancellation_wake =
            match cancellation.register_wake(Arc::new(move || wake_cancelled_task(&scheduler, task)))
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
    ) -> Result<(), SchedulerError> {
        if !Weak::ptr_eq(&wake.scheduler, &Arc::downgrade(&self.data)) {
            return Err(SchedulerError::UnknownTask(wake.task));
        }

        let mut state = self.lock_state()?;

        let Some(task) = state.tasks.get(&wake.task) else {
            return Err(SchedulerError::UnknownTask(wake.task));
        };

        select_task_lane(&self.data, task, state_id)?;

        if state.timer_count >= self.data.limits.timers.get() {
            return Err(SchedulerError::TimerCapacityReached);
        }

        state.timers.entry(deadline).or_default().push(TimerWake {
            task: wake.task,
            state: state_id,
        });

        state.timer_count += 1;

        drop(state);

        self.data.event.wake_handle().wake()?;

        Ok(())
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

    fn promote_elapsed_timers(&self, scheduler: &mut SchedulerState) -> Result<(), SchedulerError> {
        while let Some(deadline) = scheduler.timers.keys().next().copied() {
            if !deadline.has_elapsed() {
                break;
            }

            let Some(wakes) = scheduler.timers.remove(&deadline) else {
                continue;
            };

            let mut wakes = wakes.into_iter();

            while let Some(wake) = wakes.next() {
                match enqueue_task(&self.data, scheduler, wake.task, wake.state) {
                    Ok(_) | Err(SchedulerError::UnknownTask(_)) => {
                        scheduler.timer_count = scheduler.timer_count.saturating_sub(1);
                    }
                    Err(error) => {
                        let retained = scheduler.timers.entry(deadline).or_default();

                        retained.push(wake);
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
    pub fn lane(
        &self,
        state_id: ProtectedFrameStateId,
    ) -> Result<ExecutionLane, SchedulerError> {
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

            wakes.retain(|wake| wake.task != self.task);
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

        let changed = enqueue_task(&scheduler, &mut state, self.task, state_id)?;

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

    let Some(DispatchState::Idle(state_id)) =
        state.tasks.get(&task_id).map(|task| task.dispatch)
    else {
        return;
    };

    let Ok(changed) = enqueue_task(&scheduler, &mut state, task_id, state_id) else {
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
    task: TaskId,
    state: ProtectedFrameStateId,
    lane: ExecutionLane,
    scheduler: Weak<SchedulerData>,
    released: bool,
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
            &scheduler,
            &mut state,
            self.task,
            self.state,
            self.state,
            false,
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

            task.dispatch = DispatchState::Running {
                state: retained,
                pending: Some(PendingWake {
                    state: state_id,
                    lane,
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
    });

    let Some(task) = state.tasks.get_mut(&task_id) else {
        return Err(SchedulerError::UnknownTask(task_id));
    };

    task.dispatch = DispatchState::Queued(state_id);

    Ok(true)
}

fn select_task_lane(
    scheduler: &SchedulerData,
    task: &RegisteredTask,
    state_id: ProtectedFrameStateId,
) -> Result<ExecutionLane, SchedulerError> {
    let Some(frame_state) = task.descriptor.state(state_id) else {
        return Err(SchedulerError::UnknownFrameState(state_id));
    };

    select_execution_lane(
        frame_state.lane_requirements(),
        frame_state.affinity(),
        &scheduler.capabilities,
        task.origin,
        scheduler.main_thread,
    )
    .map_err(Into::into)
}

fn pop_ready(
    scheduler: &Arc<SchedulerData>,
    state: &mut SchedulerState,
    lane: ExecutionLane,
) -> Option<ReadyTask> {
    loop {
        let queued = state.queues.get_mut(&lane)?.pop_front()?;

        let Some(task) = state.tasks.get_mut(&queued.task) else {
            continue;
        };

        if task.dispatch != DispatchState::Queued(queued.state) {
            continue;
        }

        task.dispatch = DispatchState::Running {
            state: queued.state,
            pending: None,
        };

        return Some(ReadyTask {
            task: queued.task,
            state: queued.state,
            lane,
            scheduler: Arc::downgrade(scheduler),
            released: false,
        });
    }
}

fn earliest_deadline(
    first: Option<MonotonicDeadline>,
    second: Option<MonotonicDeadline>,
) -> Option<MonotonicDeadline> {
    match (first, second) {
        (Some(first), Some(second)) => Some(first.min(second)),
        (Some(deadline), None) | (None, Some(deadline)) => Some(deadline),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;
    use std::time::Duration;

    use bray_platform::{MonotonicClock, RuntimeThreadScope};
    use bray_runtime_interface::{ProtectedFrameStateId, RuntimeCapability};

    use super::{Scheduler, SchedulerError, SchedulerLimits};
    use crate::test_support::TestFrame;
    use crate::{ExecutionLane, ExecutionLanePlacement, ExecutionWorkload, TaskControlBlock};

    #[test]
    fn ready_tasks_are_distributed_in_stable_lane_order() {
        let runtime = RuntimeThreadScope::enter()
            .unwrap_or_else(|error| panic!("runtime thread must initialize: {error:?}"));

        let scheduler = scheduler(runtime.runtime().id());

        let first_task = TaskControlBlock::start(TestFrame::completing(1))
            .unwrap_or_else(|error| panic!("first task must start: {error:?}"));

        let second_task = TaskControlBlock::start(TestFrame::completing(2))
            .unwrap_or_else(|error| panic!("second task must start: {error:?}"));

        let first = scheduler
            .register_task(
                first_task.id(),
                first_task.descriptor().clone(),
                runtime.runtime().id(),
                ProtectedFrameStateId::new(0),
                first_task.cancellation_context(),
            )
            .unwrap_or_else(|error| panic!("first task must register: {error:?}"));

        let second = scheduler
            .register_task(
                second_task.id(),
                second_task.descriptor().clone(),
                runtime.runtime().id(),
                ProtectedFrameStateId::new(0),
                second_task.cancellation_context(),
            )
            .unwrap_or_else(|error| panic!("second task must register: {error:?}"));

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

        let registration = scheduler
            .register_task(
                task.id(),
                task.descriptor().clone(),
                runtime.runtime().id(),
                ProtectedFrameStateId::new(0),
                task.cancellation_context(),
            )
            .unwrap_or_else(|error| panic!("task must register: {error:?}"));

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

        let registration = scheduler
            .register_task(
                task.id(),
                task.descriptor().clone(),
                runtime.runtime().id(),
                ProtectedFrameStateId::new(0),
                task.cancellation_context(),
            )
            .unwrap_or_else(|error| panic!("task must register: {error:?}"));

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

        let _registration = scheduler
            .register_task(
                task.id(),
                task.descriptor().clone(),
                runtime.runtime().id(),
                ProtectedFrameStateId::new(0),
                task.cancellation_context(),
            )
            .unwrap_or_else(|error| panic!("task must register: {error:?}"));

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

        let registration = scheduler
            .register_task(
                task.id(),
                task.descriptor().clone(),
                runtime.runtime().id(),
                ProtectedFrameStateId::new(0),
                task.cancellation_context(),
            )
            .unwrap_or_else(|error| panic!("task must register: {error:?}"));

        let deadline = MonotonicClock
            .deadline_after(Duration::ZERO)
            .unwrap_or_else(|| panic!("zero deadline must be representable"));

        scheduler
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

        let registration = scheduler
            .register_task(
                task.id(),
                task.descriptor().clone(),
                runtime.runtime().id(),
                ProtectedFrameStateId::new(0),
                task.cancellation_context(),
            )
            .unwrap_or_else(|error| panic!("task must register: {error:?}"));

        registration
            .wake_handle()
            .wake(ProtectedFrameStateId::new(0))
            .unwrap_or_else(|error| panic!("task must wake: {error:?}"));

        let deadline = MonotonicClock
            .deadline_after(Duration::ZERO)
            .unwrap_or_else(|| panic!("zero deadline must be representable"));

        scheduler
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
        assert_eq!(state.timers.get(&deadline).map(Vec::len), Some(1));
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

        let _registration = scheduler
            .register_task(
                first.id(),
                first.descriptor().clone(),
                runtime.runtime().id(),
                ProtectedFrameStateId::new(0),
                first.cancellation_context(),
            )
            .unwrap_or_else(|error| panic!("first task must register: {error:?}"));

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

        let registration = scheduler
            .register_task(
                task.id(),
                task.descriptor().clone(),
                runtime.runtime().id(),
                ProtectedFrameStateId::new(0),
                task.cancellation_context(),
            )
            .unwrap_or_else(|error| panic!("task must register: {error:?}"));

        registration
            .wake_handle()
            .wake(ProtectedFrameStateId::new(0))
            .unwrap_or_else(|error| panic!("task must wake: {error:?}"));

        let deadline = MonotonicClock
            .deadline_after(Duration::from_secs(60))
            .unwrap_or_else(|| panic!("timer deadline must be representable"));

        scheduler
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
