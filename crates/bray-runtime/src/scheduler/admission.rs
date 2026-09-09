use std::sync::Arc;

use bray_platform::RuntimeThreadId;
use bray_runtime_model::{ProtectedFrameDescriptor, ProtectedFrameStateId};

use crate::task::TaskAdmissionKind;
use crate::{CancellationContext, ExecutionLane, TaskId};

use super::contract::SchedulerError;
use super::dispatch::select_task_lane;
use super::engine::{
    DispatchState, RegisteredTask, Scheduler, SchedulerState, TaskRegistration, TaskWakeHandle,
};

/// Task-owned registration storage that can be prepared before choosing a scheduler or origin.
pub(crate) struct TaskRegistrationStorage {
    descriptor: ProtectedFrameDescriptor,
    cancellation: CancellationContext,
    cancellation_wake: Option<crate::cancellation::CancellationWakeRegistration>,
    ready_lanes: Vec<ExecutionLane>,
}

impl TaskRegistrationStorage {
    pub(crate) fn lane(
        &self,
        scheduler: &Scheduler,
        origin: RuntimeThreadId,
        state: ProtectedFrameStateId,
    ) -> Result<ExecutionLane, SchedulerError> {
        select_task_lane(&scheduler.data, &self.descriptor, origin, state)
    }

    pub(crate) fn prepare(
        descriptor: ProtectedFrameDescriptor,
        cancellation: &CancellationContext,
    ) -> Result<Self, SchedulerError> {
        let mut ready_lanes = Vec::new();
        crate::allocation::reserve_vec_entries(&mut ready_lanes, descriptor.states().len())?;
        let cancellation_wake = cancellation.reserve_wake()?;

        Ok(Self {
            descriptor,
            cancellation: cancellation.clone(),
            cancellation_wake: Some(cancellation_wake),
            ready_lanes,
        })
    }
}

impl Scheduler {
    /// Registers one independent task without making it ready.
    pub fn register_task(
        &self,
        task: TaskId,
        descriptor: ProtectedFrameDescriptor,
        origin: RuntimeThreadId,
        initial_state: ProtectedFrameStateId,
        cancellation: &CancellationContext,
    ) -> Result<TaskRegistration, SchedulerError> {
        self.register_admitted_task(
            task,
            descriptor,
            origin,
            initial_state,
            cancellation,
            TaskAdmissionKind::Independent,
        )
    }

    pub(crate) fn register_admitted_task(
        &self,
        task: TaskId,
        descriptor: ProtectedFrameDescriptor,
        origin: RuntimeThreadId,
        initial_state: ProtectedFrameStateId,
        cancellation: &CancellationContext,
        admission: TaskAdmissionKind,
    ) -> Result<TaskRegistration, SchedulerError> {
        select_task_lane(&self.data, &descriptor, origin, initial_state)?;

        self.lock_state()?
            .check_task_admission(task, admission, self.data.limits.tasks().get())?;

        let mut storage = TaskRegistrationStorage::prepare(descriptor, cancellation)?;

        self.register_prepared_task(task, origin, initial_state, admission, &mut storage)
    }

