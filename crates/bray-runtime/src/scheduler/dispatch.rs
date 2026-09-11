use std::sync::Arc;
use std::time::Duration;

use bray_platform::{MonotonicClock, MonotonicDeadline, MonotonicInstant, RuntimeThreadId};
use bray_runtime_model::{ProtectedFrameDescriptor, ProtectedFrameStateDescriptor, ProtectedFrameStateId};

use crate::lane::select_execution_lane;
use crate::{
    ExecutionLane, ScheduledTaskSnapshot, ScheduledTaskState, SchedulerSnapshot, TaskId,
    TaskWakeCause,
};

use super::contract::SchedulerError;
use super::engine::{DispatchState, ReadyTask, RegisteredTask, SchedulerData, SchedulerState};

pub(super) fn select_task_lane(
    scheduler: &SchedulerData,
    descriptor: &ProtectedFrameDescriptor,
    origin: RuntimeThreadId,
    state_id: ProtectedFrameStateId,
) -> Result<ExecutionLane, SchedulerError> {
    let Some(frame_state) = descriptor.state(state_id) else {
        return Err(SchedulerError::UnknownFrameState(state_id));
    };

    select_state_lane(scheduler, frame_state, origin)
}

pub(super) fn select_state_lane(
    scheduler: &SchedulerData,
    descriptor: &ProtectedFrameStateDescriptor,
    origin: RuntimeThreadId,
) -> Result<ExecutionLane, SchedulerError> {
    select_execution_lane(
        descriptor.lane_requirements(),
        descriptor.affinity(),
        &scheduler.capabilities,
        origin,
        scheduler.main_thread,
    )
    .map_err(Into::into)
}

pub(super) fn pop_ready(
    scheduler: &Arc<SchedulerData>,
    state: &mut SchedulerState,
    lane: ExecutionLane,
) -> Option<ReadyTask> {
    loop {
        let queued = state.ready.pop(state.queues.get_mut(&lane)?)?;

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
            // Dispatch owns the shared descriptor while registration retains current execution.
            execution: task.execution.clone(),
            lane,
            wake_cause: queued.cause,
            queue_latency: queued
                .queued_at
                .map(|queued_at| MonotonicClock.now().elapsed_since(queued_at)),
            scheduler: Arc::downgrade(scheduler),
            released: false,
        });
    }
}

pub(super) fn scheduler_snapshot(
    scheduler: &SchedulerData,
    state: &SchedulerState,
) -> Result<SchedulerSnapshot, SchedulerError> {
    let now = scheduler.observe_queue_time.then(|| MonotonicClock.now());

    let tasks = state
        .tasks
        .iter()
        .map(|(&task_id, task)| {
            let queued = state
                .ready
                .queued(task.ready_slot)
                .map(|queued| QueuedObservation {
                    cause: queued.cause,
                    age: now
                        .zip(queued.queued_at)
                        .map(|(now, queued_at)| now.elapsed_since(queued_at)),
                });

            scheduled_task_snapshot(scheduler, task_id, task, queued)
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(SchedulerSnapshot::new(tasks, state.timer_count))
}

pub(super) fn queue_instant(scheduler: &SchedulerData) -> Option<MonotonicInstant> {
    scheduler.observe_queue_time.then(|| MonotonicClock.now())
}

pub(super) fn earliest_deadline(
    first: Option<MonotonicDeadline>,
    second: Option<MonotonicDeadline>,
) -> Option<MonotonicDeadline> {
    match (first, second) {
        (Some(first), Some(second)) => Some(first.min(second)),
        (Some(deadline), None) | (None, Some(deadline)) => Some(deadline),
        (None, None) => None,
    }
}

#[derive(Clone, Copy)]
struct QueuedObservation {
    cause: TaskWakeCause,
    age: Option<Duration>,
}

fn scheduled_task_snapshot(
    scheduler: &SchedulerData,
    task_id: TaskId,
    task: &RegisteredTask,
    queued: Option<QueuedObservation>,
) -> Result<ScheduledTaskSnapshot, SchedulerError> {
    let (dispatch, wake_cause) = match task.dispatch {
        DispatchState::Terminal(_) => (ScheduledTaskState::Terminal, None),
        DispatchState::Idle(_) => (ScheduledTaskState::Idle, None),
        DispatchState::Queued(_) => (
            ScheduledTaskState::Queued,
            queued.map(|queued| queued.cause),
        ),
        DispatchState::Running { pending, .. } => (ScheduledTaskState::Running, pending),
    };

    let lane = select_state_lane(scheduler, task.execution.descriptor(), task.origin)?;

    Ok(ScheduledTaskSnapshot::new(
        task_id,
        // Observation owns a shared snapshot independent of later dispatch transitions.
        task.execution.clone(),
        dispatch,
        lane,
        task.wake_count,
        wake_cause,
        queued.and_then(|queued| queued.age),
        task.cancellation.observation(),
    ))
}
