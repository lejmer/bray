use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use bray_platform::{
    MonotonicClock, MonotonicDeadline, MonotonicInstant,
};
use bray_runtime_interface::ProtectedFrameStateId;

use crate::lane::select_execution_lane;
use crate::{
    ExecutionLane, ScheduledTaskSnapshot, ScheduledTaskState, SchedulerSnapshot,
    TaskId, TaskWakeCause,
};

use super::contract::SchedulerError;
use super::engine::{
    DispatchState, ReadyTask, RegisteredTask, SchedulerData, SchedulerState,
};

pub(super) fn select_task_lane(
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
            state: queued.state,
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
            let age = now.zip(queued.queued_at).map(|(now, queued_at)| {
                now.elapsed_since(queued_at)
            });

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
    let (state, dispatch, wake_cause) = match task.dispatch {
        DispatchState::Idle(state) => (state, ScheduledTaskState::Idle, None),
        DispatchState::Queued(state) => (
            state,
            ScheduledTaskState::Queued,
            queued.map(|queued| queued.cause),
        ),
        DispatchState::Running { state, pending } => (
            state,
            ScheduledTaskState::Running,
            pending.map(|pending| pending.cause),
        ),
    };

    let lane = select_task_lane(scheduler, task, state)?;

    let Some(frame_state) = task.descriptor.state(state) else {
        return Err(SchedulerError::UnknownFrameState(state));
    };

    Ok(ScheduledTaskSnapshot::new(
        task_id,
        state,
        dispatch,
        lane,
        frame_state.affinity(),
        Arc::from(frame_state.lane_requirements()),
        task.wake_count,
        wake_cause,
        queued.and_then(|queued| queued.age),
        task.cancellation.is_requested(),
    ))
}
