use std::sync::Arc;

use bray_runtime_abi::{
    NativeActivationExecution, NativeExecutionLaneResult, NativeFrameProgressKind,
    NativeInactiveFrame, NativeRootHandle, NativeRunOutcome, NativeRunState,
    NativeRuntimeStatus, NativeTaskAllocation, NativeTaskHandle, NativeTaskReference,
    NativeTaskSnapshot,
};
use bray_runtime_model::ProtectedFrameStateId;

use crate::context::with_task_execution_context;
use crate::{ExecutionLanePlacement, RootCancellationHandle, TaskExecutionContext, TaskId, report_provider};
use super::super::NativeTaskServices;
use super::super::task::SuspendedWait;
use super::binding::{CleanupWorkloadScope, current_thread_lanes, lane_result, runtime_failure};
use super::core::{NativeRuntime, current_services, with_task_services};

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
        let handle = report_provider::bray_runtime_task_control_admission(self.identity);

        NativeTaskHandle::new(handle).map_or_else(
            || NativeTaskAllocation::failure(NativeRuntimeStatus::RUNTIME_FAILURE),
            NativeTaskAllocation::success,
        )
    }

    pub(in crate::native) fn start(&self, handle: NativeTaskHandle, frame: &NativeInactiveFrame) -> NativeRuntimeStatus {
        let origin = self.thread.runtime().id();
        let mut initial = NativeActivationExecution::default();
        let status = report_provider::bray_runtime_activation_inspection(frame, 0, 0, 0, &mut initial);

        if status != 0 {
            return NativeRuntimeStatus::INVALID_ARGUMENT;
        }

        initial.origin = origin.raw();

        let mut reference = NativeTaskReference::empty();
        let status = report_provider::bray_runtime_task_control_retention(self.identity, handle.raw(), &mut reference);

        if !status.is_success() {
            return status;
        }

        let mut snapshot = NativeTaskSnapshot::default();
        let status = report_provider::bray_runtime_task_control_snapshot(&reference, &mut snapshot);

        assert!(status.is_success(), "the acquired control remains live");

        if snapshot.phase != 0 {
            report_provider::bray_runtime_task_control_reference_release(&reference);

            return NativeRuntimeStatus::INVALID_ARGUMENT;
        }

        let cancellation = crate::CancellationContext::root();
        let identity = TaskId::from_bray_handle(handle);

        let registration = match self.scheduler.register_task(identity, super::super::activation::execution(&initial), origin, &cancellation) {
            Ok(registration) => registration,
            Err(_) => {
                report_provider::bray_runtime_task_control_reference_release(&reference);

                return NativeRuntimeStatus::RUNTIME_FAILURE;
            }
        };

        if self.cleanup_workloads.get() && !self.main_thread_lane && registration.lane().is_ok_and(|lane| matches!(lane.placement(), ExecutionLanePlacement::MainThread(_))) {
            report_provider::bray_runtime_task_control_reference_release(&reference);

            return NativeRuntimeStatus::RUNTIME_FAILURE;
        }

        let task = Arc::new(NativeTaskServices::new(reference, cancellation, registration));

        self.scheduler.install_native_task(Arc::clone(&task)).expect("a new registration accepts its admitted native resources");

        let root = report_provider::bray_runtime_activation_entry(frame, origin.raw());

        if root == 0 {
            self.scheduler.unregister_native_task(identity);

            return NativeRuntimeStatus::INVALID_ARGUMENT;
        }

        let status = report_provider::bray_runtime_task_control_publication(&task.control, root, &initial);

        assert!(status.is_success(), "activation publishes into its previously admitted control");
        task.registration.wake_handle().wake().expect("the newly published task accepts its first wake");

        NativeRuntimeStatus::SUCCESS
    }

    pub(in crate::native) fn compose_awaited(&self, frame: &NativeInactiveFrame, resume_state: u32, destination: usize, composition: bray_runtime_abi::NativeFrameComposition) -> NativeRuntimeStatus {
        let Some(parent) = current_services() else {
            return NativeRuntimeStatus::INVALID_ARGUMENT;
        };

        report_provider::bray_runtime_task_control_composition(
            &parent.control, frame, resume_state, self.thread.runtime().id().raw(), destination,
            composition.code(), super::super::activation::admission,
        )
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

    pub(in crate::native) fn drive_ready(&self, mut ready: crate::ReadyTask) -> NativeRuntimeStatus {
        let task = ready.take_native_task().expect("native dispatch retains the registered native resources");
        let wake = task.registration.wake_handle();

        let (pending, completed_wait) = {
            let mut wait = task.suspended_wait.lock().expect("task wait transport is available");

            if wait.as_ref().is_some_and(|wait| !wait.is_ready())
                && !task.cancellation.is_requested()
                && task.snapshot().phase < 4
            {
                (true, None)
            } else {
                (false, wait.take())
            }
        };

        drop(completed_wait);

        if pending {
            return ready.suspend(task.execution_state()).map_or(NativeRuntimeStatus::RUNTIME_FAILURE, |_| NativeRuntimeStatus::SUCCESS);
        }

        let context = TaskExecutionContext::new(task.id(), ready.state(), task.cancellation.clone(), &task.output, ready.lane(), wake.clone());

        let _bridge = match super::super::bridge::Scope::enter() {
            Ok(scope) => scope,
            Err(status) => return status,
        };

        let (status, progress, execution) = with_task_services(Arc::clone(&task), || {
            with_task_execution_context(context, || task.dispatch())
        });

        if status == 6 {
            return ready.suspend(task.execution_state()).map_or(NativeRuntimeStatus::RUNTIME_FAILURE, |_| NativeRuntimeStatus::SUCCESS);
        }

        assert!(status <= 3, "Bray dispatch returns one admitted scheduling outcome");

        let execution = super::super::activation::execution(&execution);

        if status == 0 && progress.kind() == NativeFrameProgressKind::TASK_EVENT {
            return self.suspend_on_task_event(ready, &task, crate::FrameSuspension::task_event(ProtectedFrameStateId::new(progress.state()), progress.payload()), execution, wake);
        }

        if ready.suspend(execution).is_err() {
            return NativeRuntimeStatus::RUNTIME_FAILURE;
        }

        if status == 0 && progress.kind() == NativeFrameProgressKind::YIELDED {
            wake.wake().expect("a yielded task retains its admitted registration");
        }

        if status == 1 && progress.kind() == NativeFrameProgressKind::RUNTIME_FAILURE {
            NativeRuntimeStatus::RUNTIME_FAILURE
        } else {
            NativeRuntimeStatus::SUCCESS
        }
    }

    fn suspend_on_task_event(
        &self,
        ready: crate::ReadyTask,
        started: &NativeTaskServices,
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
        self.retained_task(handle).map_or_else(|status| status, |task| {
            task.registration.wake_handle().wake().map_or(NativeRuntimeStatus::RUNTIME_FAILURE, |_| NativeRuntimeStatus::SUCCESS)
        })
    }

    pub(in crate::native) fn request_cancellation(&self, handle: NativeTaskHandle) -> NativeRuntimeStatus {
        self.retained_task(handle).map_or_else(|status| status, |task| {
            task.cancellation.request();

            NativeRuntimeStatus::SUCCESS
        })
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

    pub(in crate::native) fn root_cancellation(&self, root: NativeRootHandle) -> Result<RootCancellationHandle, NativeRuntimeStatus> {
        let task = NativeTaskHandle::new(root.raw()).ok_or(NativeRuntimeStatus::UNKNOWN_TASK)?;

        self.retained_task(task).map(|task| RootCancellationHandle::new(task.cancellation.clone()))
    }

    pub(in crate::native) fn observe_root(&self, root: NativeRootHandle) -> NativeRunOutcome {
        let Some(task) = NativeTaskHandle::new(root.raw()) else {
            return runtime_failure(NativeRuntimeStatus::UNKNOWN_TASK);
        };

        self.resolve_task(task)
    }

    pub(in crate::native) fn resolve_task(&self, task: NativeTaskHandle) -> NativeRunOutcome {
        let _parked = super::core::park_current_execution();

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
        let _parked = super::core::park_current_execution();

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

    pub(in crate::native) fn retire_task(&self, handle: NativeTaskHandle) -> NativeRuntimeStatus {
        let task = match self.retained_task(handle) {
            Ok(task) => task,
            Err(status) => return status,
        };

        let status = if let Some(parent) = current_services() {
            report_provider::bray_runtime_task_control_wait(&task.control, &parent.control, 1)
        } else {
            report_provider::bray_runtime_task_control_retirement(&task.control)
        };

        if status == NativeRuntimeStatus::PENDING {
            task.registration.wake_handle().wake().expect("retirement retains its dispatch registration");
        }

        status
    }

    pub(in crate::native) fn destroy_task(&self, handle: NativeTaskHandle) -> NativeRuntimeStatus {
        if let Ok(task) = self.retained_task(handle) {
            let status = task.resolve_source_owner();

            if status.is_success() {
                self.scheduler.unregister_native_task(task.id());
            }

            return status;
        }

        let mut reference = NativeTaskReference::empty();
        let status = report_provider::bray_runtime_task_control_retention(self.identity, handle.raw(), &mut reference);

        if !status.is_success() {
            return status;
        }

        let status = report_provider::bray_runtime_task_control_destruction(&reference);
        let released = report_provider::bray_runtime_task_control_reference_release(&reference);

        assert!(released.is_success(), "rejected start releases its temporary internal reference");

        status
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

    pub(in crate::native) fn observe(&self, handle: NativeTaskHandle) -> NativeRunOutcome {
        self.retained_task(handle).and_then(|task| task.observe()).unwrap_or_else(|status| {
            if status == NativeRuntimeStatus::PENDING {
                NativeRunOutcome::new(NativeRunState::PENDING, 0)
            } else {
                runtime_failure(status)
            }
        })
    }

    pub(in crate::native) fn lane(&self, handle: NativeTaskHandle, state: u32) -> NativeExecutionLaneResult {
        self.retained_task(handle).and_then(|task| {
            let execution = task.execution_state();

            if execution.state() != ProtectedFrameStateId::new(state) {
                return Err(NativeRuntimeStatus::INVALID_ARGUMENT);
            }

            task.registration.execution_lane(&execution).map_err(|_| NativeRuntimeStatus::RUNTIME_FAILURE)
        }).map(lane_result).unwrap_or_else(NativeExecutionLaneResult::failure)
    }

    pub(in crate::native) fn retained_task(&self, handle: NativeTaskHandle) -> Result<Arc<NativeTaskServices>, NativeRuntimeStatus> {
        if let Some(task) = current_services().filter(|task| task.handle() == handle) {
            return Ok(task);
        }

        self.scheduler.native_task(TaskId::from_bray_handle(handle)).map_err(|_| NativeRuntimeStatus::UNKNOWN_TASK)
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

}
