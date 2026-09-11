use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use bray_runtime_abi::{
    NativeRootHandle, NativeRunOutcome, NativeRunState, NativeRuntimeStatus, NativeTaskHandle,
    NativeWakeCallback,
};

use crate::{CleanupIncidentProducer, TaskObservationError};

use super::super::binding::{current_native_task, runtime_failure, task_outcome};
use super::super::core::{NativeRuntime, NativeTaskSlot, StartedTask, TerminalOutcome};

struct TaskObservationClaim<'a>(&'a AtomicBool);

impl Drop for TaskObservationClaim<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

impl NativeRuntime {
    pub(in crate::native) fn observe_root(&self, root: NativeRootHandle) -> NativeRunOutcome {
        let Some(task) = NativeTaskHandle::new(root.raw()) else {
            return runtime_failure(NativeRuntimeStatus::UNKNOWN_TASK);
        };

        self.resolve_task(task)
    }

    pub(in crate::native) fn resolve_task(&self, task: NativeTaskHandle) -> NativeRunOutcome {
        loop {
            let outcome = self.observe(task);

            if outcome.state() != NativeRunState::PENDING {
                return outcome;
            }

            let status = self.wait_main_thread();

            if !status.is_success() {
                let outcome = self.observe(task);

                return if outcome.state() == NativeRunState::PENDING {
                    runtime_failure(status)
                } else {
                    outcome
                };
            }
        }
    }

    pub(in crate::native) fn resolve_root_completion(
        &self,
        root: NativeRootHandle,
    ) -> NativeRuntimeStatus {
        let Some(task) = NativeTaskHandle::new(root.raw()) else {
            return NativeRuntimeStatus::UNKNOWN_TASK;
        };

        self.destroy_task(task)
    }

    pub(in crate::native) fn destroy_task(&self, task: NativeTaskHandle) -> NativeRuntimeStatus {
        let mut tasks = self
            .tasks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        match tasks.get(&task) {
            Some(NativeTaskSlot::Terminal {
                outcome: TerminalOutcome::Transferring | TerminalOutcome::Borrowed(_),
                ..
            }) => NativeRuntimeStatus::PENDING,
            Some(NativeTaskSlot::Terminal { .. }) => {
                let removed = tasks.remove(&task);

                if let Some(slot) = &removed {
                    self.release_admission(slot);
                }

                drop(tasks);
                drop(removed);

                NativeRuntimeStatus::SUCCESS
            }
            Some(NativeTaskSlot::Started(_))
            | Some(NativeTaskSlot::Allocated(_) | NativeTaskSlot::Starting(_)) => {
                NativeRuntimeStatus::PENDING
            }
            None => NativeRuntimeStatus::UNKNOWN_TASK,
        }
    }

    pub(in crate::native) fn report_cleanup_incidents(&self) -> NativeRuntimeStatus {
        crate::native::incident::report_cleanup_incidents(&self.cleanup_reports)
    }

    pub(in crate::native) fn discard_cleanup_incidents(&self) -> usize {
        let mut count = 0;

        self.cleanup_reports.drain(|_| {
            count += 1;
        });

        count
    }

    #[cfg(test)]

    pub(in crate::native) fn pending_cleanup_incidents(&self) -> usize {
        self.cleanup_reports.pending_count()
    }

    pub(in crate::native) fn join(
        &self,
        handle: NativeTaskHandle,
        callback: NativeWakeCallback,
        context: usize,
    ) -> NativeRunOutcome {
        let outcome = self.observe(handle);

        if outcome.state() != NativeRunState::PENDING {
            return outcome;
        }

        let owner = current_native_task().unwrap_or(handle);

        let status = self.register_task_waiter(owner, handle, Arc::new(move || callback(context)));

        if status.is_success() {
            outcome
        } else {
            runtime_failure(status)
        }
    }

