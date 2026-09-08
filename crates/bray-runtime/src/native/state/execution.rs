use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use bray_runtime_abi::{
    NativeExecutionLaneResult, NativeProtectedFrame, NativeRootHandle, NativeRunOutcome,
    NativeRunState, NativeRuntimeStatus, NativeTaskAllocation, NativeTaskHandle,
    NativeWakeCallback,
};
use bray_runtime_model::ProtectedFrameStateId;

use crate::context::with_task_execution_context;
use crate::{
    CleanupIncidentOrigin, CleanupIncidentProducer, FrameSuspensionKind, JoinWaitRegistration,
    RootCancellationHandle, TaskExecutionContext, TaskObservationError, TaskResumeStatus,
};

use super::binding::{
    CleanupWorkloadScope, current_native_task, current_thread_lanes, lane_result, runtime_failure,
    task_outcome, with_native_task,
};
use super::core::{NativeRuntime, NativeTaskSlot, StartedTask, TerminalOutcome};

struct TaskObservationClaim<'a>(&'a AtomicBool);

impl Drop for TaskObservationClaim<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

impl NativeRuntime {
    pub(in crate::native) fn with_cleanup_driving<T>(&self, callback: impl FnOnce() -> T) -> T {
        let previous = self.cleanup_workloads.replace(true);

        let _workloads = CleanupWorkloadScope {
            runtime: self,
            previous,
        };

        callback()
    }

    pub(in crate::native) fn allocate(&self) -> NativeTaskAllocation {
        self.allocate_kind(crate::task::TaskAdmissionKind::Independent)
    }

    pub(in crate::native) fn allocate_continuation(&self) -> NativeTaskAllocation {
        self.allocate_kind(crate::task::TaskAdmissionKind::Continuation)
    }

    fn allocate_kind(&self, kind: crate::task::TaskAdmissionKind) -> NativeTaskAllocation {
        let mut tasks = self
            .tasks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        if kind == crate::task::TaskAdmissionKind::Independent
            && self.independent_tasks.load(Ordering::Relaxed) >= self.task_capacity.get()
        {
            return NativeTaskAllocation::failure(NativeRuntimeStatus::RUNTIME_FAILURE);
        }

        let next = self.next_task.load(Ordering::Relaxed);

        let Some(handle) = NativeTaskHandle::new(next) else {
            return NativeTaskAllocation::failure(NativeRuntimeStatus::RUNTIME_FAILURE);
        };

        let Some(next) = next.checked_add(1) else {
            return NativeTaskAllocation::failure(NativeRuntimeStatus::RUNTIME_FAILURE);
        };

        self.next_task.store(next, Ordering::Relaxed);
        tasks.insert(handle, NativeTaskSlot::Allocated(kind));

        if kind == crate::task::TaskAdmissionKind::Independent {
            self.independent_tasks.fetch_add(1, Ordering::Relaxed);
        }

        NativeTaskAllocation::success(handle)
    }

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