    pub(crate) fn register_prepared_task(
        &self,
        task: TaskId,
        origin: RuntimeThreadId,
        initial_state: ProtectedFrameStateId,
        admission: TaskAdmissionKind,
        storage: &mut TaskRegistrationStorage,
    ) -> Result<TaskRegistration, SchedulerError> {
        let descriptor = &storage.descriptor;
        select_task_lane(&self.data, descriptor, origin, initial_state)?;

        assert!(
            storage.cancellation_wake.is_some(),
            "registration storage must install once"
        );

        let ready_lanes = &mut storage.ready_lanes;
        ready_lanes.clear();

        for frame_state in descriptor.states() {
            let lane = select_task_lane(&self.data, descriptor, origin, frame_state.state())?;
            ready_lanes.push(lane);
        }

        ready_lanes.sort_unstable();
        ready_lanes.dedup();

        {
            let mut state = self.lock_state()?;

            state.check_task_admission(task, admission, self.data.limits.tasks().get())?;

            crate::allocation::reserve_map_entries(&mut state.tasks, 1)?;
            let ready_slot = state.ready.reserve()?;

            if let Err(error) = state.reserve_ready_queues(ready_lanes) {
                let SchedulerState { ready, queues, .. } = &mut *state;
                ready.release(ready_slot, queues);

                return Err(error);
            }

            state.tasks.insert(
                task,
                RegisteredTask {
                    admission,
                    descriptor: descriptor.clone(),
                    origin,
                    cancellation: storage.cancellation.clone(),
                    dispatch: DispatchState::Idle(initial_state),
                    wake_count: 0,
                    ready_lanes: std::mem::take(ready_lanes),
                    ready_slot,
                },
            );

            if admission == TaskAdmissionKind::Independent {
                state.independent_tasks += 1;
            }
        }

        let Some(mut cancellation_wake) = storage.cancellation_wake.take() else {
            unreachable!("prepared registration retains its cancellation wake");
        };

        cancellation_wake.bind(TaskWakeHandle {
            task,
            scheduler: Arc::downgrade(&self.data),
        });

        Ok(TaskRegistration {
            task,
            scheduler: Arc::downgrade(&self.data),
            _cancellation_wake: cancellation_wake,
        })
    }
}

impl SchedulerState {
    fn check_task_admission(
        &self,
        task: TaskId,
        admission: TaskAdmissionKind,
        limit: usize,
    ) -> Result<(), SchedulerError> {
        if self.tasks.contains_key(&task) {
            return Err(SchedulerError::TaskAlreadyRegistered(task));
        }

        if admission == TaskAdmissionKind::Independent && self.independent_tasks >= limit {
            return Err(SchedulerError::TaskCapacityReached);
        }

        Ok(())
    }

    pub(super) fn remove_task(&mut self, task: TaskId) {
        if let Some(registered) = self.tasks.remove(&task) {
            self.ready.release(registered.ready_slot, &mut self.queues);
            self.release_ready_queues(&registered.ready_lanes);

            if registered.admission == TaskAdmissionKind::Independent {
                self.independent_tasks -= 1;
            }
        }
    }

    fn reserve_ready_queues(&mut self, lanes: &[ExecutionLane]) -> Result<(), SchedulerError> {
        let additional = lanes
            .iter()
            .filter(|lane| !self.queues.contains_key(lane))
            .count();

        crate::allocation::reserve_map_entries(&mut self.queues, additional)?;

        for (index, lane) in lanes.iter().copied().enumerate() {
            let queue = self.queues.entry(lane).or_default();

            if let Err(error) = queue.reserve() {
                if queue.is_unreserved() {
                    self.queues.remove(&lane);
                }

                self.release_ready_queues(&lanes[..index]);

                return Err(error);
            }
        }

        Ok(())
    }