    pub(in crate::native) fn observe(&self, handle: NativeTaskHandle) -> NativeRunOutcome {
        // A running frame may need the task table while observation waits for its task lock.
        let task = {
            let tasks = self
                .tasks
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);

            match tasks.get(&handle) {
                Some(NativeTaskSlot::Started(task)) => {
                    if task
                        .observation_claimed
                        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                        .is_err()
                    {
                        return NativeRunOutcome::new(NativeRunState::PENDING, 0);
                    }

                    triomphe::Arc::clone(task)
                }
                Some(NativeTaskSlot::Terminal { outcome, .. }) => {
                    return match outcome {
                        TerminalOutcome::Available(outcome) => *outcome,
                        TerminalOutcome::Transferring | TerminalOutcome::Borrowed(_) => {
                            NativeRunOutcome::new(NativeRunState::PENDING, 0)
                        }
                        TerminalOutcome::Consumed => {
                            runtime_failure(NativeRuntimeStatus::INVALID_ARGUMENT)
                        }
                    };
                }
                Some(NativeTaskSlot::Allocated(_) | NativeTaskSlot::Starting(_)) | None => {
                    return runtime_failure(NativeRuntimeStatus::UNKNOWN_TASK);
                }
            }
        };

        let _claim = TaskObservationClaim(&task.observation_claimed);

        let outcome = match task.task.take_outcome() {
            Ok(outcome) => Some(outcome),
            Err(TaskObservationError::RuntimeFailed(_)) => None,
            Err(TaskObservationError::Pending) => {
                return NativeRunOutcome::new(NativeRunState::PENDING, 0);
            }
            Err(
                TaskObservationError::AlreadyObserved
                | TaskObservationError::WaiterIdentityExhausted
                | TaskObservationError::WaiterAllocationFailed
                | TaskObservationError::OwnerAlreadyWaiting
                | TaskObservationError::SynchronizationPoisoned,
            ) => return runtime_failure(NativeRuntimeStatus::RUNTIME_FAILURE),
        };

        task.waits
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();

        task.continuation.clear();

        let outcome = match outcome {
            Some(outcome) => task_outcome(outcome, &task.terminal).unwrap_or_else(|panic| {
                self.transfer_task_incident(
                    &task,
                    crate::incident::OwnedCleanupIncident::host(Box::new(panic)),
                );

                runtime_failure(NativeRuntimeStatus::PANICKED)
            }),
            None => runtime_failure(NativeRuntimeStatus::RUNTIME_FAILURE),
        };

        self.transfer_cleanup_incidents(&task);

        self.tasks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(
                handle,
                NativeTaskSlot::Terminal {
                    outcome: TerminalOutcome::Available(outcome),
                    _task: triomphe::Arc::clone(&task),
                },
            );

        outcome
    }

    fn transfer_cleanup_incidents(&self, task: &StartedTask) {
        for incident in task.terminal.take_cleanup_incidents() {
            self.transfer_task_incident(task, incident);
        }
    }

    fn transfer_task_incident(
        &self,
        task: &StartedTask,
        incident: crate::incident::OwnedCleanupIncident,
    ) {
        let incident = if task.admission == crate::task::TaskAdmissionKind::Continuation {
            // Cleanup driving returns incidents to the owner whose obligation it settles.
            match crate::native::incident::retain_cleanup_incident(incident) {
                Ok(()) => return,
                Err(incident) => incident,
            }
        } else {
            incident
        };

        self.cleanup_reports.transfer_owned(
            CleanupIncidentProducer::Task(task.task.id()),
            task.task.execution_origin(),
            incident,
        );
    }
}

