use std::sync::Arc;

use crate::TaskId;

use super::engine::{DispatchState, SchedulerData, TaskRegistration};

/// Temporarily lends a blocked native callback's execution capacity to other work.
/// The enclosing dispatch keeps exclusive ownership of its task throughout.
pub(crate) struct ExecutionPark {
    scheduler: Arc<SchedulerData>,
    task: TaskId,
}

impl TaskRegistration {
    pub(crate) fn park_execution(&self) -> Option<ExecutionPark> {
        let scheduler = self.scheduler.upgrade()?;
        let mut state = scheduler.state.lock().expect("execution quota remains available");
        let task = state.tasks.get_mut(&self.task)?;

        if !task.execution_slot {
            return None;
        }

        assert!(matches!(task.dispatch, DispatchState::Running { .. }),
            "an execution slot belongs to an exclusive running dispatch");

        task.execution_slot = false;
        state.running_count -= 1;

        drop(state);

        let _ = scheduler.event.wake_handle().wake();

        Some(ExecutionPark { scheduler, task: self.task })
    }
}

impl Drop for ExecutionPark {
    fn drop(&mut self) {
        loop {
            let observed = self.scheduler.event.observe()
                .expect("execution quota retains its admitted native event");

            {
                let mut state = self.scheduler.state.lock().expect("execution quota remains available");

                if !state.tasks.contains_key(&self.task) {
                    return;
                }

                if state.running_count < self.scheduler.limits.tasks().get() {
                    let task = state.tasks.get_mut(&self.task).expect("the parked dispatch remains registered");

                    assert!(matches!(task.dispatch, DispatchState::Running { .. }) && !task.execution_slot,
                        "parking preserves exclusive dispatch ownership");

                    task.execution_slot = true;
                    state.running_count += 1;

                    return;
                }
            }

            self.scheduler.event.wait(observed, None)
                .expect("execution quota retains its admitted native event");
        }
    }
}
