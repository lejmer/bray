use std::sync::{Arc, Mutex, OnceLock};

use bray_runtime_model::ProtectedFrameStateId;

use crate::{JoinWake, SchedulerError, TaskId, TaskStartError, TaskWakeHandle};

pub(crate) enum JoinNotification {
    Callback(Arc<dyn JoinWake>),
    Continuation(triomphe::Arc<TaskWaitWake<TaskId>>),
}

impl std::fmt::Debug for JoinNotification {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("JoinNotification")
            .finish_non_exhaustive()
    }
}

impl From<Arc<dyn JoinWake>> for JoinNotification {
    fn from(wake: Arc<dyn JoinWake>) -> Self {
        Self::Callback(wake)
    }
}

impl<W: JoinWake> From<Arc<W>> for JoinNotification {
    fn from(wake: Arc<W>) -> Self {
        Self::Callback(wake)
    }
}

impl JoinWake for JoinNotification {
    fn wake(&self, task: TaskId) {
        match self {
            Self::Callback(wake) => wake.wake(task),
            Self::Continuation(wake) => wake.wake(task),
        }
    }
}

/// One admission-owned wake record that rejects notifications from a replaced wait source.
pub(crate) struct TaskWaitWake<S> {
    wake: OnceLock<TaskWakeHandle>,
    target: Mutex<Option<(S, ProtectedFrameStateId)>>,
}

impl<S: Copy + Eq> TaskWaitWake<S> {
    pub(crate) fn reserve() -> Result<triomphe::Arc<Self>, TaskStartError> {
        #[cfg(test)]
        if crate::test_support::allocation_should_fail() {
            return Err(TaskStartError::AllocationFailed);
        }

        triomphe::Arc::try_new(Self {
            wake: OnceLock::new(),
            target: Mutex::new(None),
        })
        .map_err(|_| TaskStartError::AllocationFailed)
    }

    /// Binds preallocated notification storage before the owning task becomes executable.
    pub(crate) fn bind(&self, wake: TaskWakeHandle) {
        assert!(
            self.wake.set(wake).is_ok(),
            "a wait record must bind exactly once"
        );
    }

    fn bound_wake(&self) -> &TaskWakeHandle {
        self.wake
            .get()
            .unwrap_or_else(|| unreachable!("a wait record must bind before it is armed"))
    }

    pub(crate) fn arm(&self, source: S, state: ProtectedFrameStateId) {
        *self
            .target
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some((source, state));
    }

    pub(crate) fn clear(&self) {
        self.target
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
    }

    pub(crate) fn disarm(&self) -> Result<(), SchedulerError> {
        let mut target = self
            .target
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        if let Some((_, state)) = target.take() {
            self.bound_wake().withdraw_pending_wake(state)?;
        }

        Ok(())
    }
    pub(crate) fn notify(&self, source: S) {
        // Selection and queue publication must stay together so a delayed notification cannot
        // publish an old state after this record has been armed for a replacement wait source.
        let target = self
            .target
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        if let Some((expected, state)) = *target
            && expected == source
        {
            let _ = self.bound_wake().wake(state);
        }
    }
}

impl JoinWake for TaskWaitWake<TaskId> {
    fn wake(&self, task: TaskId) {
        self.notify(task);
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;

    use bray_platform::RuntimeThreadScope;
    use bray_runtime_model::{ProtectedFrameStateId, RuntimeCapability};

    use super::TaskWaitWake;
    use crate::test_support::{TestFrame, register_task};
    use crate::{FrameSuspension, JoinWake, Scheduler, SchedulerLimits, TaskControlBlock};

    #[test]
    fn delayed_notifications_cannot_wake_a_replacement_childs_continuation() {
        let wake = TaskWaitWake::reserve().unwrap();
        let runtime = RuntimeThreadScope::enter().unwrap();
        let thread = runtime.runtime().id();

        let scheduler = Scheduler::new(
            [
                RuntimeCapability::CooperativeExecution,
                RuntimeCapability::MigratableLanes,
            ],
            thread,
            SchedulerLimits::new(NonZeroUsize::new(1).unwrap(), NonZeroUsize::new(1).unwrap()),
        );

        let parent = TaskControlBlock::start(TestFrame::suspending_then_completing(0)).unwrap();
        let first_child = TaskControlBlock::start(TestFrame::completing(1)).unwrap();
        let next_child = TaskControlBlock::start(TestFrame::completing(2)).unwrap();
        let registration = register_task(&scheduler, &parent, thread);
        crate::test_support::with_allocation_failure(|| wake.bind(registration.wake_handle()));
        let initial = ProtectedFrameStateId::new(0);
        let resumed = ProtectedFrameStateId::new(1);

        wake.arm(first_child.id(), initial);
        wake.wake(first_child.id());

        let ready = scheduler
            .take_ready(registration.lane(initial).unwrap())
            .unwrap()
            .unwrap();

        // A duplicate may arrive after dequeue but before the native driver enters the frame.
        wake.wake(first_child.id());
        wake.disarm().unwrap();
        wake.wake(first_child.id());

        wake.arm(next_child.id(), resumed);
        wake.wake(first_child.id());
        ready.suspend(FrameSuspension::new(resumed)).unwrap();

        assert!(
            scheduler
                .take_ready(registration.lane(resumed).unwrap())
                .unwrap()
                .is_none()
        );

        wake.wake(next_child.id());

        let ready = scheduler
            .take_ready(registration.lane(resumed).unwrap())
            .unwrap()
            .unwrap();

        assert_eq!(ready.state(), resumed);
        wake.disarm().unwrap();
        wake.wake(first_child.id());
        wake.wake(next_child.id());
        ready.complete().unwrap();

        assert!(
            scheduler
                .take_ready(registration.lane(resumed).unwrap())
                .unwrap()
                .is_none()
        );
    }
}
