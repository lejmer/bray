use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use bray_platform::{MonotonicDeadline, NativeEvent, RuntimeThreadId};
use bray_runtime_model::{ProtectedFrameDescriptor, ProtectedFrameStateId, RuntimeCapability};

use crate::cancellation::CancellationWakeRegistration;
use crate::{
    CancellationContext, ExecutionLane, FrameExecutionState, SchedulerSnapshot, TaskId,
    TaskWakeCause,
};

use super::contract::{SchedulerError, SchedulerLimits};
use super::dispatch::{
    pop_ready, queue_instant, scheduler_snapshot, select_state_lane, select_task_lane,
};
use super::ready::{QueuedTask, ReadyQueue, ReadySlotId, ReadySlots};

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
    pub(super) tasks: HashMap<TaskId, RegisteredTask>,
    pub(super) independent_tasks: usize,
    pub(super) cleanup_tasks: usize,
    pub(super) cleanup_lanes: usize,
    pub(super) pending_tasks: usize,
    pub(super) pending_lanes: usize,
    pub(super) queues: HashMap<ExecutionLane, ReadyQueue>,
    pub(super) ready: ReadySlots,
    pub(super) timers: BTreeMap<MonotonicDeadline, BTreeMap<u64, TimerWake>>,
    next_timer: u64,
    pub(super) timer_count: usize,
}

