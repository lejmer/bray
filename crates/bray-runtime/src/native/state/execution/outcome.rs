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
                self.cleanup_reports.transfer_owned(
                    CleanupIncidentProducer::Task(task.task.id()),
                    task.task.execution_origin(),
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
        if let Some(parent) = &task.cleanup_parent {
            // Direct composition retains cleanup incidents in its enclosing run. That run's
            // eventual outcome determines whether they belong to a panic report or the host sink.
            for incident in task.terminal.take_cleanup_incidents() {
                parent.record_cleanup_incident(incident);
            }

            return;
        }

        let producer = CleanupIncidentProducer::Task(task.task.id());

        let origin = task.task.execution_origin();

        for incident in task.terminal.take_cleanup_incidents().into_iter().rev() {
            self.cleanup_reports
                .transfer_owned(producer, origin, incident);
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
    fn composed_cleanup_incidents_stay_with_the_parent_until_its_boundary_resolves() {
        extern "C-unwind" fn complete(_: usize) -> NativeFrameProgress {
            NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 0)
        }

        extern "C-unwind" fn action(_: usize) {}
        extern "C-unwind" fn resolve(_: usize, _: NativeFrameExit) {}
        extern "C-unwind" fn move_completion(_: usize, _: usize) {}

        for composed in [false, true] {
            assert_eq!(
                initialize(NativeRuntimeConfiguration::new(2, 1)),
                NativeRuntimeStatus::SUCCESS
            );

            with_runtime(|runtime| {
                let parent = triomphe::Arc::new(crate::native::frame::NativeTerminalState::new());
                let child = runtime.allocate().task().unwrap();

                let frame = NativeProtectedFrame::new(
                    0,
                    bray_runtime_abi::NativeFrameMetadata::new(
                        [42; 32],
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
                        composed.then(|| triomphe::Arc::clone(&parent))
                    ),
                    NativeRuntimeStatus::SUCCESS
                );

                runtime
                    .with_started(child, |task| {
                        for ordinal in [7u32, 11] {
                            task.terminal.record_cleanup_incident(
                                crate::incident::OwnedCleanupIncident::host(Box::new(ordinal)),
                            );
                        }
                    })
                    .unwrap();

                let outcome = runtime.resolve_task(child);

                assert_eq!(outcome.state(), NativeRunState::COMPLETED);
                assert_eq!(runtime.observe(child), outcome);

                let incidents: Vec<_> = parent
                    .take_cleanup_incidents()
                    .into_iter()
                    .map(|incident| *incident.payload().downcast_ref::<u32>().unwrap())
                    .collect();

                assert_eq!(incidents, if composed { vec![7, 11] } else { Vec::new() });

                assert_eq!(
                    runtime.pending_cleanup_incidents(),
                    if composed { 0 } else { 2 }
                );

                assert_eq!(runtime.observe(child), outcome);
                assert!(parent.take_cleanup_incidents().is_empty());

                assert_eq!(
                    runtime.discard_cleanup_incidents(),
                    if composed { 0 } else { 2 }
                );

                assert_eq!(runtime.destroy_task(child), NativeRuntimeStatus::SUCCESS);
            })
            .unwrap();

            assert_eq!(shutdown(), NativeRuntimeStatus::SUCCESS);
        }
    }
}
