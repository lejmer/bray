use bray_runtime_abi::{
    NativeExecutionLaneResult, NativeRootHandle, NativeRuntimeStatus, NativeTaskHandle,
};
use bray_runtime_model::ProtectedFrameStateId;

use crate::context::with_task_execution_context;
use crate::{
    FrameSuspensionKind, JoinWaitRegistration, RootCancellationHandle, TaskExecutionContext,
    TaskResumeStatus,
};

use super::super::binding::{current_thread_lanes, lane_result, with_native_task};
use super::super::core::{NativeRuntime, NativeTaskSlot, StartedTask};

impl NativeRuntime {
    pub(in crate::native) fn drive_main_thread(&self) -> NativeRuntimeStatus {
        let thread = self.thread.runtime().id();

        for lane in
            current_thread_lanes(thread, self.main_thread_lane, self.cleanup_workloads.get())
        {
            match self.scheduler.take_ready(lane) {
                Ok(Some(ready)) => return self.drive_ready(ready),
                Ok(None) => {}
                Err(_) => return NativeRuntimeStatus::RUNTIME_FAILURE,
            }
        }

        NativeRuntimeStatus::PENDING
    }

    pub(in crate::native) fn drive_ready(&self, ready: crate::ReadyTask) -> NativeRuntimeStatus {
        let selected = self
            .tasks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .find_map(|(handle, slot)| {
                let NativeTaskSlot::Started(task) = slot else {
                    return None;
                };

                if task.task.id() != ready.task() {
                    return None;
                }

                task.waits
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .retain(JoinWaitRegistration::is_pending);

                Some((*handle, triomphe::Arc::clone(task)))
            });

        let Some((handle, started)) = selected else {
            return NativeRuntimeStatus::RUNTIME_FAILURE;
        };

        let task = &started.task;

        // Notifications request another readiness check. Only a ready wait or deliverable
        // cancellation may advance generated code. Keep registrations armed across a stale wake.
        if !task.cancellation_observable() {
            let event_ready = match started.event_wait.is_ready() {
                Ok(ready) => ready,
                Err(_) => return NativeRuntimeStatus::RUNTIME_FAILURE,
            };

            if !event_ready || !started.continuation.is_ready() {
                let execution = ready.execution_state().clone();

                return ready
                    .suspend(execution)
                    .map_or(NativeRuntimeStatus::RUNTIME_FAILURE, |()| {
                        NativeRuntimeStatus::SUCCESS
                    });
            }
        }

        let wake = started.registration().wake_handle();

        started.continuation.disarm();
        started.event_wait.disarm();

        // The resume context retains one wake handle while yield publication uses the other.
        let context = TaskExecutionContext::new(
            task.id(),
            ready.state(),
            task.cancellation_context().clone(),
            task.output_context().clone(),
            ready.lane(),
            wake.clone(),
        );

        let status = crate::native::incident::with_incident_owner(&started.terminal, || {
            with_native_task(handle, || {
                with_task_execution_context(context, || task.resume())
            })
        });

        let status = match status {
            Ok(status) => status,
            Err(error) => {
                let terminal = match error {
                    crate::TaskResumeError::NotResumable(state) => state.is_terminal(),
                    crate::TaskResumeError::RuntimeFailed(_)
                    | crate::TaskResumeError::UnknownSuspensionState(_) => true,
                    crate::TaskResumeError::AlreadyRunning
                    | crate::TaskResumeError::SynchronizationPoisoned => false,
                };

                if terminal {
                    let _ = ready.complete();
                }

                return NativeRuntimeStatus::RUNTIME_FAILURE;
            }
        };

        match status {
            TaskResumeStatus::Suspended(suspension, execution) => {
                let kind = suspension.kind();

                if kind == FrameSuspensionKind::TaskEvent {
                    return self.suspend_on_task_event(ready, &started, suspension, execution);
                }

                let child = match kind {
                    FrameSuspensionKind::TaskCompletion => {
                        let Some(child) = suspension
                            .payload()
                            .and_then(|payload| u64::try_from(payload).ok())
                            .and_then(NativeTaskHandle::new)
                        else {
                            return NativeRuntimeStatus::INVALID_ARGUMENT;
                        };

                        Some(child)
                    }
                    FrameSuspensionKind::Awaited => self.awaited_child(handle),
                    FrameSuspensionKind::Yield => None,
                    FrameSuspensionKind::TaskEvent => {
                        unreachable!("event suspension was handled above")
                    }
                };

                if let Some(child) = child {
                    // Arm while dispatch still owns the parent. Publication or cancellation can
                    // queue its next state, but no worker may resume it before registration finishes.
                    let status = self.register_continuation_wait(handle, child);

                    if !status.is_success() {
                        return status;
                    }
                }

                if ready.suspend(execution).is_err() {
                    return NativeRuntimeStatus::RUNTIME_FAILURE;
                }

                if kind == FrameSuspensionKind::Yield {
                    wake.wake()
                        .map_or(NativeRuntimeStatus::RUNTIME_FAILURE, |_| {
                            NativeRuntimeStatus::SUCCESS
                        })
                } else {
                    NativeRuntimeStatus::SUCCESS
                }
            }
            TaskResumeStatus::Terminal(_) => ready
                .complete()
                .map_or(NativeRuntimeStatus::RUNTIME_FAILURE, |()| {
                    NativeRuntimeStatus::SUCCESS
                }),
        }
    }