#[derive(Debug)]
pub(super) struct RegisteredTask {
    pub(super) admission: crate::task::TaskAdmissionKind,
    pub(super) descriptor: ProtectedFrameDescriptor,
    pub(super) execution: FrameExecutionState,
    pub(super) origin: RuntimeThreadId,
    pub(super) cancellation: CancellationContext,
    pub(super) dispatch: DispatchState,
    pub(super) wake_count: u64,
    pub(super) ready_lanes: Vec<ExecutionLane>,
    pub(super) ready_slot: ReadySlotId,
    pub(super) origin_lane: Option<ExecutionLane>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DispatchState {
    Idle(ProtectedFrameStateId),
    Terminal(ProtectedFrameStateId),
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
    pub(crate) fn main_thread(&self) -> Option<RuntimeThreadId> {
        self.data
            .capabilities
            .contains(&RuntimeCapability::MainThreadLane)
            .then_some(self.data.main_thread)
    }

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

    pub(crate) fn retains_thread(&self, thread: RuntimeThreadId) -> Result<bool, SchedulerError> {
        let state = self.lock_state()?;

        if state.tasks.values().any(|task| {
            task.execution.origin() == Some(thread)
                && !matches!(task.dispatch, DispatchState::Terminal(_))
        }) {
            return Ok(true);
        }

        for task in state.tasks.values().filter(|task| task.origin == thread) {
            for frame_state in task.descriptor.states() {
                let lane = select_task_lane(
                    &self.data,
                    &task.descriptor,
                    task.origin,
                    frame_state.state(),
                )?;

                if matches!(lane.placement(), crate::ExecutionLanePlacement::OriginThread(origin)
                    | crate::ExecutionLanePlacement::PinnedWorker(origin) if origin == thread)
                {
                    return Ok(true);
                }
            }
        }

        Ok(false)
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
    pub(super) scheduler: Weak<SchedulerData>,
    pub(super) _cancellation_wake: CancellationWakeRegistration,
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

        select_task_lane(&scheduler, &task.descriptor, task.origin, state_id)
    }

    /// Selects an admitted lane for active-frame metadata without publishing a transition.
    pub(crate) fn execution_lane(
        &self,
        execution: &FrameExecutionState,
    ) -> Result<ExecutionLane, SchedulerError> {
        let scheduler = self
            .scheduler
            .upgrade()
            .ok_or(SchedulerError::UnknownTask(self.task))?;

        let state = scheduler
            .state
            .lock()
            .map_err(|_| SchedulerError::SynchronizationPoisoned)?;

        let task = state
            .tasks
            .get(&self.task)
            .ok_or(SchedulerError::UnknownTask(self.task))?;

        admitted_execution_lane(&scheduler, &state, task, execution)
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

        state.remove_task(self.task);

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
    pub(super) task: TaskId,
    pub(super) scheduler: Weak<SchedulerData>,
}

impl TaskWakeHandle {
    pub(crate) fn wake_cancellation(&self) {
        wake_cancelled_task(&self.scheduler, self.task);
    }

    /// Requests dispatch at the task's saved suspension state.
    ///
    /// Returns whether this call added a new ready-queue entry.
    pub fn wake(&self) -> Result<bool, SchedulerError> {
        let Some(scheduler) = self.scheduler.upgrade() else {
            return Err(SchedulerError::UnknownTask(self.task));
        };

        let mut state = scheduler
            .state
            .lock()
            .map_err(|_| SchedulerError::SynchronizationPoisoned)?;

        let changed = enqueue_task(&scheduler, &mut state, self.task, TaskWakeCause::Explicit)?;

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
}

impl ReadyTask {
    /// Ends terminal execution while retaining the registered task's ownership metadata.
    pub fn complete(mut self, execution: FrameExecutionState) -> Result<(), SchedulerError> {
        let scheduler = self
            .scheduler
            .upgrade()
            .ok_or(SchedulerError::UnknownTask(self.task))?;

        let mut state = scheduler
            .state
            .lock()
            .map_err(|_| SchedulerError::SynchronizationPoisoned)?;

        let task = state
            .tasks
            .get_mut(&self.task)
            .ok_or(SchedulerError::UnknownTask(self.task))?;

        if !matches!(task.dispatch, DispatchState::Running { state, .. } if state == self.state()) {
            return Err(SchedulerError::TaskNotRunning(self.task));
        }

        task.dispatch = DispatchState::Terminal(execution.state());
        task.execution = execution;
        self.released = true;

        Ok(())
    }

    /// Returns the task selected for this dispatch.
    pub const fn task(&self) -> TaskId {
        self.task
    }

    /// Returns the protected-frame state selected for resumption.
    pub const fn state(&self) -> ProtectedFrameStateId {
        self.execution.state()
    }

    /// Returns the active frame identity and checked execution metadata for this dispatch.
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
    pub fn suspend(mut self, execution: FrameExecutionState) -> Result<(), SchedulerError> {
        let Some(scheduler) = self.scheduler.upgrade() else {
            return Err(SchedulerError::UnknownTask(self.task));
        };

        let mut state = scheduler
            .state
            .lock()
            .map_err(|_| SchedulerError::SynchronizationPoisoned)?;

        let changed = release_dispatch(&scheduler, &mut state, self.task, self.state(), execution)?;

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

    let lane = admitted_execution_lane(scheduler, state, task, &execution)?;
    let suspended_state = execution.state();
    let ready_slot = task.ready_slot;
    let previous_origin = task.origin_lane;
    let origin_lane = (!task.ready_lanes.contains(&lane)).then_some(lane);

    let next = pending.or_else(|| {
        task.cancellation
            .is_requested()
            .then_some(TaskWakeCause::Cancellation)
    });

    let reserve_origin = origin_lane.filter(|_| origin_lane != previous_origin);

    if let Some(lane) = reserve_origin {
        state
            .queues
            .get_mut(&lane)
            .ok_or(SchedulerError::MissingReadyQueue(lane))?
            .reserve()?;
    }

    if let Some(cause) = next {
        // Selection verified the queue under this same lock. No map insertion occurs here.
        let queue = state
            .queues
            .get_mut(&lane)
            .expect("validated lane retains its queue");

        if let Err(error) = state.ready.push(
            ready_slot,
            queue,
            lane,
            QueuedTask {
                task: task_id,
                state: suspended_state,
                cause,
                queued_at: queue_instant(scheduler),
            },
        ) {
            if let Some(lane) = reserve_origin {
                state.release_ready_queues(&[lane]);
            }

            return Err(error);
        }
    }

    // Exclusive scheduler state retains this registration throughout publication.
    let task = state
        .tasks
        .get_mut(&task_id)
        .expect("dispatch retains its registered task");

    task.execution = execution;
    task.origin_lane = origin_lane;

    task.dispatch = if next.is_some() {
        DispatchState::Queued(suspended_state)
    } else {
        DispatchState::Idle(suspended_state)
    };

    if let Some(previous) = previous_origin.filter(|_| previous_origin != origin_lane) {
        state.release_ready_queues(&[previous]);
    }

    Ok(next.is_some())
}

fn admitted_execution_lane(
    scheduler: &SchedulerData,
    state: &SchedulerState,
    task: &RegisteredTask,
    execution: &FrameExecutionState,
) -> Result<ExecutionLane, SchedulerError> {
    let lane = select_state_lane(
        scheduler,
        execution.descriptor(),
        execution.origin().unwrap_or(task.origin),
    )?;

    // An admitted migratable workload may become tied to its executing worker. This narrows
    // placement and uses headers secured by that worker, without granting another workload.
    let narrowed = execution.origin().is_some_and(|origin| {
        lane.placement() == crate::ExecutionLanePlacement::OriginThread(origin)
            && task.ready_lanes.contains(&ExecutionLane::new(
                crate::ExecutionLanePlacement::Migratable,
                lane.workload(),
            ))
    });

    if (!task.ready_lanes.contains(&lane) && !narrowed) || !state.queues.contains_key(&lane) {
        return Err(SchedulerError::MissingReadyQueue(lane));
    }

    Ok(lane)
}

pub(super) fn enqueue_task(
    scheduler: &SchedulerData,
    state: &mut SchedulerState,
    task_id: TaskId,
    cause: TaskWakeCause,
) -> Result<bool, SchedulerError> {
    let Some(task) = state.tasks.get(&task_id) else {
        return Err(SchedulerError::UnknownTask(task_id));
    };

    let state_id = match task.dispatch {
        DispatchState::Terminal(_) | DispatchState::Queued(_) => return Ok(false),
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
        DispatchState::Idle(state) => state,
    };

    let lane = admitted_execution_lane(scheduler, state, task, &task.execution)?;

    let ready_slot = task.ready_slot;

    let queue = state
        .queues
        .get_mut(&lane)
        .ok_or(SchedulerError::MissingReadyQueue(lane))?;

    state.ready.push(
        ready_slot,
        queue,
        lane,
        QueuedTask {
            task: task_id,
            state: state_id,
            cause,
            queued_at: queue_instant(scheduler),
        },
    )?;

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
    use bray_runtime_model::{ProtectedFrameStateId, RuntimeCapability};

    use super::{Scheduler, SchedulerError, SchedulerLimits};
    use crate::test_support::{TestFrame, register_task};
    use crate::{
        ExecutionLane, ExecutionLanePlacement, ExecutionWorkload, FrameExecutionState,
        FrameSuspensionKind, ScheduledTaskState, TaskControlBlock, TaskResumeStatus, TaskWakeCause,
    };

    #[test]
    fn terminal_dispatch_discards_pending_wakes_and_retains_ownership_metadata() {
        for cancellation_stage in 0..3 {
            let runtime = RuntimeThreadScope::enter().unwrap();
            let scheduler = scheduler(runtime.runtime().id());
            let task = TaskControlBlock::start(TestFrame::completing(1)).unwrap();
            let registration = register_task(&scheduler, &task, runtime.runtime().id());
            let wake = registration.wake_handle();

            if cancellation_stage == 0 {
                task.request_cancellation();
            }

            wake.wake().unwrap();
            let ready = scheduler.take_ready(cooperative_lane()).unwrap().unwrap();
            wake.wake().unwrap();

            if cancellation_stage == 1 {
                task.request_cancellation();
            }

            assert!(matches!(
                task.resume(),
                Ok(TaskResumeStatus::Terminal(_, _))
            ));

            {
                let execution = ready.execution_state().clone();

                ready.complete(execution)
            }
            .unwrap();

            if cancellation_stage == 2 {
                task.request_cancellation();
            }

            assert!(!wake.wake().unwrap());
            assert!(scheduler.take_ready(cooperative_lane()).unwrap().is_none());
            let snapshot = scheduler.snapshot().unwrap();
            assert_eq!(snapshot.tasks().len(), 1);
            assert_eq!(snapshot.tasks()[0].dispatch(), ScheduledTaskState::Terminal);
            assert!(task.has_unobserved_outcome().unwrap());

            drop(registration);
            assert!(scheduler.snapshot().unwrap().tasks().is_empty());
        }
    }

    #[test]
    fn observed_schedulers_publish_structured_queue_observations_on_demand() {
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
    fn registered_affinity_retains_its_origin_until_registration_is_released() {
        use bray_runtime_model::{
            ProtectedFrameAffinity, ProtectedFrameDescriptor, ProtectedFrameStateDescriptor,
        };

        let runtime = RuntimeThreadScope::enter().unwrap();
        let origin = runtime.runtime().id();

        let other = std::thread::spawn(|| RuntimeThreadScope::enter().unwrap().runtime().id())
            .join()
            .unwrap();

        for (affinity, migratable, expected) in [
            (ProtectedFrameAffinity::Movable, true, false),
            (ProtectedFrameAffinity::Movable, false, true),
            (ProtectedFrameAffinity::OriginThread, true, true),
            (ProtectedFrameAffinity::MainThread, true, false),
        ] {
            let mut capabilities = vec![
                RuntimeCapability::CooperativeExecution,
                RuntimeCapability::LocalLanes,
                RuntimeCapability::MainThreadLane,
            ];

            if migratable {
                capabilities.push(RuntimeCapability::MigratableLanes);
            }

            let scheduler = Scheduler::new(
                capabilities,
                origin,
                SchedulerLimits::new(nonzero(8), nonzero(8)),
            );

            let task = TaskControlBlock::start(TestFrame::completing(1)).unwrap();
            let source = task.descriptor();

            let descriptor = ProtectedFrameDescriptor::try_new(
                source.frame(),
                source.abi_version(),
                source.frame_abi(),
                source.layout(),
                source.completion_layout(),
                [ProtectedFrameStateDescriptor::new(
                    ProtectedFrameStateId::new(0),
                    [],
                    [],
                    [],
                    affinity,
                )],
            )
            .unwrap();

            assert!(!scheduler.retains_thread(origin).unwrap());

            let registration = scheduler
                .register_task(
                    task.id(),
                    descriptor,
                    origin,
                    ProtectedFrameStateId::new(0),
                    task.cancellation_context(),
                )
                .unwrap();

            assert_eq!(scheduler.retains_thread(origin).unwrap(), expected);
            assert!(!scheduler.retains_thread(other).unwrap());

            drop(registration);

            assert!(!scheduler.retains_thread(origin).unwrap());
        }
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
        let runtime = RuntimeThreadScope::enter()
            .unwrap_or_else(|error| panic!("runtime thread must initialize: {error:?}"));

        let scheduler = scheduler(runtime.runtime().id());

        let yielding = TaskControlBlock::start(TestFrame::yielding_then_completing(1))
            .unwrap_or_else(|error| panic!("yielding task must start: {error:?}"));

        let ready = TaskControlBlock::start(TestFrame::completing(2))
            .unwrap_or_else(|error| panic!("ready task must start: {error:?}"));

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
        let runtime = RuntimeThreadScope::enter()
            .unwrap_or_else(|error| panic!("runtime thread must initialize: {error:?}"));

        let scheduler = scheduler(runtime.runtime().id());

        let task = TaskControlBlock::start(TestFrame::completing(1))
            .unwrap_or_else(|error| panic!("test task must start: {error:?}"));

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
    fn running_wakes_use_the_newly_published_suspension_state() {
        let runtime = RuntimeThreadScope::enter()
            .unwrap_or_else(|error| panic!("runtime thread must initialize: {error:?}"));

        let scheduler = scheduler(runtime.runtime().id());

        let task = TaskControlBlock::start(TestFrame::suspending_then_completing(1))
            .unwrap_or_else(|error| panic!("test task must start: {error:?}"));

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
            .unwrap_or_else(|error| panic!("pending wake must succeed: {error:?}"));

        wake.wake()
            .unwrap_or_else(|error| panic!("stale wake must coalesce: {error:?}"));

        ready
            .suspend(FrameExecutionState::new(
                task.descriptor().frame(),
                task.descriptor()
                    .state(ProtectedFrameStateId::new(1))
                    .unwrap()
                    .clone(),
            ))
            .unwrap();

        let pending = scheduler
            .take_ready(lane)
            .unwrap_or_else(|error| panic!("pending wake must be available: {error:?}"))
            .unwrap_or_else(|| panic!("next state must be ready"));

        assert_eq!(pending.state(), ProtectedFrameStateId::new(1));
    }

    #[test]
    fn child_execution_uses_admitted_lanes_and_rejection_preserves_current_state() {
        use bray_runtime_model::{
            ProtectedAsyncFrameId, ProtectedFrameAffinity, ProtectedFrameStateDescriptor,
        };

        let runtime = RuntimeThreadScope::enter().unwrap();
        let thread = runtime.runtime().id();

        let scheduler = Scheduler::new(
            [
                RuntimeCapability::CooperativeExecution,
                RuntimeCapability::MigratableLanes,
                RuntimeCapability::MainThreadLane,
            ],
            thread,
            SchedulerLimits::new(nonzero(2), nonzero(1)),
        );

        let task = TaskControlBlock::start(TestFrame::completing(1)).unwrap();
        let registration = register_task(&scheduler, &task, thread);

        // Another task owns a main-thread queue, but this task has not admitted that lane.
        let other = TaskControlBlock::start(TestFrame::main_thread_then_movable(2)).unwrap();
        let _other_registration = register_task(&scheduler, &other, thread);

        let child = FrameExecutionState::new(
            ProtectedAsyncFrameId::new([91; 32]),
            ProtectedFrameStateDescriptor::new(
                ProtectedFrameStateId::new(42),
                [],
                [],
                [],
                ProtectedFrameAffinity::Movable,
            ),
        );

        assert!(task.descriptor().state(child.state()).is_none());

        assert_eq!(
            crate::test_support::with_allocation_failure(|| registration.execution_lane(&child))
                .unwrap(),
            cooperative_lane(),
        );

        let wake = registration.wake_handle();
        wake.wake().unwrap();
        let ready = scheduler.take_ready(cooperative_lane()).unwrap().unwrap();
        assert_eq!(ready.execution_state().frame(), task.descriptor().frame());
        assert_eq!(ready.state(), ProtectedFrameStateId::new(0));
        wake.wake().unwrap();
        crate::test_support::with_allocation_failure(|| ready.suspend(child.clone())).unwrap();

        let ready = scheduler.take_ready(cooperative_lane()).unwrap().unwrap();
        assert_eq!(ready.execution_state(), &child);
        assert_eq!(ready.state(), child.state());
        let snapshot = scheduler.snapshot().unwrap();

        let observed = snapshot
            .tasks()
            .iter()
            .find(|entry| entry.task() == task.id())
            .unwrap();

        assert_eq!(observed.execution(), &child);
        assert_eq!(observed.lane(), cooperative_lane());

        let disallowed = FrameExecutionState::new(
            child.frame(),
            ProtectedFrameStateDescriptor::new(
                ProtectedFrameStateId::new(43),
                [],
                [],
                [],
                ProtectedFrameAffinity::MainThread,
            ),
        );

        let main_lane = ExecutionLane::new(
            ExecutionLanePlacement::MainThread(thread),
            ExecutionWorkload::Cooperative,
        );

        assert!(
            scheduler
                .lock_state()
                .unwrap()
                .queues
                .contains_key(&main_lane)
        );

        assert!(matches!(
            crate::test_support::with_allocation_failure(|| registration.execution_lane(&disallowed)),
            Err(SchedulerError::MissingReadyQueue(lane)) if lane == main_lane
        ));

        {
            let state = scheduler.lock_state().unwrap();
            let registered = state.tasks.get(&task.id()).unwrap();
            assert_eq!(&registered.execution, &child);

            assert!(matches!(
                registered.dispatch,
                super::DispatchState::Running { pending: None, .. }
            ));
        }

        wake.wake().unwrap();

        assert!(matches!(ready.suspend(disallowed),
            Err(SchedulerError::MissingReadyQueue(lane)) if lane == main_lane));

        // Failed publication drops the dispatch with its original execution and pending wake.
        assert!(scheduler.take_ready(main_lane).unwrap().is_none());
        let ready = scheduler.take_ready(cooperative_lane()).unwrap().unwrap();
        assert_eq!(ready.execution_state(), &child);
        ready.suspend(child.clone()).unwrap();
        assert!(scheduler.take_ready(cooperative_lane()).unwrap().is_none());
        wake.wake().unwrap();
        let ready = scheduler.take_ready(cooperative_lane()).unwrap().unwrap();
        assert_eq!(ready.execution_state(), &child);
        let root = task.snapshot().unwrap().execution().clone();
        ready.complete(root.clone()).unwrap();
        let state = scheduler.lock_state().unwrap();
        let registered = state.tasks.get(&task.id()).unwrap();
        assert_eq!(&registered.execution, &root);

        assert_eq!(
            registered.dispatch,
            super::DispatchState::Terminal(root.state())
        );
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
    fn timer_promotion_coalesces_with_a_queued_notification() {
        let runtime = RuntimeThreadScope::enter()
            .unwrap_or_else(|error| panic!("runtime thread must initialize: {error:?}"));

        let scheduler = scheduler(runtime.runtime().id());

        let task = TaskControlBlock::start(TestFrame::suspending_then_completing(1))
            .unwrap_or_else(|error| panic!("test task must start: {error:?}"));

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

        let ready = scheduler.take_ready(cooperative_lane()).unwrap().unwrap();
        assert_eq!(ready.state(), ProtectedFrameStateId::new(0));
        assert_eq!(ready.wake_cause(), crate::TaskWakeCause::Explicit);

        let state = scheduler
            .data
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        assert_eq!(state.timer_count, 0);

        assert_eq!(state.timers.get(&deadline).map(|timers| timers.len()), None);
    }

    #[test]
    fn failed_timer_promotion_retains_the_failed_notification() {
        let runtime = RuntimeThreadScope::enter().unwrap();
        let scheduler = scheduler(runtime.runtime().id());
        let task = TaskControlBlock::start(TestFrame::completing(1)).unwrap();
        let registration = register_task(&scheduler, &task, runtime.runtime().id());
        let deadline = MonotonicClock.deadline_after(Duration::ZERO).unwrap();

        let _timer = scheduler
            .register_timer(deadline, &registration.wake_handle())
            .unwrap();

        let lane = cooperative_lane();

        let queue = scheduler
            .lock_state()
            .unwrap()
            .queues
            .remove(&lane)
            .unwrap();

        assert!(
            matches!(scheduler.take_ready(lane), Err(SchedulerError::MissingReadyQueue(found)) if found == lane)
        );

        {
            let mut state = scheduler.lock_state().unwrap();
            assert_eq!(state.timer_count, 1);
            assert_eq!(state.timers.get(&deadline).unwrap().len(), 1);
            state.queues.insert(lane, queue);
        }

        let ready = scheduler.take_ready(lane).unwrap().unwrap();
        assert_eq!(ready.task(), task.id());
        assert_eq!(ready.wake_cause(), crate::TaskWakeCause::Timer);
        assert_eq!(scheduler.lock_state().unwrap().timer_count, 0);

        {
            let execution = ready.execution_state().clone();

            ready.complete(execution)
        }
        .unwrap();
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
            .register_timer(deadline, &registration.wake_handle())
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

        let registration = register_task(&scheduler, &first, runtime.runtime().id());

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

        let continuation = scheduler
            .register_admitted_task(
                second.id(),
                second.descriptor().clone(),
                runtime.runtime().id(),
                ProtectedFrameStateId::new(0),
                second.cancellation_context(),
                crate::task::TaskAdmissionKind::Continuation,
            )
            .unwrap();

        assert_eq!(scheduler.task_count().unwrap(), 2);
        drop(continuation);

        assert!(matches!(
            scheduler.register_task(
                second.id(),
                second.descriptor().clone(),
                runtime.runtime().id(),
                ProtectedFrameStateId::new(0),
                second.cancellation_context(),
            ),
            Err(SchedulerError::TaskCapacityReached)
        ));

        drop(registration);

        let _replacement = register_task(&scheduler, &second, runtime.runtime().id());

        assert_eq!(scheduler.task_count().unwrap(), 1);
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