    fn release_ready_queues(&mut self, lanes: &[ExecutionLane]) {
        for lane in lanes {
            if let Some(queue) = self.queues.get_mut(lane)
                && queue.release()
            {
                self.queues.remove(lane);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;

    use bray_platform::RuntimeThreadScope;
    use bray_runtime_model::{ProtectedFrameStateId, RuntimeCapability};

    use super::Scheduler;
    use crate::test_support::{TestFrame, register_task, with_allocation_failure};
    use crate::{FrameSuspension, SchedulerError, SchedulerLimits, TaskControlBlock};

    #[test]
    fn prepared_registration_retries_each_scheduler_allocation_without_losing_storage() {
        let runtime = RuntimeThreadScope::enter().unwrap();
        let thread = runtime.runtime().id();

        for allowed in 0..8 {
            let scheduler = Scheduler::new(
                [
                    RuntimeCapability::CooperativeExecution,
                    RuntimeCapability::MigratableLanes,
                    RuntimeCapability::MainThreadLane,
                ],
                thread,
                SchedulerLimits::new(NonZeroUsize::new(1).unwrap(), NonZeroUsize::new(1).unwrap()),
            );

            let task = TaskControlBlock::start(TestFrame::main_thread_then_movable(7)).unwrap();

            let mut storage = super::TaskRegistrationStorage::prepare(
                task.descriptor().clone(),
                task.cancellation_context(),
            )
            .unwrap();

            let capacity = storage.ready_lanes.capacity();

            let register = |storage: &mut super::TaskRegistrationStorage| {
                scheduler.register_prepared_task(
                    task.id(),
                    thread,
                    ProtectedFrameStateId::new(0),
                    crate::task::TaskAdmissionKind::Independent,
                    storage,
                )
            };

            let result = crate::test_support::with_allocation_failure_after(allowed, || {
                register(&mut storage)
            });

            match result {
                Ok(registration) => {
                    with_allocation_failure(|| drop(registration));
                    assert_eq!(scheduler.task_count().unwrap(), 0);
                    return;
                }
                Err(SchedulerError::AdmissionAllocation(_)) => {
                    assert_eq!(scheduler.task_count().unwrap(), 0);
                    assert!(scheduler.lock_state().unwrap().queues.is_empty());
                    assert_eq!(storage.ready_lanes.capacity(), capacity);
                    assert!(storage.cancellation_wake.is_some());
                    let registration = register(&mut storage).unwrap();
                    assert_eq!(scheduler.task_count().unwrap(), 1);
                    assert!(storage.cancellation_wake.is_none());
                    with_allocation_failure(|| drop(registration));
                }
                Err(error) => panic!("unexpected prepared registration rejection: {error:?}"),
            }
        }

        panic!("prepared registration did not reach successful admission");
    }

    #[test]
    fn failed_multi_lane_admission_rolls_back_and_existing_tasks_keep_their_capacity() {
        let runtime = RuntimeThreadScope::enter().unwrap();
        let thread = runtime.runtime().id();

        let scheduler = Scheduler::new(
            [
                RuntimeCapability::CooperativeExecution,
                RuntimeCapability::MigratableLanes,
                RuntimeCapability::MainThreadLane,
            ],
            thread,
            SchedulerLimits::new(NonZeroUsize::new(2).unwrap(), NonZeroUsize::new(1).unwrap()),
        );

        let first_task = TaskControlBlock::start(TestFrame::completing(1)).unwrap();
        let second_task = TaskControlBlock::start(TestFrame::main_thread_then_movable(2)).unwrap();
        let initial = ProtectedFrameStateId::new(0);
        let resumed = ProtectedFrameStateId::new(1);
        let first = register_task(&scheduler, &first_task, thread);

        let register_second = || {
            scheduler.register_task(
                second_task.id(),
                second_task.descriptor().clone(),
                thread,
                initial,
                second_task.cancellation_context(),
            )
        };

        assert!(matches!(
            with_allocation_failure(register_second),
            Err(SchedulerError::AdmissionAllocation(_))
        ));

        assert_eq!(scheduler.task_count().unwrap(), 1);
        assert_eq!(scheduler.lock_state().unwrap().queues.len(), 1);

        with_allocation_failure(|| {
            first.wake_handle().wake(initial).unwrap();

            let ready = scheduler
                .take_ready(first.lane(initial).unwrap())
                .unwrap()
                .unwrap();

            first_task.resume().unwrap();
            ready.complete().unwrap();
        });

        let second = register_second().unwrap();

        assert_eq!(scheduler.task_count().unwrap(), 2);
        assert_eq!(scheduler.lock_state().unwrap().queues.len(), 2);

        let snapshot = scheduler.snapshot().unwrap();

        assert!(
            snapshot
                .tasks()
                .windows(2)
                .all(|tasks| tasks[0].task() < tasks[1].task())
        );

        // Queue admission covers both the initial main-thread state and the later movable state.
        // Exercise the scheduler's running-wake protocol independently of frame execution.
        with_allocation_failure(|| {
            second.wake_handle().wake(initial).unwrap();

            let ready = scheduler
                .take_ready(second.lane(initial).unwrap())
                .unwrap()
                .unwrap();

            second.wake_handle().wake(resumed).unwrap();
            ready.suspend(FrameSuspension::new(resumed)).unwrap();

            let ready = scheduler
                .take_ready(second.lane(resumed).unwrap())
                .unwrap()
                .unwrap();

            assert_eq!(ready.state(), resumed);
            ready.complete().unwrap();
        });

        drop(second);

        assert_eq!(scheduler.lock_state().unwrap().queues.len(), 1);

        drop(first);

        assert!(scheduler.lock_state().unwrap().queues.is_empty());
    }
}
