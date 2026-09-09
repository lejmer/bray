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

        {
            let mut state = self.lock_state()?;

            if state.tasks.contains_key(&task) {
                return Err(SchedulerError::TaskAlreadyRegistered(task));
            }

            if admission == TaskAdmissionKind::Independent
                && state.independent_tasks >= self.data.limits.tasks().get()
            {
                return Err(SchedulerError::TaskCapacityReached);
            }

            let mut ready_lanes = Vec::new();
            crate::allocation::reserve_vec_entries(&mut ready_lanes, descriptor.states().len())?;

            for frame_state in descriptor.states() {
                let lane = select_task_lane(&self.data, &descriptor, origin, frame_state.state())?;
                ready_lanes.push(lane);
            }

            ready_lanes.sort_unstable();
            ready_lanes.dedup();

            crate::allocation::reserve_map_entries(&mut state.tasks, 1)?;
            let ready_slot = state.ready.reserve()?;

            if let Err(error) = state.reserve_ready_queues(&ready_lanes) {
                let SchedulerState { ready, queues, .. } = &mut *state;
                ready.release(ready_slot, queues);

                return Err(error);
            }

            state.tasks.insert(
                task,
                RegisteredTask {
                    admission,
                    descriptor,
                    origin,
                    cancellation: cancellation.clone(),
                    dispatch: DispatchState::Idle(initial_state),
                    wake_count: 0,
                    ready_lanes,
                    ready_slot,
                },
            );

            if admission == TaskAdmissionKind::Independent {
                state.independent_tasks += 1;
            }
        }

        let cancellation_wake = match cancellation.register_wake(TaskWakeHandle {
            task,
            scheduler: Arc::downgrade(&self.data),
        }) {
            Ok(registration) => registration,
            Err(error) => {
                self.lock_state()?.remove_task(task);

                return Err(error.into());
            }
        };

        Ok(TaskRegistration {
            task,
            scheduler: Arc::downgrade(&self.data),
            _cancellation_wake: cancellation_wake,
        })
    }
}

impl SchedulerState {
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
