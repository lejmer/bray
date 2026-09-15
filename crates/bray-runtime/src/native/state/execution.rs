use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use bray_runtime_abi::{
    NativeExecutionLaneResult, NativeInactiveFrame, NativeProtectedFrame, NativeRootHandle,
    NativeRunOutcome, NativeRunState, NativeRuntimeStatus, NativeTaskAllocation, NativeTaskHandle,
    NativeWakeCallback,
};
use bray_runtime_model::ProtectedFrameStateId;

use crate::context::with_task_execution_context;
use crate::{
    CleanupIncidentOrigin, ExecutionLanePlacement, FrameSuspensionKind, JoinWaitRegistration,
    RootCancellationHandle, TaskControlBlock, TaskExecutionContext, TaskObservationError,
    TaskResumeStatus,
};

use super::super::frame::NativeFrame;
use super::binding::{
    CleanupWorkloadScope, current_native_task, current_thread_lanes, lane_result, runtime_failure,
    task_outcome, write_cleanup_incident_report,
};
use super::core::{CURRENT_NATIVE_TASK, NativeRuntime, NativeTaskSlot, StartedTask};

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
        let Ok(admission) = crate::TaskAdmission::new() else {
            return NativeTaskAllocation::failure(NativeRuntimeStatus::RUNTIME_FAILURE);
        };

        self.allocate_admitted(admission)
    }

    pub(in crate::native) fn allocate_admitted(
        &self,
        admission: crate::TaskAdmission,
    ) -> NativeTaskAllocation {
        let mut tasks = self
            .tasks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        if tasks.len() >= self.task_capacity.get() {
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

        tasks.insert(handle, NativeTaskSlot::Allocated(admission));

        NativeTaskAllocation::success(handle)
    }

    pub(in crate::native) fn start(
        &self,
        handle: NativeTaskHandle,
        frame: NativeProtectedFrame,
    ) -> NativeRuntimeStatus {
        let Some(mut frame) = NativeFrame::try_new(frame) else {
            return NativeRuntimeStatus::INVALID_ARGUMENT;
        };

        let mut tasks = self
            .tasks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        if !matches!(tasks.get(&handle), Some(NativeTaskSlot::Allocated(_))) {
            return NativeRuntimeStatus::UNKNOWN_TASK;
        }

        let Some(NativeTaskSlot::Allocated(mut admission)) = tasks.remove(&handle) else {
            unreachable!("allocated task was checked under the same lock");
        };

        frame.outgoing = admission.outgoing.take_one();
        frame.outgoing.append(&mut admission.outgoing.take_one());
        let terminal = frame.terminal_state();

        let task = TaskControlBlock::start(admission, frame);
        let state = ProtectedFrameStateId::new(0);

        let Ok(registration) = self.scheduler.register_task(
            task.id(),
            task.descriptor().clone(),
            self.thread.runtime().id(),
            state,
            task.cancellation_context(),
        ) else {
            return NativeRuntimeStatus::RUNTIME_FAILURE;
        };

        let cleanup_lane = registration.lane(state);

        if self.cleanup_workloads.get()
            && !self.main_thread_lane
            && cleanup_lane
                .is_ok_and(|lane| matches!(lane.placement(), ExecutionLanePlacement::MainThread(_)))
        {
            return NativeRuntimeStatus::RUNTIME_FAILURE;
        }

        let task = Arc::new(StartedTask {
            task,
            registration,
            waits: Mutex::new(Vec::new()),
            event_wait: Mutex::new(None),
            observation_claimed: AtomicBool::new(false),
            terminal,
        });

        let wake = task.registration.wake_handle();

        tasks.insert(handle, NativeTaskSlot::Started(task));
        drop(tasks);

        if wake.wake(state).is_err() {
            self.tasks
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .remove(&handle);

            return NativeRuntimeStatus::RUNTIME_FAILURE;
        }

        NativeRuntimeStatus::SUCCESS
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

        CURRENT_NATIVE_TASK.set(Some(handle));

        let status = with_task_execution_context(context, || task.resume());

        CURRENT_NATIVE_TASK.set(None);

        let Ok(status) = status else {
            return NativeRuntimeStatus::RUNTIME_FAILURE;
        };

        match status {
            TaskResumeStatus::Suspended(suspension) => {
                let state = suspension.state();
                let kind = suspension.kind();

                if kind == FrameSuspensionKind::TaskEvent {
                    return self.suspend_on_task_event(ready, &started, suspension, wake);
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
                    FrameSuspensionKind::TaskEvent => {
                        unreachable!("task-event suspension must publish its registration first")
                    }
                }
            }
            TaskResumeStatus::Terminal(_) => NativeRuntimeStatus::SUCCESS,
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
        frame: NativeInactiveFrame,
    ) -> NativeRuntimeStatus {
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

        let allocation = self.allocate();

        let Some(child) = allocation.task() else {
            return allocation.status();
        };

        let status = self.start(child, frame.into_protected());

        if !status.is_success() {
            return status;
        }

        self.awaited
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(parent, child);

        NativeRuntimeStatus::SUCCESS
    }

    pub(in crate::native) fn resolve_awaited_completion(
        &self,
    ) -> Result<usize, NativeRuntimeStatus> {
        let parent = current_native_task().ok_or(NativeRuntimeStatus::INVALID_ARGUMENT)?;

        let child = self
            .awaited
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&parent)
            .ok_or(NativeRuntimeStatus::UNKNOWN_TASK)?;

        let outcome = self.observe(child);

        if outcome.state() != NativeRunState::COMPLETED || outcome.payload() == 0 {
            self.awaited
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .insert(parent, child);

            return Err(NativeRuntimeStatus::RUNTIME_FAILURE);
        }

        self.resolved_awaits
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entry(parent)
            .or_default()
            .push(child);

        Ok(outcome.payload())
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
            Some(NativeTaskSlot::Terminal { .. }) => {
                tasks.remove(&task);

                NativeRuntimeStatus::SUCCESS
            }
            Some(NativeTaskSlot::Started(_)) | Some(NativeTaskSlot::Allocated(_)) => {
                NativeRuntimeStatus::PENDING
            }
            None => NativeRuntimeStatus::UNKNOWN_TASK,
        }
    }

    pub(in crate::native) fn report_cleanup_incidents(&self) -> NativeRuntimeStatus {
        let mut status = NativeRuntimeStatus::SUCCESS;

        self.cleanup_reports.drain(|mut incident| {
            if write_cleanup_incident_report(&mut std::io::stderr().lock(), &incident).is_err()
                && status.is_success()
            {
                status = NativeRuntimeStatus::RUNTIME_FAILURE;
            }

            let disposal = incident.dispose();

            if status.is_success() {
                status = disposal;
            }
        });

        status
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

        let registration = self.with_started(handle, |task| {
            task.task
                .register_join_waiter(Arc::new(move || callback(context)))
        });

        let Ok(Ok(registration)) = registration else {
            return runtime_failure(NativeRuntimeStatus::RUNTIME_FAILURE);
        };

        let status = self.retain_wait(owner, registration);

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
                Some(NativeTaskSlot::Terminal { state, payload, .. }) => {
                    return if *state == NativeRunState::PANICKED {
                        runtime_failure(NativeRuntimeStatus::UNKNOWN_TASK)
                    } else {
                        NativeRunOutcome::new(*state, *payload)
                    };
                }
                Some(NativeTaskSlot::Allocated(_)) | None => {
                    return runtime_failure(NativeRuntimeStatus::UNKNOWN_TASK);
                }
            }
        };

        let _claim = TaskObservationClaim(&task.observation_claimed);

        match task.task.take_outcome() {
            Ok(outcome) => {
                task.waits
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .clear();

                self.transfer_cleanup_incidents(&task);
                let outcome = task_outcome(outcome, &task.task);

                self.tasks
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .insert(
                        handle,
                        NativeTaskSlot::Terminal {
                            state: outcome.state(),
                            payload: outcome.payload(),
                            _task: Arc::clone(&task),
                        },
                    );

                self.release_resolved_awaits(handle);

                outcome
            }
            Err(TaskObservationError::Pending) => NativeRunOutcome::new(NativeRunState::PENDING, 0),
            Err(TaskObservationError::RuntimeFailed(_)) => {
                task.waits
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .clear();

                self.transfer_cleanup_incidents(&task);
                let outcome = runtime_failure(NativeRuntimeStatus::RUNTIME_FAILURE);

                self.tasks
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .insert(
                        handle,
                        NativeTaskSlot::Terminal {
                            state: outcome.state(),
                            payload: outcome.payload(),
                            _task: Arc::clone(&task),
                        },
                    );

                outcome
            }
            Err(
                TaskObservationError::AlreadyObserved
                | TaskObservationError::WaiterIdentityExhausted
                | TaskObservationError::SynchronizationPoisoned,
            ) => runtime_failure(NativeRuntimeStatus::RUNTIME_FAILURE),
        }
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
        let origin = CleanupIncidentOrigin::new(
            task.task.descriptor().frame(),
            task.task.state_id_for_reporting(),
        );

        for incident in task.terminal.take_cleanup_incidents().into_iter().flatten() {
            task.task
                .transfer_cleanup_incident(&self.cleanup_reports, origin, incident);
        }
    }

    fn register_awaited_wake(
        &self,
        parent: NativeTaskHandle,
        state: ProtectedFrameStateId,
    ) -> NativeRuntimeStatus {
        let Some(child) = self
            .awaited
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&parent)
            .copied()
        else {
            return NativeRuntimeStatus::SUCCESS;
        };

        let wake = self.with_started(parent, |task| task.registration.wake_handle());

        let Ok(wake) = wake else {
            return NativeRuntimeStatus::RUNTIME_FAILURE;
        };

        let registration = self.with_started(child, |task| {
            task.task.register_join_waiter(Arc::new(move || {
                let _ = wake.wake(state);
            }))
        });

        let Ok(Ok(registration)) = registration else {
            return NativeRuntimeStatus::RUNTIME_FAILURE;
        };

        self.retain_wait(parent, registration)
    }

    fn retain_wait(
        &self,
        owner: NativeTaskHandle,
        registration: JoinWaitRegistration<usize>,
    ) -> NativeRuntimeStatus {
        self.with_started(owner, |task| {
            task.waits
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(registration);

            NativeRuntimeStatus::SUCCESS
        })
        .unwrap_or_else(|status| status)
    }

    fn release_resolved_awaits(&self, parent: NativeTaskHandle) {
        let Some(children) = self
            .resolved_awaits
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&parent)
        else {
            return;
        };

        let mut tasks = self
            .tasks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        for child in children {
            tasks.remove(&child);
        }
    }
}
