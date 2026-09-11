use std::sync::Arc;
use std::time::Duration;

use bray_runtime_model::{
    ExecutionLaneRequirement, ProtectedFrameAffinity, ProtectedFrameDependencyId,
    ProtectedFrameDescriptor, ProtectedFrameStateId, ProtectedFrameStorageId,
};

use crate::{ExecutionLane, RunOutcomeKind, TaskId, TaskState};

/// Cancellation request and observation state at one inspection point.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CancellationObservation {
    requested: bool,
    observable: bool,
}

impl CancellationObservation {
    pub(crate) const fn new(requested: bool, observable: bool) -> Self {
        Self {
            requested,
            observable,
        }
    }

    /// Returns whether cancellation has been requested.
    pub const fn requested(self) -> bool {
        self.requested
    }

    /// Returns whether the request is currently observable by run code.
    pub const fn observable(self) -> bool {
        self.observable
    }
}

/// Runtime location that created one independently executing task.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TaskStartSite {
    parent: TaskId,
    frame: bray_runtime_model::ProtectedAsyncFrameId,
    state: ProtectedFrameStateId,
}

impl TaskStartSite {
    pub(crate) const fn new(
        parent: TaskId,
        frame: bray_runtime_model::ProtectedAsyncFrameId,
        state: ProtectedFrameStateId,
    ) -> Self {
        Self {
            parent,
            frame,
            state,
        }
    }

    /// Returns the active parent frame that created the child.
    pub const fn frame(self) -> bray_runtime_model::ProtectedAsyncFrameId {
        self.frame
    }

    /// Returns the parent task that created the child.
    pub const fn parent(self) -> TaskId {
        self.parent
    }

    /// Returns the parent frame state active at task creation.
    pub const fn state(self) -> ProtectedFrameStateId {
        self.state
    }
}

/// Immutable inspection observations for one task-control block.
#[derive(Clone, Debug)]
pub struct TaskSnapshot {
    task: TaskId,
    start_site: Option<TaskStartSite>,
    descriptor: ProtectedFrameDescriptor,
    state: TaskState,
    execution: crate::FrameExecutionState,
    cancellation: CancellationObservation,
    join_waiters: usize,
    unobserved_outcome: Option<RunOutcomeKind>,
}

impl TaskSnapshot {
    #[expect(
        clippy::too_many_arguments,
        reason = "the snapshot preserves the complete task observation contract"
    )]
    pub(crate) const fn new(
        task: TaskId,
        start_site: Option<TaskStartSite>,
        descriptor: ProtectedFrameDescriptor,
        state: TaskState,
        execution: crate::FrameExecutionState,
        cancellation: CancellationObservation,
        join_waiters: usize,
        unobserved_outcome: Option<RunOutcomeKind>,
    ) -> Self {
        Self {
            task,
            start_site,
            descriptor,
            state,
            execution,
            cancellation,
            join_waiters,
            unobserved_outcome,
        }
    }

    /// Returns the inspected task identity.
    pub const fn task(&self) -> TaskId {
        self.task
    }

    /// Returns the parent task and state that created this task, when applicable.
    pub const fn start_site(&self) -> Option<TaskStartSite> {
        self.start_site
    }

    /// Returns the compiler-generated descriptor for this task's frame.
    pub const fn descriptor(&self) -> &ProtectedFrameDescriptor {
        &self.descriptor
    }

    /// Returns the current task execution state.
    pub const fn state(&self) -> TaskState {
        self.state
    }

    /// Returns the active frame identity and its checked local execution state.
    pub const fn execution(&self) -> &crate::FrameExecutionState {
        &self.execution
    }

    /// Returns pending and currently observable cancellation state.
    pub const fn cancellation(&self) -> CancellationObservation {
        self.cancellation
    }

    /// Returns the number of observers waiting for terminal publication.
    pub const fn join_waiters(&self) -> usize {
        self.join_waiters
    }

    /// Returns an unobserved terminal outcome category, when one remains owned.
    pub const fn unobserved_outcome(&self) -> Option<RunOutcomeKind> {
        self.unobserved_outcome
    }

    /// Returns storage retained by the current protected-frame state.
    pub fn retained_storage(&self) -> &[ProtectedFrameStorageId] {
        self.execution.descriptor().initialized_storage()
    }

    /// Returns task dependencies that can block cleanup in the current state.
    pub fn cleanup_blockers(&self) -> &[ProtectedFrameDependencyId] {
        self.execution.descriptor().dependencies()
    }
}