#[cfg(test)]
mod tests {
    use crate::native::frame::NativeFrameTransfer;
    use crate::native::state::{initialize, shutdown, with_runtime};
    use bray_runtime_abi::{
        NativeFrameExit, NativeFrameMetadata, NativeFrameProgress, NativeFrameProgressKind,
        NativeProtectedFrame, NativeRunState, NativeRuntimeConfiguration, NativeRuntimeStatus,
    };
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn composed_cleanup_incidents_stay_with_the_run_until_its_boundary_resolves() {
        static ENTERED: AtomicBool = AtomicBool::new(false);
        extern "C-unwind" fn action(_: usize) {}
        extern "C-unwind" fn resolve(_: usize, _: NativeFrameExit) {}
        extern "C-unwind" fn move_completion(_: usize, _: usize) {}

        fn frame(
            resume: extern "C-unwind" fn(usize) -> NativeFrameProgress,
        ) -> NativeProtectedFrame {
            NativeProtectedFrame::new(
                0,
                NativeFrameMetadata::new(
                    [42; 32],
                    2,
                    0,
                    1,
                    0,
                    1,
                    crate::test_support::native_main_frame_state,
                ),
                resume,
                resume,
                action,
                resolve,
                move_completion,
                action,
            )
        }

        extern "C-unwind" fn child(_: usize) -> NativeFrameProgress {
            let payloads: [Box<dyn std::any::Any + Send>; 2] = [Box::new(7u32), Box::new(11u64)];

            for payload in payloads {
                assert!(
                    crate::native::incident::retain_cleanup_incident(
                        crate::incident::OwnedCleanupIncident::host(payload)
                    )
                    .is_ok()
                );
            }

            NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 0)
        }

        extern "C-unwind" fn parent(_: usize) -> NativeFrameProgress {
            with_runtime(|runtime| {
                assert_eq!(runtime.pending_cleanup_incidents(), 0);

                if !ENTERED.swap(true, Ordering::Relaxed) {
                    assert_eq!(
                        runtime.compose_awaited(NativeFrameTransfer::new(frame(child))),
                        NativeRuntimeStatus::SUCCESS
                    );

                    return NativeFrameProgress::new(NativeFrameProgressKind::SUSPENDED, 1, 0);
                }

                assert_eq!(
                    runtime.resolve_awaited_terminal(|outcome| {
                        assert_eq!(outcome.state(), NativeRunState::COMPLETED);
                        Ok(())
                    }),
                    NativeRuntimeStatus::SUCCESS
                );

                assert_eq!(runtime.pending_cleanup_incidents(), 0);

                NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 0)
            })
            .unwrap()
        }

        for continuation in [false, true] {
            ENTERED.store(false, Ordering::Relaxed);

            assert_eq!(
                initialize(NativeRuntimeConfiguration::new(1, 1)),
                NativeRuntimeStatus::SUCCESS
            );

            with_runtime(|runtime| {
                let allocation = if continuation {
                    runtime.allocate_continuation()
                } else {
                    runtime.allocate()
                };

                let root = allocation.task().unwrap();

                assert_eq!(
                    runtime.start(root, &mut NativeFrameTransfer::new(frame(parent))),
                    NativeRuntimeStatus::SUCCESS
                );

                let ((), incidents) = crate::native::with_cleanup_incident_owner(|_| {
                    assert_eq!(
                        runtime.resolve_task(root).state(),
                        NativeRunState::COMPLETED
                    );

                    assert_eq!(
                        runtime.pending_cleanup_incidents(),
                        if continuation { 0 } else { 2 }
                    );

                    runtime.observe(root);

                    assert_eq!(
                        runtime.pending_cleanup_incidents(),
                        if continuation { 0 } else { 2 }
                    );
                });

                let owned = incidents
                    .iter()
                    .map(|incident| incident.payload().is::<u32>())
                    .collect::<Vec<_>>();

                let mut reported = Vec::new();

                runtime
                    .cleanup_reports
                    .drain(|incident| reported.push(incident.payload_is::<u32>()));

                assert_eq!(
                    owned,
                    if continuation {
                        vec![true, false]
                    } else {
                        vec![]
                    }
                );

                assert_eq!(
                    reported,
                    if continuation {
                        vec![]
                    } else {
                        vec![true, false]
                    }
                );

                assert_eq!(runtime.destroy_task(root), NativeRuntimeStatus::SUCCESS);
            })
            .unwrap();

            assert_eq!(shutdown(), NativeRuntimeStatus::SUCCESS);
        }
    }
}