    fn suspend_on_task_event(
        &self,
        ready: crate::ReadyTask,
        started: &StartedTask,
        suspension: crate::FrameSuspension,
        execution: crate::FrameExecutionState,
    ) -> NativeRuntimeStatus {
        let Some(identity) = suspension.payload() else {
            return NativeRuntimeStatus::RUNTIME_FAILURE;
        };

        let Some(event) = crate::native::event::event(&self.core, identity) else {
            return NativeRuntimeStatus::RUNTIME_FAILURE;
        };

        // Keep the dispatch running until registration ownership is published. An immediate
        // event wake then becomes a pending scheduler wake that suspension releases.
        if started.event_wait.register(&event, identity).is_err() {
            return NativeRuntimeStatus::RUNTIME_FAILURE;
        }

        if ready.suspend(execution).is_err() {
            started.event_wait.disarm();

            return NativeRuntimeStatus::RUNTIME_FAILURE;
        }

        NativeRuntimeStatus::SUCCESS
    }

    pub(in crate::native) fn wake(&self, handle: NativeTaskHandle) -> NativeRuntimeStatus {
        self.with_started(handle, |task| {
            task.registration()
                .wake_handle()
                .wake()
                .map_or(NativeRuntimeStatus::RUNTIME_FAILURE, |_| {
                    NativeRuntimeStatus::SUCCESS
                })
        })
        .unwrap_or_else(|status| status)
    }

    pub(in crate::native) fn request_cancellation(
        &self,
        handle: NativeTaskHandle,
    ) -> NativeRuntimeStatus {
        self.with_started(handle, |task| {
            task.task.request_cancellation();

            NativeRuntimeStatus::SUCCESS
        })
        .unwrap_or_else(|status| status)
    }

    pub(in crate::native) fn request_root_cancellation(
        &self,
        root: NativeRootHandle,
    ) -> NativeRuntimeStatus {
        let Some(task) = NativeTaskHandle::new(root.raw()) else {
            return NativeRuntimeStatus::UNKNOWN_TASK;
        };

        self.request_cancellation(task)
    }

    pub(in crate::native) fn root_cancellation(
        &self,
        root: NativeRootHandle,
    ) -> Result<RootCancellationHandle, NativeRuntimeStatus> {
        let task = NativeTaskHandle::new(root.raw()).ok_or(NativeRuntimeStatus::UNKNOWN_TASK)?;

        self.with_started(task, |task| {
            RootCancellationHandle::new(task.task.cancellation_context().clone())
        })
    }

    pub(in crate::native) fn lane(
        &self,
        handle: NativeTaskHandle,
        state: u32,
    ) -> NativeExecutionLaneResult {
        self.with_started(handle, |task| {
            task.registration()
                .lane(ProtectedFrameStateId::new(state))
                .map(lane_result)
                .unwrap_or_else(|_| {
                    NativeExecutionLaneResult::failure(NativeRuntimeStatus::RUNTIME_FAILURE)
                })
        })
        .unwrap_or_else(NativeExecutionLaneResult::failure)
    }

    pub(in crate::native::state::execution) fn with_started<T>(
        &self,
        handle: NativeTaskHandle,
        callback: impl FnOnce(&StartedTask) -> T,
    ) -> Result<T, NativeRuntimeStatus> {
        let task = {
            let tasks = self
                .tasks
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);

            let Some(NativeTaskSlot::Started(task)) = tasks.get(&handle) else {
                return Err(NativeRuntimeStatus::UNKNOWN_TASK);
            };

            triomphe::Arc::clone(task)
        };

        Ok(callback(&task))
    }

    pub(in crate::native::state::execution) fn wait_main_thread(&self) -> NativeRuntimeStatus {
        let ready = self.drive_main_thread();

        if ready != NativeRuntimeStatus::PENDING {
            return ready;
        }

        let thread = self.thread.runtime().id();

        let lanes =
            current_thread_lanes(thread, self.main_thread_lane, self.cleanup_workloads.get());

        let deadline =
            bray_platform::MonotonicClock.deadline_after(std::time::Duration::from_millis(10));

        match self.scheduler.wait_ready_from(lanes, deadline) {
            Ok(Some(ready)) => self.drive_ready(ready),
            Ok(None) => NativeRuntimeStatus::SUCCESS,
            Err(_) => NativeRuntimeStatus::RUNTIME_FAILURE,
        }
    }
}

#[cfg(test)]
mod tests {
    use bray_runtime_abi::{
        NativeFrameExit, NativeFrameProgress, NativeFrameProgressKind, NativeProtectedFrame,
        NativeRunState, NativeRuntimeConfiguration, NativeRuntimeStatus,
    };

