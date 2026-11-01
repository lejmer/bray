use std::num::NonZeroU64;
use std::sync::Arc;
use std::time::Duration;

use bray_platform::RuntimeThreadId;
use bray_runtime_abi::NativeSchedulerObservation;
use bray_runtime_model::ProtectedFrameStateId;

use super::contract::SchedulerError;
use super::engine::SchedulerData;
use super::native;
use crate::{
    ExecutionLane, FrameExecutionState, ScheduledTaskSnapshot, ScheduledTaskState,
    SchedulerSnapshot, TaskId, TaskWakeCause,
};

pub(super) fn pop_ready(
    scheduler: &Arc<SchedulerData>,
    thread: RuntimeThreadId,
    lanes: u32,
) -> Result<Option<ReadyTask>, SchedulerError> {
    let state = scheduler
        .state
        .lock()
        .map_err(|_| SchedulerError::SynchronizationPoisoned)?;

    let mut selected = NativeSchedulerObservation::default();

    let status = native::bray_runtime_scheduler_acquisition(
        &scheduler.source.reference,
        thread.raw(),
        lanes,
        &mut selected,
    );

    if status == 0 {
        return Ok(None);
    }

    assert_eq!(
        status, 1,
        "Bray returns either an exclusive dispatch or no ready work"
    );

    let task = TaskId::from_native(
        NonZeroU64::new(selected.task).expect("a ready entry retains its nonzero binding identity"),
    );

    let binding = state
        .tasks
        .get(&task)
        .expect("a selected Bray entry retains its native binding");

    assert_eq!(
        binding.registration, selected.registration,
        "dispatch retains the admitted generation"
    );

    Ok(Some(ReadyTask {
        task,
        registration: selected.registration,
        execution: binding.execution.clone(),
        lane: native::from_native(selected.lane),
        wake_cause: native::wake_cause(selected.cause)
            .expect("a ready entry retains its wake cause"),
        queue_latency: queue_age(scheduler, selected.queued_at),
        scheduler: Arc::clone(scheduler),
        released: false,
        native: binding.native.as_ref().map(Arc::clone),
    }))
}

pub(super) fn scheduler_snapshot(
    scheduler: &SchedulerData,
) -> Result<SchedulerSnapshot, SchedulerError> {
    let mut tasks = Vec::new();

    loop {
        let state = scheduler
            .state
            .lock()
            .map_err(|_| SchedulerError::SynchronizationPoisoned)?;

        if tasks.capacity() < state.tasks.len() {
            let required = state.tasks.len();

            drop(state);

            tasks
                .try_reserve(required)
                .map_err(SchedulerError::ReadyStorageUnavailable)?;

            continue;
        }

        for (&task, binding) in &state.tasks {
            let Some(cancellation) = binding.cancellation.as_ref() else {
                continue;
            };

            let mut observation = NativeSchedulerObservation::default();

            let status = native::bray_runtime_scheduler_snapshot(
                &scheduler.source.reference,
                binding.registration,
                &mut observation,
            );

            assert_eq!(status, 0, "a native binding retains its Bray registration");

            let dispatch = match observation.phase {
                0 => ScheduledTaskState::Idle,
                1 => ScheduledTaskState::Queued,
                2 => ScheduledTaskState::Running,
                phase => panic!("a live binding has invalid phase {phase}"),
            };

            let cancellation = cancellation.observation();

            tasks.push(ScheduledTaskSnapshot::new(
                task,
                binding.execution.clone(),
                dispatch,
                native::from_native(observation.lane),
                observation.wakes,
                native::wake_cause(observation.cause),
                (observation.phase == 1)
                    .then(|| queue_age(scheduler, observation.queued_at))
                    .flatten(),
                cancellation,
            ));
        }

        let timers = state.timers.len();

        drop(state);
        tasks.sort_unstable_by_key(ScheduledTaskSnapshot::task);

        return Ok(SchedulerSnapshot::new(tasks, timers));
    }
}

fn queue_age(scheduler: &SchedulerData, queued_at: u64) -> Option<Duration> {
    scheduler
        .observe_queue_time
        .then(|| Duration::from_nanos(scheduler.ticks().saturating_sub(queued_at)))
}

/// One exclusive dispatch and the domain/resources retained through its return.
#[derive(Debug)]
pub struct ReadyTask {
    task: TaskId,
    registration: u64,
    execution: FrameExecutionState,
    lane: ExecutionLane,
    wake_cause: TaskWakeCause,
    queue_latency: Option<Duration>,
    scheduler: Arc<SchedulerData>,
    released: bool,
    native: Option<Arc<crate::native::NativeTaskServices>>,
}

impl ReadyTask {
    pub(crate) fn take_native_task(&mut self) -> Option<Arc<crate::native::NativeTaskServices>> {
        self.native.take()
    }
    /// Returns the independent run identity.
    pub const fn task(&self) -> TaskId {
        self.task
    }
    /// Returns the checked continuation state.
    pub fn state(&self) -> ProtectedFrameStateId {
        self.execution.state()
    }
    /// Returns the complete retained frame metadata.
    pub const fn execution(&self) -> &FrameExecutionState {
        &self.execution
    }
    /// Returns the lane acquired by this transition.
    pub const fn lane(&self) -> ExecutionLane {
        self.lane
    }
    /// Returns the first wake coalesced into this dispatch.
    pub const fn wake_cause(&self) -> TaskWakeCause {
        self.wake_cause
    }
    /// Returns optional time spent queued.
    pub const fn queue_latency(&self) -> Option<Duration> {
        self.queue_latency
    }

    /// Commits suspension, releases quota and remembers any racing wake.
    pub fn suspend(
        mut self,
        execution: FrameExecutionState,
    ) -> Result<ExecutionLane, SchedulerError> {
        let lane = self.release(execution)?;

        self.released = true;

        Ok(lane)
    }

    fn release(&self, execution: FrameExecutionState) -> Result<ExecutionLane, SchedulerError> {
        let mut state = self
            .scheduler
            .state
            .lock()
            .map_err(|_| SchedulerError::SynchronizationPoisoned)?;

        let binding = state
            .tasks
            .get_mut(&self.task)
            .filter(|binding| binding.registration == self.registration)
            .ok_or(SchedulerError::UnknownTask(self.task))?;

        assert_eq!(
            binding.execution.state(),
            self.state(),
            "dispatch release retains the acquired continuation"
        );

        let lane = self.scheduler.source.select(
            execution.descriptor(),
            execution.origin().unwrap_or(binding.origin),
        )?;

        let cancellation = u32::from(
            binding
                .cancellation
                .as_ref()
                .is_some_and(crate::CancellationContext::recheck_dispatch),
        ) * native::cause(TaskWakeCause::Cancellation);

        let previous = std::mem::replace(&mut binding.execution, execution);

        let status = native::bray_runtime_scheduler_release(
            &self.scheduler.source.reference,
            self.registration,
            &native::to_native(lane),
            cancellation,
            self.scheduler.ticks(),
        );

        assert_eq!(
            status, 0,
            "a retained binding releases its exclusive Bray dispatch"
        );

        drop(state);
        drop(previous);

        Ok(lane)
    }
}

impl Drop for ReadyTask {
    fn drop(&mut self) {
        if !self.released {
            match self.release(self.execution.clone()) {
                Ok(_)
                | Err(SchedulerError::UnknownTask(_))
                | Err(SchedulerError::SynchronizationPoisoned) => {}
                Err(error) => {
                    panic!("an unchanged checked dispatch releases successfully: {error:?}")
                }
            }
        }
    }
}
