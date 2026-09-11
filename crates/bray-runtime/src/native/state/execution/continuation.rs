use std::sync::Arc;

use bray_runtime_abi::{NativeRunOutcome, NativeRuntimeStatus, NativeTaskHandle};

use super::super::binding::current_native_task;
use super::super::core::{NativeRuntime, NativeTaskSlot, StartedTask};

impl NativeRuntime {
    pub(in crate::native) fn compose_awaited(
        &self,
        mut transfer: crate::native::frame::NativeFrameTransfer,
    ) -> NativeRuntimeStatus {
        let Some(parent) = current_native_task() else {
            return NativeRuntimeStatus::INVALID_ARGUMENT;
        };

        self.with_started(parent, |task| {
            let mut awaited = task
                .awaited
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);

            if awaited.is_some() {
                return NativeRuntimeStatus::RUNTIME_FAILURE;
            }

            let allocation = self.allocate_frame_continuation(&transfer);

            let Some(child) = allocation.task() else {
                return allocation.status();
            };

            let status = self.start(
                child,
                &mut transfer,
                Some(triomphe::Arc::clone(&task.terminal)),
            );

            if !status.is_success() {
                return status;
            }

            *awaited = Some(child);

            NativeRuntimeStatus::SUCCESS
        })
        .unwrap_or_else(|status| status)
    }

    pub(in crate::native) fn resolve_awaited_terminal(
        &self,
        transfer: impl FnOnce(NativeRunOutcome) -> Result<(), NativeRuntimeStatus>,
    ) -> NativeRuntimeStatus {
        let Some(parent) = current_native_task() else {
            return NativeRuntimeStatus::INVALID_ARGUMENT;
        };

        let Some(child) = self.awaited_child(parent) else {
            return NativeRuntimeStatus::UNKNOWN_TASK;
        };

        if let Err(status) = self.transfer_task_outcome(child, transfer) {
            return status;
        }

        if let Err(status) = self.with_started(parent, |task| {
            task.awaited
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take();
        }) {
            return status;
        }

        self.destroy_task(child)
    }

    pub(in crate::native::state) fn awaited_child(
        &self,
        parent: NativeTaskHandle,
    ) -> Option<NativeTaskHandle> {
        self.with_started(parent, |task| {
            *task
                .awaited
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
        })
        .ok()
        .flatten()
    }

    pub(in crate::native::state::execution) fn register_continuation_wait(
        &self,
        owner: NativeTaskHandle,
        child: NativeTaskHandle,
    ) -> NativeRuntimeStatus {
        let (owner, task) = match self.retained_task_pair(owner, child) {
            Ok(tasks) => tasks,
            Err(status) => return status,
        };

        owner.continuation.register(child, &task.task)
    }

    pub(in crate::native::state::execution) fn register_task_waiter(
        &self,
        owner: NativeTaskHandle,
        child: NativeTaskHandle,
        wake: Arc<dyn crate::JoinWake>,
    ) -> NativeRuntimeStatus {
        let (owner, child) = match self.retained_task_pair(owner, child) {
            Ok(tasks) => tasks,
            Err(status) => return status,
        };

        // Terminal publication preserves the task record. Registration on that record wakes
        // immediately, including when publication raced the caller's earlier readiness check.
        let registration = match child.task.register_join_waiter(wake) {
            Ok(registration) => registration,
            Err(_) => return NativeRuntimeStatus::RUNTIME_FAILURE,
        };

        owner
            .waits
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(registration);

        NativeRuntimeStatus::SUCCESS
    }

    fn retained_task_pair(
        &self,
        owner: NativeTaskHandle,
        child: NativeTaskHandle,
    ) -> Result<(triomphe::Arc<StartedTask>, triomphe::Arc<StartedTask>), NativeRuntimeStatus> {
        let tasks = self
            .tasks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let retained = |handle| match tasks.get(&handle) {
            Some(NativeTaskSlot::Started(task) | NativeTaskSlot::Terminal { _task: task, .. }) => {
                Some(triomphe::Arc::clone(task))
            }
            _ => None,
        };

        let (Some(owner), Some(child)) = (retained(owner), retained(child)) else {
            return Err(NativeRuntimeStatus::UNKNOWN_TASK);
        };

        Ok((owner, child))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use bray_runtime_abi::{
        NativeFrameExit, NativeFrameProgress, NativeFrameProgressKind, NativeProtectedFrame,
        NativeRunState, NativeRuntimeConfiguration, NativeRuntimeStatus, NativeTaskHandle,
    };

    use crate::native::state::core::{initialize, shutdown, with_runtime};

    #[test]
    fn task_completion_suspension_retains_early_and_late_terminal_results() {
        extern "C-unwind" fn complete(_: usize) -> NativeFrameProgress {
            NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 0)
        }

        extern "C-unwind" fn observe(child: usize) -> NativeFrameProgress {
            let state = crate::current_task_execution_context()
                .unwrap()
                .state()
                .raw();

            if state == 0 {
                NativeFrameProgress::new(NativeFrameProgressKind::TASK_COMPLETION, 1, child)
            } else {
                assert_eq!(state, 1);

                complete(0)
            }
        }

        extern "C-unwind" fn action(_: usize) {}
        extern "C-unwind" fn resolve(_: usize, _: NativeFrameExit) {}
        extern "C-unwind" fn move_completion(_: usize, _: usize) {}

        for completed_early in [false, true] {
            assert_eq!(
                initialize(NativeRuntimeConfiguration::new(4, 1)),
                NativeRuntimeStatus::SUCCESS
            );

            with_runtime(|runtime| {
                let child = runtime.allocate().task().unwrap();
                let parent = runtime.allocate().task().unwrap();

                let child_frame = NativeProtectedFrame::new(
                    0,
                    bray_runtime_abi::NativeFrameMetadata::new(
                        [40; 32],
                        1,
                        0,
                        1,
                        0,
                        1,
                        crate::test_support::native_main_frame_state,
                    ),
                    complete,
                    complete,
                    action,
                    resolve,
                    move_completion,
                    action,
                );

                let parent_frame = NativeProtectedFrame::new(
                    usize::try_from(child.raw()).unwrap(),
                    bray_runtime_abi::NativeFrameMetadata::new(
                        [41; 32],
                        2,
                        0,
                        1,
                        0,
                        1,
                        crate::test_support::native_main_frame_state,
                    ),
                    observe,
                    observe,
                    action,
                    resolve,
                    move_completion,
                    action,
                );

                if completed_early {
                    assert_eq!(
                        runtime.start(
                            child,
                            &mut crate::native::frame::NativeFrameTransfer::new(child_frame),
                            None
                        ),
                        NativeRuntimeStatus::SUCCESS
                    );

                    assert_eq!(
                        runtime.resolve_task(child).state(),
                        NativeRunState::COMPLETED
                    );

                    assert_eq!(
                        runtime.start(
                            parent,
                            &mut crate::native::frame::NativeFrameTransfer::new(parent_frame),
                            None
                        ),
                        NativeRuntimeStatus::SUCCESS
                    );
                } else {
                    assert_eq!(
                        runtime.start(
                            parent,
                            &mut crate::native::frame::NativeFrameTransfer::new(parent_frame),
                            None
                        ),
                        NativeRuntimeStatus::SUCCESS
                    );

                    assert_eq!(
                        runtime.start(
                            child,
                            &mut crate::native::frame::NativeFrameTransfer::new(child_frame),
                            None
                        ),
                        NativeRuntimeStatus::SUCCESS
                    );
                }

                if !completed_early {
                    assert_eq!(runtime.drive_main_thread(), NativeRuntimeStatus::SUCCESS);

                    // Polling and cancellation retarget the same admission-owned continuation.
                    // A different child cannot displace the waiter protecting the current child.
                    assert_eq!(
                        runtime.register_continuation_wait(parent, child),
                        NativeRuntimeStatus::SUCCESS
                    );

                    assert_eq!(
                        runtime.register_continuation_wait(parent, parent),
                        NativeRuntimeStatus::RUNTIME_FAILURE
                    );

                    crate::test_support::with_allocation_failure(|| {
                        assert_eq!(
                            runtime.register_continuation_wait(parent, child),
                            NativeRuntimeStatus::SUCCESS
                        );
                    });

                    runtime
                        .with_started(parent, |task| {
                            assert!(task.waits.lock().unwrap().is_empty());
                        })
                        .unwrap();
                }

                // Each frame gets at most its initial entry and the observer's one resumption.
                for _ in 0..3 {
                    assert!(matches!(
                        runtime.drive_main_thread(),
                        NativeRuntimeStatus::SUCCESS | NativeRuntimeStatus::PENDING
                    ));
                }

                assert_eq!(runtime.observe(parent).state(), NativeRunState::COMPLETED);
                let completed = runtime.observe(child);
                assert_eq!(completed.state(), NativeRunState::COMPLETED);
                assert_eq!(runtime.observe(child), completed);
                assert_eq!(runtime.destroy_task(parent), NativeRuntimeStatus::SUCCESS);
                assert_eq!(runtime.destroy_task(child), NativeRuntimeStatus::SUCCESS);
            })
            .unwrap();

            assert_eq!(shutdown(), NativeRuntimeStatus::SUCCESS);
        }
    }

    #[test]
    fn waiter_registration_survives_terminal_publication_after_a_pending_observation() {
        extern "C-unwind" fn complete(_: usize) -> NativeFrameProgress {
            NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 0)
        }

        extern "C-unwind" fn action(_: usize) {}
        extern "C-unwind" fn resolve(_: usize, _: NativeFrameExit) {}
        extern "C-unwind" fn move_completion(_: usize, _: usize) {}

        assert_eq!(
            initialize(NativeRuntimeConfiguration::new(2, 1)),
            NativeRuntimeStatus::SUCCESS
        );

        with_runtime(|runtime| {
            let child = runtime.allocate().task().unwrap();

            let frame = NativeProtectedFrame::new(
                0,
                bray_runtime_abi::NativeFrameMetadata::new(
                    [39; 32],
                    1,
                    0,
                    1,
                    0,
                    1,
                    crate::test_support::native_main_frame_state,
                ),
                complete,
                complete,
                action,
                resolve,
                move_completion,
                action,
            );

            assert_eq!(
                runtime.start(
                    child,
                    &mut crate::native::frame::NativeFrameTransfer::new(frame),
                    None
                ),
                NativeRuntimeStatus::SUCCESS
            );

            assert_eq!(runtime.observe(child).state(), NativeRunState::PENDING);

            // Complete and publish between the caller's readiness check and registration.
            let completed = runtime.resolve_task(child);
            assert_eq!(completed.state(), NativeRunState::COMPLETED);
            let wakes = Arc::new(AtomicUsize::new(0));
            let wake_count = Arc::clone(&wakes);

            assert_eq!(
                runtime.register_task_waiter(
                    child,
                    child,
                    Arc::new(move || {
                        wake_count.fetch_add(1, Ordering::SeqCst);
                    })
                ),
                NativeRuntimeStatus::SUCCESS
            );

            assert_eq!(wakes.load(Ordering::SeqCst), 1);
            assert_eq!(runtime.observe(child), completed);

            let missing = NativeTaskHandle::new(u64::MAX).unwrap();

            assert_eq!(
                runtime.register_task_waiter(child, missing, Arc::new(|| {})),
                NativeRuntimeStatus::UNKNOWN_TASK
            );

            assert_eq!(
                runtime.register_task_waiter(missing, child, Arc::new(|| {})),
                NativeRuntimeStatus::UNKNOWN_TASK
            );

            assert_eq!(runtime.destroy_task(child), NativeRuntimeStatus::SUCCESS);
        })
        .unwrap();

        assert_eq!(shutdown(), NativeRuntimeStatus::SUCCESS);
    }
}