    use crate::native::state::core::{initialize, shutdown, with_runtime};

    #[test]
    fn stale_wakes_preserve_event_and_join_waits_until_ready_or_cancelled() {
        extern "C-unwind" fn wait_event(event: usize) -> NativeFrameProgress {
            wait_at_entry(NativeFrameProgressKind::TASK_EVENT, event)
        }

        extern "C-unwind" fn wait_child(child: usize) -> NativeFrameProgress {
            wait_at_entry(NativeFrameProgressKind::TASK_COMPLETION, child)
        }

        fn wait_at_entry(kind: NativeFrameProgressKind, payload: usize) -> NativeFrameProgress {
            let state = crate::current_task_execution_context()
                .unwrap()
                .state()
                .raw();

            if state == 0 {
                NativeFrameProgress::new(kind, 1, payload)
            } else {
                assert_eq!(state, 1);

                NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 0)
            }
        }

        extern "C-unwind" fn cancel(_: usize) -> NativeFrameProgress {
            NativeFrameProgress::new(NativeFrameProgressKind::CANCELLED, 0, 0)
        }

        extern "C-unwind" fn action(_: usize) {}
        extern "C-unwind" fn resolve(_: usize, _: NativeFrameExit) {}
        extern "C-unwind" fn move_completion(_: usize, _: usize) {}

        for cancelled in [false, true] {
            assert_eq!(
                initialize(NativeRuntimeConfiguration::new(2, 1)),
                NativeRuntimeStatus::SUCCESS
            );

            with_runtime(|runtime| {
                let event = crate::native::event::create(&runtime.core);
                let child = runtime.allocate().task().unwrap();
                let parent = runtime.allocate().task().unwrap();

                for (handle, payload, resume) in [
                    (
                        child,
                        event,
                        wait_event as extern "C-unwind" fn(usize) -> NativeFrameProgress,
                    ),
                    (parent, usize::try_from(child.raw()).unwrap(), wait_child),
                ] {
                    let frame = NativeProtectedFrame::new(
                        payload,
                        bray_runtime_abi::NativeFrameMetadata::new(
                            [43; 32],
                            2,
                            0,
                            1,
                            0,
                            1,
                            crate::test_support::native_main_frame_state,
                        ),
                        resume,
                        cancel,
                        action,
                        resolve,
                        move_completion,
                        action,
                    );

                    assert_eq!(
                        runtime.start(
                            handle,
                            &mut crate::native::frame::NativeFrameTransfer::new(frame),
                            None,
                        ),
                        NativeRuntimeStatus::SUCCESS
                    );
                }

                for _ in 0..2 {
                    assert_eq!(runtime.drive_main_thread(), NativeRuntimeStatus::SUCCESS);
                }

                for handle in [child, parent] {
                    assert_eq!(runtime.wake(handle), NativeRuntimeStatus::SUCCESS);
                    assert_eq!(runtime.drive_main_thread(), NativeRuntimeStatus::SUCCESS);
                    assert_eq!(runtime.observe(handle).state(), NativeRunState::PENDING);
                }

                assert_eq!(runtime.drive_main_thread(), NativeRuntimeStatus::PENDING);

                if cancelled {
                    let shield = runtime
                        .with_started(child, |task| task.task.cancellation_context().shield())
                        .unwrap();

                    assert_eq!(
                        runtime.request_cancellation(child),
                        NativeRuntimeStatus::SUCCESS
                    );

                    assert_eq!(runtime.wake(child), NativeRuntimeStatus::SUCCESS);
                    assert_eq!(runtime.drive_main_thread(), NativeRuntimeStatus::SUCCESS);
                    assert_eq!(runtime.observe(child).state(), NativeRunState::PENDING);
                    assert_eq!(runtime.drive_main_thread(), NativeRuntimeStatus::PENDING);
                    drop(shield);
                } else {
                    assert_eq!(
                        crate::native::event::signal(event),
                        NativeRuntimeStatus::SUCCESS
                    );
                }

                // Readiness/cancellation reaches the child, and child termination wakes its owner.
                for _ in 0..2 {
                    assert_eq!(runtime.drive_main_thread(), NativeRuntimeStatus::SUCCESS);
                }

                assert_eq!(
                    runtime.observe(child).state(),
                    if cancelled {
                        NativeRunState::CANCELLED
                    } else {
                        NativeRunState::COMPLETED
                    }
                );

                assert_eq!(runtime.observe(parent).state(), NativeRunState::COMPLETED);

                for handle in [parent, child] {
                    assert_eq!(runtime.destroy_task(handle), NativeRuntimeStatus::SUCCESS);
                }

                assert_eq!(
                    crate::native::event::destroy(&runtime.core, event),
                    NativeRuntimeStatus::SUCCESS
                );
            })
            .unwrap();

            assert_eq!(shutdown(), NativeRuntimeStatus::SUCCESS);
        }
    }
}