/// Scheduler ownership of one registered task at observation time.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ScheduledTaskState {
    /// Execution has ended; result and registration ownership remain retained.
    Terminal,
    /// The task is registered but not queued.
    Idle,
    /// The task is waiting in its selected lane.
    Queued,
    /// One worker currently owns the task dispatch.
    Running,
}

/// Cause that made one protected-frame state ready.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TaskWakeCause {
    /// Runtime or generated code explicitly requested a wake.
    Explicit,
    /// A registered timer elapsed.
    Timer,
    /// Cancellation became observable.
    Cancellation,
}

/// Immutable scheduling observations for one registered task.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScheduledTaskSnapshot {
    task: TaskId,
    execution: crate::FrameExecutionState,
    dispatch: ScheduledTaskState,
    lane: ExecutionLane,
    wake_count: u64,
    wake_cause: Option<TaskWakeCause>,
    queue_age: Option<Duration>,
    cancellation: CancellationObservation,
}

impl ScheduledTaskSnapshot {
    #[expect(
        clippy::too_many_arguments,
        reason = "the snapshot preserves the complete scheduler observation contract"
    )]
    pub(crate) fn new(
        task: TaskId,
        execution: crate::FrameExecutionState,
        dispatch: ScheduledTaskState,
        lane: ExecutionLane,
        wake_count: u64,
        wake_cause: Option<TaskWakeCause>,
        queue_age: Option<Duration>,
        cancellation: CancellationObservation,
    ) -> Self {
        Self {
            task,
            execution,
            dispatch,
            lane,
            wake_count,
            wake_cause,
            queue_age,
            cancellation,
        }
    }

    /// Returns the registered task identity.
    pub const fn task(&self) -> TaskId {
        self.task
    }

    /// Returns the protected-frame state owned by the scheduler.
    pub const fn state(&self) -> ProtectedFrameStateId {
        self.execution.state()
    }

    /// Returns the active frame identity and checked execution metadata.
    pub const fn execution(&self) -> &crate::FrameExecutionState {
        &self.execution
    }

    /// Returns how the scheduler currently owns the task.
    pub const fn dispatch(&self) -> ScheduledTaskState {
        self.dispatch
    }

    /// Returns the compatible lane selected for this state.
    pub const fn lane(&self) -> ExecutionLane {
        self.lane
    }

    /// Returns the frame affinity that caused lane placement.
    pub const fn affinity(&self) -> ProtectedFrameAffinity {
        self.execution.descriptor().affinity()
    }

    /// Returns the checked workload and placement requirements.
    pub fn lane_requirements(&self) -> &[ExecutionLaneRequirement] {
        self.execution.descriptor().lane_requirements()
    }

    /// Returns the number of accepted wake requests for this registration.
    pub const fn wake_count(&self) -> u64 {
        self.wake_count
    }

    /// Returns the cause retained by the current queued or pending wake.
    pub const fn wake_cause(&self) -> Option<TaskWakeCause> {
        self.wake_cause
    }

    /// Returns time spent in the current ready queue when timing is enabled.
    pub const fn queue_age(&self) -> Option<Duration> {
        self.queue_age
    }

    /// Returns pending and currently observable cancellation state.
    pub const fn cancellation(&self) -> CancellationObservation {
        self.cancellation
    }
}

/// Immutable observation of one scheduler's registered work.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SchedulerSnapshot {
    tasks: Arc<[ScheduledTaskSnapshot]>,
    timer_count: usize,
}

impl SchedulerSnapshot {
    pub(crate) fn new(
        tasks: impl IntoIterator<Item = ScheduledTaskSnapshot>,
        timer_count: usize,
    ) -> Self {
        let mut tasks = tasks.into_iter().collect::<Vec<_>>();
        tasks.sort_unstable_by_key(ScheduledTaskSnapshot::task);

        Self {
            tasks: tasks.into(),
            timer_count,
        }
    }

    /// Returns tasks in deterministic process-local identity order.
    pub fn tasks(&self) -> &[ScheduledTaskSnapshot] {
        &self.tasks
    }

    /// Returns the number of pending timer wakes.
    pub const fn timer_count(&self) -> usize {
        self.timer_count
    }
}
