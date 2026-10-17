use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use bray_runtime_abi::{
    NativeExecutionLaneResult, NativeInactiveFrame, NativeRootHandle, NativeRunOutcome,
    NativeRunState, NativeRuntimeStatus, NativeTaskAllocation, NativeTaskHandle,
};
use bray_runtime_model::ProtectedFrameStateId;

use crate::context::with_task_execution_context;
use crate::{
    ExecutionLanePlacement, FrameSuspensionKind, JoinWaitRegistration, RootCancellationHandle,
    TaskControlBlock, TaskExecutionContext, TaskObservationError, TaskResumeStatus,
};

use super::binding::{
    CleanupWorkloadScope, current_native_task, current_thread_lanes, lane_result, runtime_failure,
    task_outcome,
};
use super::core::{
    NativeRuntime, NativeTaskSlot, Retirement, StartedTask, SuspendedWait, with_current_task,
};

struct TaskObservationClaim<'a>(&'a AtomicBool);

impl Drop for TaskObservationClaim<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

fn suspended_wait_ready(task: &StartedTask) -> bool {
    task.suspended_wait
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .as_ref()
        .is_none_or(SuspendedWait::is_ready)
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

    pub(in crate::native) fn compose_awaited(
        &self,
        frame: &NativeInactiveFrame,
        resume_state: u32,
        destination: usize,
        abandoning: bool,
    ) -> NativeRuntimeStatus {
        let Some(parent) = current_native_task() else {
            return NativeRuntimeStatus::INVALID_ARGUMENT;
        };

        self.with_started(parent, |task| {
            task.run
                .compose(frame, resume_state, destination, abandoning)
        })
        .unwrap_or_else(|status| status)
    }

    pub(in crate::native) fn start(
        &self,
        handle: NativeTaskHandle,
        frame: &NativeInactiveFrame,
    ) -> NativeRuntimeStatus {
        let origin = self.thread.runtime().id();

        let initial = match super::super::run::NativeRun::inspect(frame, origin.raw()) {
            Ok(initial) => initial,
            Err(status) => return status,
        };

        let run = super::super::run::NativeRun::new(&initial);
        let cancellation = crate::CancellationContext::root();

        let mut tasks = self
            .tasks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let Some(NativeTaskSlot::Allocated(admission)) = tasks.get(&handle) else {
            return NativeRuntimeStatus::UNKNOWN_TASK;
        };

        let Ok(registration) = self.scheduler.register_task(
            admission.id,
            super::super::activation::execution(&initial),
            origin,
            &cancellation,
        ) else {
            return NativeRuntimeStatus::RUNTIME_FAILURE;
        };

        if self.cleanup_workloads.get()
            && !self.main_thread_lane
            && registration
                .lane()
                .is_ok_and(|lane| matches!(lane.placement(), ExecutionLanePlacement::MainThread(_)))
        {
            return NativeRuntimeStatus::RUNTIME_FAILURE;
        }

        // Admission and registration precede ownership transfer. Rejection leaves
        // the caller's exact inactive token and the task admission intact.
        let status = run.activate(frame, origin.raw());

        if !status.is_success() {
            return status;
        }

        let Some(NativeTaskSlot::Allocated(admission)) = tasks.remove(&handle) else {
            unreachable!("the admitted task remains allocated under this lock");
        };

        let task = TaskControlBlock::start_frame(
            admission,
            crate::erase_sendable_protected_frame(Arc::clone(&run)),
            cancellation,
        );

        let task = Arc::new(StartedTask {
            task,
            run,
            registration,
            waits: Mutex::new(Vec::new()),
            suspended_wait: Mutex::new(None),
            observation_claimed: AtomicBool::new(false),
            retirement: Mutex::new(Retirement::Retained),
        });

        let wake = task.registration.wake_handle();

        tasks.insert(handle, NativeTaskSlot::Started(task));
        drop(tasks);

        wake.wake()
            .expect("fresh task registration accepts its initial wake");

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
                let task = match slot {
                    NativeTaskSlot::Started(task)
                    | NativeTaskSlot::Terminal { _task: task, .. } => task,
                    NativeTaskSlot::Allocated(_) => return None,
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
            // A ready notification can race terminal removal after dispatch was
            // acquired. It has no frame authority and needs no further work.
            return NativeRuntimeStatus::SUCCESS;
        };

        if started.run.is_finished() {
            return self.drive_retirement(ready, handle, &started);
        }

        let task = &started.task;
        let wake = started.registration.wake_handle();

        if !task.cancellation_observable() && !suspended_wait_ready(&started) {
            let execution = ready.execution_state().clone();

            return ready
                .suspend(execution)
                .map_or(NativeRuntimeStatus::RUNTIME_FAILURE, |_| {
                    NativeRuntimeStatus::SUCCESS
                });
        }

        started
            .suspended_wait
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

        let _bridge = match super::super::bridge::Scope::enter() {
            Ok(scope) => scope,
            Err(status) => return status,
        };

        let status = with_current_task(handle, || {
            with_task_execution_context(context, || task.resume())
        });

        let Ok(status) = status else {
            return NativeRuntimeStatus::RUNTIME_FAILURE;
        };

        match status {
            TaskResumeStatus::Suspended(suspension, execution) => {
                let kind = suspension.kind();

                if kind == FrameSuspensionKind::TaskEvent {
                    return self
                        .suspend_on_task_event(ready, &started, suspension, execution, wake);
                }

                if ready.suspend(execution).is_err() {
                    started
                        .suspended_wait
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .take();

                    return NativeRuntimeStatus::RUNTIME_FAILURE;
                }

                match kind {
                    FrameSuspensionKind::Awaited => NativeRuntimeStatus::SUCCESS,
                    FrameSuspensionKind::Yield => wake
                        .wake()
                        .map_or(NativeRuntimeStatus::RUNTIME_FAILURE, |_| {
                            NativeRuntimeStatus::SUCCESS
                        }),
                    FrameSuspensionKind::TaskEvent => {
                        unreachable!("task-event suspension must publish its registration first")
                    }
                }
            }
            TaskResumeStatus::Terminal(_) => {
                ready
                    .suspend(started.run.snapshot())
                    .expect("terminal dispatch retains its admitted retirement lane");

                NativeRuntimeStatus::SUCCESS
            }
        }
    }

    fn drive_retirement(
        &self,
        ready: crate::ReadyTask,
        handle: NativeTaskHandle,
        task: &StartedTask,
    ) -> NativeRuntimeStatus {
        let requested = matches!(
            *task
                .retirement
                .lock()
                .expect("retirement request is available"),
            Retirement::Requested(_)
        );

        if !requested {
            ready
                .suspend(task.run.snapshot())
                .expect("terminal task retains its retirement lane");

            return NativeRuntimeStatus::SUCCESS;
        }

        let _bridge = match super::super::bridge::Scope::enter() {
            Ok(scope) => scope,
            Err(status) => return status,
        };

        let status = with_current_task(handle, || task.run.retire(task, ready.lane()));

        if status == NativeRuntimeStatus::PENDING {
            ready
                .suspend(task.run.snapshot())
                .expect("deferred retirement retains its admitted execution metadata");

            task.registration
                .wake_handle()
                .wake()
                .expect("deferred retirement retains its task registration");

            return NativeRuntimeStatus::SUCCESS;
        }

        assert!(
            status.is_success(),
            "admitted retirement must complete or defer"
        );

        let previous = std::mem::replace(
            &mut *task
                .retirement
                .lock()
                .expect("retirement request is available"),
            Retirement::Released,
        );

        let Retirement::Requested(waiter) = previous else {
            panic!("exclusive dispatch completes the requested retirement once");
        };

        ready
            .suspend(task.run.snapshot())
            .expect("retirement retains its admitted execution metadata");

        if let Some(waiter) = waiter {
            let _ = waiter.wake();
        }

        NativeRuntimeStatus::SUCCESS
    }

    fn suspend_on_task_event(
        &self,
        ready: crate::ReadyTask,
        started: &StartedTask,
        suspension: crate::FrameSuspension,
        execution: crate::FrameExecutionState,
        wake: crate::TaskWakeHandle,
    ) -> NativeRuntimeStatus {
        let Some(identity) = suspension.payload() else {
            return NativeRuntimeStatus::RUNTIME_FAILURE;
        };

        let Some(event) = super::super::event::event(&self.core, identity) else {
            return NativeRuntimeStatus::RUNTIME_FAILURE;
        };

        let (generation, _) = event.observation();

        // Keep the dispatch running until registration ownership is published. An immediate
        // event wake then becomes a pending scheduler wake that suspension releases.
        let Ok(registration) = event.register(
            generation,
            Arc::new(move || {
                let _ = wake.wake();
            }),
        ) else {
            return NativeRuntimeStatus::RUNTIME_FAILURE;
        };

        let previous = started
            .suspended_wait
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .replace(SuspendedWait::Event {
                event,
                observed: generation,
                _registration: registration,
            });

        assert!(
            previous.is_none(),
            "task event wait registration must be consumed before resumption"
        );

        if ready.suspend(execution).is_err() {
            started
                .suspended_wait
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take();

            return NativeRuntimeStatus::RUNTIME_FAILURE;
        }

        NativeRuntimeStatus::SUCCESS
    }

    pub(in crate::native) fn wake(&self, handle: NativeTaskHandle) -> NativeRuntimeStatus {
        self.with_started(handle, |task| {
            task.registration
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

        let status = self.resolve_task_retirement(task);

        if !status.is_success() {
            return status;
        }

        self.destroy_task(task)
    }

    pub(in crate::native) fn resolve_task_retirement(
        &self,
        task: NativeTaskHandle,
    ) -> NativeRuntimeStatus {
        loop {
            let status = self.retire_task(task);

            if status != NativeRuntimeStatus::PENDING {
                return status;
            }

            let status = self.wait_main_thread();

            if !status.is_success() {
                return status;
            }
        }
    }

    pub(in crate::native) fn retire_task(&self, task: NativeTaskHandle) -> NativeRuntimeStatus {
        let retained = {
            let tasks = self
                .tasks
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);

            match tasks.get(&task) {
                Some(NativeTaskSlot::Terminal { _task, .. }) => Arc::clone(_task),
                Some(_) => return NativeRuntimeStatus::PENDING,
                None => return NativeRuntimeStatus::UNKNOWN_TASK,
            }
        };

        let waiter = current_native_task().map(|owner| {
            self.with_started(owner, |task| task.registration.wake_handle())
                .expect("retirement observer retains its task registration")
        });

        let mut retirement = retained
            .retirement
            .lock()
            .expect("retirement request is available");

        if matches!(*retirement, Retirement::Released) {
            return NativeRuntimeStatus::SUCCESS;
        }

        *retirement = Retirement::Requested(waiter);
        drop(retirement);

        retained
            .registration
            .wake_handle()
            .wake()
            .expect("retirement retains its task registration");

        NativeRuntimeStatus::PENDING
    }

    pub(in crate::native) fn destroy_task(&self, task: NativeTaskHandle) -> NativeRuntimeStatus {
        let removed = {
            let mut tasks = self
                .tasks
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);

            match tasks.get(&task) {
                Some(slot) if slot.can_destroy() => {}
                Some(_) => return NativeRuntimeStatus::PENDING,
                None => return NativeRuntimeStatus::UNKNOWN_TASK,
            }

            tasks.remove(&task)
        };

        drop(removed);

        NativeRuntimeStatus::SUCCESS
    }

    pub(in crate::native) fn report_cleanup_incidents(&self) -> NativeRuntimeStatus {
        let mut status = NativeRuntimeStatus::SUCCESS;

        self.cleanup_reports.drain(|mut incident| {
            let disposal = incident.report();

            if status.is_success() {
                status = disposal;
            }
        });

        status
    }

    #[cfg(test)]
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
        waiter: Arc<dyn crate::JoinWake>,
    ) -> NativeRunOutcome {
        let outcome = self.observe(handle);

        if outcome.state() != NativeRunState::PENDING {
            return outcome;
        }

        let owner = current_native_task().unwrap_or(handle);

        let registration = self.with_started(handle, |task| task.task.register_join_waiter(waiter));

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

                let outcome = task_outcome(outcome);

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
            Err(TaskObservationError::Pending) => NativeRunOutcome::new(NativeRunState::PENDING, 0),
            Err(TaskObservationError::RuntimeFailed(_)) => {
                task.waits
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .clear();

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
            task.run
                .execution_at(ProtectedFrameStateId::new(state))
                .and_then(|execution| {
                    task.registration
                        .execution_lane(&execution)
                        .map_err(|_| NativeRuntimeStatus::RUNTIME_FAILURE)
                })
                .map(lane_result)
                .unwrap_or_else(|_| {
                    NativeExecutionLaneResult::failure(NativeRuntimeStatus::RUNTIME_FAILURE)
                })
        })
        .unwrap_or_else(NativeExecutionLaneResult::failure)
    }

    pub(in crate::native) fn with_started<T>(
        &self,
        handle: NativeTaskHandle,
        callback: impl FnOnce(&StartedTask) -> T,
    ) -> Result<T, NativeRuntimeStatus> {
        self.retained_task(handle, false)
            .map(|task| callback(&task))
    }

    pub(in crate::native) fn retained_task(
        &self,
        handle: NativeTaskHandle,
        terminal: bool,
    ) -> Result<Arc<StartedTask>, NativeRuntimeStatus> {
        let tasks = self
            .tasks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        match tasks.get(&handle) {
            Some(NativeTaskSlot::Started(task)) => Ok(Arc::clone(task)),
            Some(NativeTaskSlot::Terminal { _task, .. }) if terminal => Ok(Arc::clone(_task)),
            _ => Err(NativeRuntimeStatus::UNKNOWN_TASK),
        }
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
}