                Some((*handle, Arc::clone(task)))
            });

        let Some((handle, started)) = selected else {
            return NativeRuntimeStatus::RUNTIME_FAILURE;
        };

        let task = &started.task;
        let wake = started.registration.wake_handle();

        started
            .event_wait
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();

        // The resume context retains one wake handle while yield publication uses the other.
        let context = TaskExecutionContext::new(
            task.id(),
            ready.state(),
            task.cancellation_context().clone(),
            task.output_context().clone(),
            ready.lane(),
            wake.clone(),
        );

        let status = {
            let _incidents = super::super::incident::IncidentOwnerScope::enter(&started.terminal);

            with_native_task(handle, || {
                with_task_execution_context(context, || task.resume())
            })
        };

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
            TaskResumeStatus::Suspended(suspension) => {
                let state = suspension.state();
                let kind = suspension.kind();

                if kind == FrameSuspensionKind::TaskEvent {
                    return self.suspend_on_task_event(ready, &started, suspension, wake);
                }

                if kind == FrameSuspensionKind::TaskCompletion {
                    let Some(child) = suspension
                        .payload()
                        .and_then(|payload| u64::try_from(payload).ok())
                        .and_then(NativeTaskHandle::new)
                    else {
                        return NativeRuntimeStatus::INVALID_ARGUMENT;
                    };

                    // Publish registration while dispatch is running. An already-terminal child
                    // records a pending wake that becomes runnable when suspension is committed.
                    let status = self.register_task_waiter(
                        handle,
                        child,
                        Arc::new(move || {
                            let _ = wake.wake(state);
                        }),
                    );

                    if !status.is_success() {
                        return status;
                    }

                    return ready
                        .suspend(suspension)
                        .map_or(NativeRuntimeStatus::RUNTIME_FAILURE, |()| {
                            NativeRuntimeStatus::SUCCESS
                        });
                }

                if ready.suspend(suspension).is_err() {
                    return NativeRuntimeStatus::RUNTIME_FAILURE;
                }

                match kind {
                    FrameSuspensionKind::Awaited => self.register_awaited_wake(handle, state),
                    FrameSuspensionKind::Yield => wake
                        .wake(state)
                        .map_or(NativeRuntimeStatus::RUNTIME_FAILURE, |_| {
                            NativeRuntimeStatus::SUCCESS
                        }),
                    FrameSuspensionKind::TaskEvent | FrameSuspensionKind::TaskCompletion => {
                        unreachable!("task suspension must publish its registration first")
                    }
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
        wake: crate::TaskWakeHandle,
    ) -> NativeRuntimeStatus {
        let state = suspension.state();

        let Some(identity) = suspension.payload() else {
            return NativeRuntimeStatus::RUNTIME_FAILURE;
        };

        let Some(event) = super::super::event::event(&self.core, identity) else {
            return NativeRuntimeStatus::RUNTIME_FAILURE;
        };

        let Ok((generation, _)) = event.observation() else {
            return NativeRuntimeStatus::RUNTIME_FAILURE;
        };

        // Keep the dispatch running until registration ownership is published. An immediate
        // event wake then becomes a pending scheduler wake that suspension releases.
        let Ok(registration) = event.register(
            generation,
            Arc::new(move || {
                let _ = wake.wake(state);
            }),
        ) else {
            return NativeRuntimeStatus::RUNTIME_FAILURE;
        };

        let previous = started
            .event_wait
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .replace(registration);

        assert!(
            previous.is_none(),
            "task event wait registration must be consumed before resumption"
        );

        if ready.suspend(suspension).is_err() {
            started
                .event_wait
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take();

            return NativeRuntimeStatus::RUNTIME_FAILURE;
        }

        NativeRuntimeStatus::SUCCESS
    }

    pub(in crate::native) fn compose_awaited(
        &self,
        frame: NativeProtectedFrame,
    ) -> NativeRuntimeStatus {
        let mut transfer = super::super::frame::NativeFrameTransfer::new(frame);

        let Some(parent) = current_native_task() else {
            return NativeRuntimeStatus::INVALID_ARGUMENT;
        };

        if self
            .awaited
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .contains_key(&parent)
        {
            return NativeRuntimeStatus::RUNTIME_FAILURE;
        }

        let cleanup_parent = match self.with_started(parent, |task| Arc::clone(&task.terminal)) {
            Ok(parent) => parent,
            Err(status) => return status,
        };

        let allocation = self.allocate_continuation();

        let Some(child) = allocation.task() else {
            return allocation.status();
        };

        let status = self.start(child, &mut transfer, Some(cleanup_parent));

        if !status.is_success() {
            return status;
        }

        self.awaited
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(parent, child);

        NativeRuntimeStatus::SUCCESS
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

        self.awaited
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&parent);

        self.destroy_task(child)
    }

    pub(in crate::native) fn wake(
        &self,
        handle: NativeTaskHandle,
        state: u32,
    ) -> NativeRuntimeStatus {
        self.with_started(handle, |task| {
            task.registration
                .wake_handle()
                .wake(ProtectedFrameStateId::new(state))
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
        super::super::incident::report_cleanup_incidents(&self.cleanup_reports)
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

                    Arc::clone(task)
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
                | TaskObservationError::SynchronizationPoisoned,
            ) => return runtime_failure(NativeRuntimeStatus::RUNTIME_FAILURE),
        };

        task.waits
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();

        let outcome = match outcome {
            Some(outcome) => task_outcome(outcome, &task.terminal).unwrap_or_else(|panic| {
                self.cleanup_reports.transfer_erased(
                    CleanupIncidentProducer::Task(task.task.id()),
                    CleanupIncidentOrigin::new(
                        task.task.descriptor().frame(),
                        task.task.state_id_for_reporting(),
                    ),
                    Box::new(panic),
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
                    _task: Arc::clone(&task),
                },
            );

        outcome
    }

    pub(in crate::native) fn lane(
        &self,
        handle: NativeTaskHandle,
        state: u32,
    ) -> NativeExecutionLaneResult {
        self.with_started(handle, |task| {
            task.registration
                .lane(ProtectedFrameStateId::new(state))
                .map(lane_result)
                .unwrap_or_else(|_| {
                    NativeExecutionLaneResult::failure(NativeRuntimeStatus::RUNTIME_FAILURE)
                })
        })
        .unwrap_or_else(NativeExecutionLaneResult::failure)
    }

    fn with_started<T>(
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

            Arc::clone(task)
        };

        Ok(callback(&task))
    }

    fn wait_main_thread(&self) -> NativeRuntimeStatus {
        let ready = self.drive_main_thread();

        if ready != NativeRuntimeStatus::PENDING {
            return ready;
        }

        let thread = self.thread.runtime().id();

        let lanes =
            current_thread_lanes(thread, self.main_thread_lane, self.cleanup_workloads.get());

        let deadline =
            bray_platform::MonotonicClock.deadline_after(std::time::Duration::from_millis(10));

        match self.scheduler.wait_ready_from(&lanes, deadline) {
            Ok(Some(ready)) => self.drive_ready(ready),
            Ok(None) => NativeRuntimeStatus::SUCCESS,
            Err(_) => NativeRuntimeStatus::RUNTIME_FAILURE,
        }
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

        let origin = CleanupIncidentOrigin::new(
            task.task.descriptor().frame(),
            task.task.state_id_for_reporting(),
        );

        for incident in task.terminal.take_cleanup_incidents().into_iter().rev() {
            self.cleanup_reports
                .transfer_erased(producer, origin, incident);
        }
    }

    fn awaited_child(&self, parent: NativeTaskHandle) -> Option<NativeTaskHandle> {
        self.awaited
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&parent)
            .copied()
    }

    fn register_awaited_wake(
        &self,
        parent: NativeTaskHandle,
        state: ProtectedFrameStateId,
    ) -> NativeRuntimeStatus {
        let Some(child) = self.awaited_child(parent) else {
            return NativeRuntimeStatus::SUCCESS;
        };

        let wake = self.with_started(parent, |task| task.registration.wake_handle());

        let Ok(wake) = wake else {
            return NativeRuntimeStatus::RUNTIME_FAILURE;
        };

        self.register_task_waiter(
            parent,
            child,
            Arc::new(move || {
                let _ = wake.wake(state);
            }),
        )
    }

    fn register_task_waiter(
        &self,
        owner: NativeTaskHandle,
        child: NativeTaskHandle,
        wake: Arc<dyn crate::JoinWake>,
    ) -> NativeRuntimeStatus {
        let (owner, child) = {
            let tasks = self
                .tasks
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);

            let retained = |handle| match tasks.get(&handle) {
                Some(
                    NativeTaskSlot::Started(task) | NativeTaskSlot::Terminal { _task: task, .. },
                ) => Some(Arc::clone(task)),
                _ => None,
            };

            let (Some(owner), Some(child)) = (retained(owner), retained(child)) else {
                return NativeRuntimeStatus::UNKNOWN_TASK;
            };

            (owner, child)
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
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use bray_runtime_abi::{
        NativeFrameAffinity, NativeFrameExit, NativeFrameProgress, NativeFrameProgressKind,
        NativeFrameState, NativeLaneRequirements, NativeProtectedFrame, NativeRunState,
        NativeRuntimeConfiguration, NativeRuntimeStatus, NativeTaskHandle,
    };

    use super::super::core::{initialize, shutdown, with_runtime};

    #[test]
    fn task_completion_suspension_retains_early_and_late_terminal_results() {
        extern "C" fn state(_: usize, _: u32) -> NativeFrameState {
            NativeFrameState::new(
                NativeFrameAffinity::MAIN_THREAD,
                NativeLaneRequirements::MAIN_THREAD,
            )
        }

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
                    [40; 32],
                    1,
                    0,
                    1,
                    0,
                    1,
                    state,
                    complete,
                    complete,
                    action,
                    resolve,
                    move_completion,
                    action,
                );

                let parent_frame = NativeProtectedFrame::new(
                    usize::try_from(child.raw()).unwrap(),
                    [41; 32],
                    2,
                    0,
                    1,
                    0,
                    1,
                    state,
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
                            &mut super::super::super::frame::NativeFrameTransfer::new(child_frame),
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
                            &mut super::super::super::frame::NativeFrameTransfer::new(parent_frame),
                            None
                        ),
                        NativeRuntimeStatus::SUCCESS
                    );
                } else {
                    assert_eq!(
                        runtime.start(
                            parent,
                            &mut super::super::super::frame::NativeFrameTransfer::new(parent_frame),
                            None
                        ),
                        NativeRuntimeStatus::SUCCESS
                    );

                    assert_eq!(
                        runtime.start(
                            child,
                            &mut super::super::super::frame::NativeFrameTransfer::new(child_frame),
                            None
                        ),
                        NativeRuntimeStatus::SUCCESS
                    );
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
    fn composed_cleanup_incidents_stay_with_the_parent_until_its_boundary_resolves() {
        extern "C" fn state(_: usize, _: u32) -> NativeFrameState {
            NativeFrameState::new(
                NativeFrameAffinity::MAIN_THREAD,
                NativeLaneRequirements::MAIN_THREAD,
            )
        }

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
                let parent = Arc::new(crate::native::frame::NativeTerminalState::new());
                let child = runtime.allocate().task().unwrap();

                let frame = NativeProtectedFrame::new(
                    0,
                    [42; 32],
                    1,
                    0,
                    1,
                    0,
                    1,
                    state,
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
                        &mut super::super::super::frame::NativeFrameTransfer::new(frame),
                        composed.then(|| Arc::clone(&parent))
                    ),
                    NativeRuntimeStatus::SUCCESS
                );

                runtime
                    .with_started(child, |task| {
                        for ordinal in [7u32, 11] {
                            task.terminal.record_cleanup_incident(Box::new(ordinal));
                        }
                    })
                    .unwrap();

                let outcome = runtime.resolve_task(child);

                assert_eq!(outcome.state(), NativeRunState::COMPLETED);
                assert_eq!(runtime.observe(child), outcome);

                let incidents: Vec<_> = parent
                    .take_cleanup_incidents()
                    .into_iter()
                    .map(|incident| *incident.downcast::<u32>().unwrap())
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

    #[test]
    fn waiter_registration_survives_terminal_publication_after_a_pending_observation() {
        extern "C" fn state(_: usize, _: u32) -> NativeFrameState {
            NativeFrameState::new(
                NativeFrameAffinity::MAIN_THREAD,
                NativeLaneRequirements::MAIN_THREAD,
            )
        }

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
                [39; 32],
                1,
                0,
                1,
                0,
                1,
                state,
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
                    &mut super::super::super::frame::NativeFrameTransfer::new(frame),
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
