use bray_platform::MonotonicInstant;
use bray_runtime_model::ProtectedFrameStateId;

use crate::{ExecutionLane, TaskId, TaskWakeCause};

use super::engine::SchedulerState;

#[derive(Debug, Default)]
pub(super) struct ReadyQueue {
    head: Option<TaskId>,
    tail: Option<TaskId>,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct QueuedTask {
    pub(super) state: ProtectedFrameStateId,
    pub(super) cause: TaskWakeCause,
    pub(super) queued_at: Option<MonotonicInstant>,
    lane: ExecutionLane,
    previous: Option<TaskId>,
    next: Option<TaskId>,
}

pub(super) fn push(
    state: &mut SchedulerState,
    task: TaskId,
    lane: ExecutionLane,
    frame: ProtectedFrameStateId,
    cause: TaskWakeCause,
    queued_at: Option<MonotonicInstant>,
) {
    assert!(state.tasks.get(&task).expect("queued work is registered").queued.is_none(),
        "a task has at most one ready link");

    let previous = state.queues.get(&lane).and_then(|queue| queue.tail);

    if let Some(previous) = previous {
        state.tasks.get_mut(&previous).and_then(|task| task.queued.as_mut())
            .expect("the tail owns its registered ready link").next = Some(task);
    }

    state.tasks.get_mut(&task).expect("queued work remains registered").queued = Some(QueuedTask {
        state: frame, cause, queued_at, lane, previous, next: None,
    });

    // Registration admits one possible bucket per task. Empty buckets are
    // removed, so a later lane change can insert here without allocating.
    assert!(state.queues.len() < state.queues.capacity(), "ready bucket backing precedes publication");

    let queue = state.queues.entry(lane).or_default();

    if queue.head.is_none() {
        queue.head = Some(task);
    }

    queue.tail = Some(task);
}

pub(super) fn remove(state: &mut SchedulerState, task: TaskId) -> Option<QueuedTask> {
    let queued = state.tasks.get_mut(&task)?.queued.take()?;

    if let Some(previous) = queued.previous {
        state.tasks.get_mut(&previous).and_then(|task| task.queued.as_mut())
            .expect("the previous ready link remains registered").next = queued.next;
    }

    if let Some(next) = queued.next {
        state.tasks.get_mut(&next).and_then(|task| task.queued.as_mut())
            .expect("the next ready link remains registered").previous = queued.previous;
    }

    let queue = state.queues.get_mut(&queued.lane).expect("a ready link retains its lane bucket");

    if queue.head == Some(task) {
        queue.head = queued.next;
    }

    if queue.tail == Some(task) {
        queue.tail = queued.previous;
    }

    if queue.head.is_none() {
        assert!(queue.tail.is_none(), "an empty queue has no tail");
        state.queues.remove(&queued.lane);
    }

    Some(queued)
}

pub(super) fn pop(state: &mut SchedulerState, lane: ExecutionLane) -> Option<(TaskId, QueuedTask)> {
    let task = state.queues.get(&lane)?.head?;
    let queued = remove(state, task).expect("the queue head owns its registered ready link");

    Some((task, queued))
}
