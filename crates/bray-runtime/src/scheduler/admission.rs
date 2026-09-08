use std::sync::Arc;

use bray_platform::RuntimeThreadId;
use bray_runtime_model::{ProtectedFrameDescriptor, ProtectedFrameStateId};

use crate::lane::select_execution_lane;
use crate::task::TaskAdmissionKind;
use crate::{CancellationContext, TaskId};

use super::contract::SchedulerError;
use super::engine::{
    DispatchState, RegisteredTask, Scheduler, SchedulerState, TaskRegistration, wake_cancelled_task,
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
        let Some(frame_state) = descriptor.state(initial_state) else {
            return Err(SchedulerError::UnknownFrameState(initial_state));
        };

        select_execution_lane(
            frame_state.lane_requirements(),
            frame_state.affinity(),
            &self.data.capabilities,
            origin,
            self.data.main_thread,
        )?;

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

            state.tasks.insert(
                task,
                RegisteredTask {
                    admission,
                    descriptor,
                    origin,
                    cancellation: cancellation.clone(),
                    dispatch: DispatchState::Idle(initial_state),
                    wake_count: 0,
                },
            );

            if admission == TaskAdmissionKind::Independent {
                state.independent_tasks += 1;
            }
        }

        let scheduler = Arc::downgrade(&self.data);

        let cancellation_wake = match cancellation
            .register_wake(Arc::new(move || wake_cancelled_task(&scheduler, task)))
        {
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
        if let Some(task) = self.tasks.remove(&task)
            && task.admission == TaskAdmissionKind::Independent
        {
            self.independent_tasks -= 1;
        }
    }
}
