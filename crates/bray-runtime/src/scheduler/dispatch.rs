use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use bray_platform::{MonotonicClock, MonotonicDeadline, MonotonicInstant};
use bray_runtime_model::{ProtectedFrameStateDescriptor, ProtectedFrameStateId};

use crate::lane::select_execution_lane;
use crate::{
    ExecutionLane, ScheduledTaskSnapshot, ScheduledTaskState, SchedulerSnapshot, TaskId,
    TaskWakeCause,
};

use super::contract::SchedulerError;
use super::engine::{DispatchState, ReadyTask, RegisteredTask, SchedulerData, SchedulerState};

pub(super) fn select_task_lane(
    scheduler: &SchedulerData,
    task: &RegisteredTask,
    state_id: ProtectedFrameStateId,
) -> Result<ExecutionLane, SchedulerError> {
    let Some(frame_state) = task.descriptor.state(state_id) else {
        return Err(SchedulerError::UnknownFrameState(state_id));
    };

    select_state_lane(scheduler, frame_state, task.origin)
}

pub(super) fn select_state_lane(
    scheduler: &SchedulerData,
    descriptor: &ProtectedFrameStateDescriptor,
    origin: bray_platform::RuntimeThreadId,
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
    let queued = queued_observations(state, now);

    let tasks = state
        .tasks
        .iter()
        .map(|(&task_id, task)| {
            scheduled_task_snapshot(scheduler, task_id, task, queued.get(&task_id).copied())
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

fn queued_observations(
    state: &SchedulerState,
    now: Option<MonotonicInstant>,
) -> BTreeMap<TaskId, QueuedObservation> {
    state
        .queues
        .values()
        .flatten()
        .map(|queued| {
            let age = now
                .zip(queued.queued_at)
                .map(|(now, queued_at)| now.elapsed_since(queued_at));

            (
                queued.task,
                QueuedObservation {
                    cause: queued.cause,
                    age,
                },
            )
        })
        .collect()
}

fn scheduled_task_snapshot(
    scheduler: &SchedulerData,
    task_id: TaskId,
    task: &RegisteredTask,
    queued: Option<QueuedObservation>,
) -> Result<ScheduledTaskSnapshot, SchedulerError> {
    let (dispatch, wake_cause) = match task.dispatch {
        DispatchState::Idle(_) => (ScheduledTaskState::Idle, None),
        DispatchState::Queued(_) => (
            ScheduledTaskState::Queued,
            queued.map(|queued| queued.cause),
        ),
        DispatchState::Running { pending, .. } => (ScheduledTaskState::Running, pending),
    };

    let origin = task.execution.origin().unwrap_or(task.origin);
    let lane = select_state_lane(scheduler, task.execution.descriptor(), origin)?;

    Ok(ScheduledTaskSnapshot::new(
        task_id,
        task.execution.clone(),
        dispatch,
        lane,
        task.wake_count,
        wake_cause,
        queued.and_then(|queued| queued.age),
        task.cancellation.observation(),
    ))
}
