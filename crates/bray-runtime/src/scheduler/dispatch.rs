use std::sync::Arc;

use bray_platform::{MonotonicClock, MonotonicDeadline, MonotonicInstant};
use bray_runtime_model::ProtectedFrameStateDescriptor;

use crate::lane::select_execution_lane;
use crate::{
    ExecutionLane, ScheduledTaskSnapshot, ScheduledTaskState, SchedulerSnapshot, TaskId,
};

use super::contract::SchedulerError;
use super::queue;
use super::engine::{DispatchState, ReadyTask, RegisteredTask, SchedulerData, SchedulerState};

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
    if state.running_count == scheduler.limits.tasks().get() {
        return None;
    }

    let (identity, queued) = queue::pop(state, lane)?;

    let task = state.tasks.get_mut(&identity).expect("a ready link retains its registered task");

    assert_eq!(task.dispatch, DispatchState::Queued(queued.state), "ready links match the registered continuation");

    task.dispatch = DispatchState::Running { state: queued.state, pending: None };
    task.execution_slot = true;
    state.running_count += 1;

    Some(ReadyTask {
        task: identity,
        execution: task.execution.clone(),
        lane,
        wake_cause: queued.cause,
        queue_latency: queued.queued_at.map(|queued_at| MonotonicClock.now().elapsed_since(queued_at)),
        scheduler: Arc::downgrade(scheduler),
        released: false,
        native: task.native.as_ref().map(Arc::clone),
    })
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
            scheduled_task_snapshot(scheduler, task_id, task, now)
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

fn scheduled_task_snapshot(
    scheduler: &SchedulerData,
    task_id: TaskId,
    task: &RegisteredTask,
    now: Option<MonotonicInstant>,
) -> Result<ScheduledTaskSnapshot, SchedulerError> {
    let queued = task.queued.as_ref();

    let latency = queued.and_then(|queued| queued.queued_at).zip(now)
        .map(|(queued_at, now)| now.elapsed_since(queued_at));

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
        latency,
        task.cancellation.observation(),
    ))
}
