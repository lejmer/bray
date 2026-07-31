use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::io::Write;
use std::num::NonZeroUsize;
use std::rc::Rc;
use std::sync::Arc;

use bray_platform::RuntimeThreadScope;
use bray_runtime_interface::{
    NativeExecutionLane, NativeExecutionLaneResult, NativeInactiveFrame, NativeProtectedFrame,
    NativeRootHandle, NativeRunOutcome, NativeRunState, NativeRuntimeConfiguration,
    NativeRuntimeStatus, NativeTaskAllocation, NativeTaskHandle, NativeWakeCallback,
    ProtectedFrameStateId, RuntimeCapability,
};

use crate::context::with_task_execution_context;
use crate::{
    CleanupIncidentOrigin, CleanupIncidentProducer, CleanupReportSink, ExecutionLane,
    ExecutionLanePlacement, ExecutionWorkload, RunOutcome, Scheduler, SchedulerLimits,
    TaskControlBlock, TaskExecutionContext, TaskObservationError, TaskRegistration,
    TaskResumeStatus,
};

use super::frame::{NativeFrame, NativeTerminalPayload, NativeTerminalState};

thread_local! {
    static NATIVE_RUNTIME: RefCell<Option<Rc<NativeRuntime>>> =
        const { RefCell::new(None) };
    static CURRENT_NATIVE_TASK: Cell<Option<NativeTaskHandle>> =
        const { Cell::new(None) };
}

type NativeTask = TaskControlBlock<usize>;

pub(super) struct NativeRuntime {
    thread: RuntimeThreadScope,
    scheduler: Scheduler,
    tasks: RefCell<BTreeMap<NativeTaskHandle, NativeTaskSlot>>,
    awaited: RefCell<BTreeMap<NativeTaskHandle, NativeTaskHandle>>,
    resolved_awaits: RefCell<BTreeMap<NativeTaskHandle, Vec<NativeTaskHandle>>>,
    task_capacity: NonZeroUsize,
    next_task: Cell<u64>,
    cleanup_reports: CleanupReportSink,
}

enum NativeTaskSlot {
    Allocated,
    Started(StartedTask),
    Terminal {
        outcome: NativeRunOutcome,
        task: StartedTask,
    },
}

struct StartedTask {
    task: Arc<NativeTask>,
    registration: TaskRegistration,
    terminal: Arc<NativeTerminalState>,
}

pub(super) fn initialize(configuration: NativeRuntimeConfiguration) -> NativeRuntimeStatus {
    let Some(task_capacity) = NonZeroUsize::new(configuration.task_capacity()) else {
        return NativeRuntimeStatus::INVALID_ARGUMENT;
    };

    let Some(timer_capacity) = NonZeroUsize::new(configuration.timer_capacity()) else {
        return NativeRuntimeStatus::INVALID_ARGUMENT;
    };

    NATIVE_RUNTIME.with(|runtime| {
        if runtime.borrow().is_some() {
            return NativeRuntimeStatus::ALREADY_INITIALIZED;
        }

        let Ok(thread) = RuntimeThreadScope::enter() else {
            return NativeRuntimeStatus::RUNTIME_FAILURE;
        };

        let scheduler = Scheduler::new(
            [
                RuntimeCapability::CooperativeExecution,
                RuntimeCapability::LocalLanes,
                RuntimeCapability::MainThreadLane,
            ],
            thread.runtime().id(),
            SchedulerLimits::new(task_capacity, timer_capacity),
        );

        runtime.replace(Some(Rc::new(NativeRuntime {
            thread,
            scheduler,
            tasks: RefCell::new(BTreeMap::new()),
            awaited: RefCell::new(BTreeMap::new()),
            resolved_awaits: RefCell::new(BTreeMap::new()),
            task_capacity,
            next_task: Cell::new(1),
            cleanup_reports: CleanupReportSink::new(),
        })));

        NativeRuntimeStatus::SUCCESS
    })
}

pub(super) fn with_runtime<T>(
    callback: impl FnOnce(&NativeRuntime) -> T,
) -> Result<T, NativeRuntimeStatus> {
    let runtime = NATIVE_RUNTIME.with(|runtime| runtime.borrow().clone());
    let runtime = runtime.ok_or(NativeRuntimeStatus::NOT_INITIALIZED)?;

    Ok(callback(&runtime))
}

pub(super) fn shutdown() -> NativeRuntimeStatus {
    let result = with_runtime(NativeRuntime::clear_tasks);

    if let Err(status) = result {
        return status;
    }

    NATIVE_RUNTIME.with(|runtime| {
        runtime.take();
    });

    NativeRuntimeStatus::SUCCESS
}

impl NativeRuntime {
    pub(super) fn allocate(&self) -> NativeTaskAllocation {
        if self.tasks.borrow().len() >= self.task_capacity.get() {
            return NativeTaskAllocation::failure(NativeRuntimeStatus::RUNTIME_FAILURE);
        }

        let next = self.next_task.get();

        let Some(handle) = NativeTaskHandle::new(next) else {
            return NativeTaskAllocation::failure(NativeRuntimeStatus::RUNTIME_FAILURE);
        };

        let Some(next) = next.checked_add(1) else {
            return NativeTaskAllocation::failure(NativeRuntimeStatus::RUNTIME_FAILURE);
        };

        self.next_task.set(next);

        self.tasks
            .borrow_mut()
            .insert(handle, NativeTaskSlot::Allocated);

        NativeTaskAllocation::success(handle)
    }

    pub(super) fn start(
        &self,
        handle: NativeTaskHandle,
        frame: NativeProtectedFrame,
    ) -> NativeRuntimeStatus {
        let Some(frame) = NativeFrame::try_new(frame) else {
            return NativeRuntimeStatus::INVALID_ARGUMENT;
        };

        let slot = self.tasks.borrow_mut().remove(&handle);

        let Some(NativeTaskSlot::Allocated) = slot else {
            return NativeRuntimeStatus::UNKNOWN_TASK;
        };

        let terminal = frame.terminal_state();

        let Ok(task) = TaskControlBlock::start(frame) else {
            return NativeRuntimeStatus::RUNTIME_FAILURE;
        };

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

        let wake = registration.wake_handle();

        if wake.wake(state).is_err() {
            return NativeRuntimeStatus::RUNTIME_FAILURE;
        }

        self.tasks.borrow_mut().insert(
            handle,
            NativeTaskSlot::Started(StartedTask {
                task,
                registration,
                terminal,
            }),
        );

        NativeRuntimeStatus::SUCCESS
    }

    pub(super) fn drive_main_thread(&self) -> NativeRuntimeStatus {
        let thread = self.thread.runtime().id();

        let lanes = [
            ExecutionLane::new(
                ExecutionLanePlacement::MainThread(thread),
                ExecutionWorkload::Cooperative,
            ),
            ExecutionLane::new(
                ExecutionLanePlacement::OriginThread(thread),
                ExecutionWorkload::Cooperative,
            ),
            ExecutionLane::new(
                ExecutionLanePlacement::PinnedWorker(thread),
                ExecutionWorkload::Cooperative,
            ),
        ];

        for lane in lanes {
            match self.scheduler.take_ready(lane) {
                Ok(Some(ready)) => return self.drive_ready(ready),
                Ok(None) => {}
                Err(_) => return NativeRuntimeStatus::RUNTIME_FAILURE,
            }
        }

        NativeRuntimeStatus::PENDING
    }

    fn drive_ready(&self, ready: crate::ReadyTask) -> NativeRuntimeStatus {
        let selected = self.tasks.borrow().iter().find_map(|(handle, slot)| {
            let NativeTaskSlot::Started(task) = slot else {
                return None;
            };

            (task.task.id() == ready.task())
                .then(|| (*handle, task.task.clone(), task.registration.wake_handle()))
        });

        let Some((handle, task, wake)) = selected else {
            return NativeRuntimeStatus::RUNTIME_FAILURE;
        };

        let context = TaskExecutionContext::new(
            task.id(),
            ready.state(),
            task.cancellation_context().clone(),
            ready.lane(),
            wake,
        );

        CURRENT_NATIVE_TASK.with(|current| current.set(Some(handle)));

        let status = with_task_execution_context(context, || task.resume());

        CURRENT_NATIVE_TASK.with(|current| current.set(None));

        let Ok(status) = status else {
            return NativeRuntimeStatus::RUNTIME_FAILURE;
        };

        match status {
            TaskResumeStatus::Suspended(suspension) => {
                let state = suspension.state();

                if ready.suspend(suspension).is_err() {
                    return NativeRuntimeStatus::RUNTIME_FAILURE;
                }

                self.register_awaited_wake(handle, state)
            }
            TaskResumeStatus::Terminal(_) => NativeRuntimeStatus::SUCCESS,
        }
    }

    pub(super) fn compose_awaited(&self, frame: NativeInactiveFrame) -> NativeRuntimeStatus {
        let Some(parent) = current_native_task() else {
            return NativeRuntimeStatus::INVALID_ARGUMENT;
        };

        if self.awaited.borrow().contains_key(&parent) {
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

        self.awaited.borrow_mut().insert(parent, child);

        NativeRuntimeStatus::SUCCESS
    }

    pub(super) fn resolve_awaited_completion(&self) -> Result<usize, NativeRuntimeStatus> {
        let parent = current_native_task().ok_or(NativeRuntimeStatus::INVALID_ARGUMENT)?;

        let child = self
            .awaited
            .borrow_mut()
            .remove(&parent)
            .ok_or(NativeRuntimeStatus::UNKNOWN_TASK)?;

        let outcome = self.observe(child);

        if outcome.state() != NativeRunState::COMPLETED || outcome.payload() == 0 {
            self.awaited.borrow_mut().insert(parent, child);

            return Err(NativeRuntimeStatus::RUNTIME_FAILURE);
        }

        self.resolved_awaits
            .borrow_mut()
            .entry(parent)
            .or_default()
            .push(child);

        Ok(outcome.payload())
    }

    pub(super) fn wake(&self, handle: NativeTaskHandle, state: u32) -> NativeRuntimeStatus {
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

    pub(super) fn request_cancellation(&self, handle: NativeTaskHandle) -> NativeRuntimeStatus {
        self.with_started(handle, |task| {
            task.task.request_cancellation();

            NativeRuntimeStatus::SUCCESS
        })
        .unwrap_or_else(|status| status)
    }

    pub(super) fn request_root_cancellation(&self, root: NativeRootHandle) -> NativeRuntimeStatus {
        let Some(task) = NativeTaskHandle::new(root.raw()) else {
            return NativeRuntimeStatus::UNKNOWN_TASK;
        };

        self.request_cancellation(task)
    }

    pub(super) fn observe_root(&self, root: NativeRootHandle) -> NativeRunOutcome {
        let Some(task) = NativeTaskHandle::new(root.raw()) else {
            return runtime_failure(NativeRuntimeStatus::UNKNOWN_TASK);
        };

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

    pub(super) fn resolve_root_completion(&self, root: NativeRootHandle) -> NativeRuntimeStatus {
        let Some(task) = NativeTaskHandle::new(root.raw()) else {
            return NativeRuntimeStatus::UNKNOWN_TASK;
        };

        let slot = self.tasks.borrow_mut().remove(&task);

        match slot {
            Some(NativeTaskSlot::Terminal { .. }) => NativeRuntimeStatus::SUCCESS,
            Some(slot) => {
                self.tasks.borrow_mut().insert(task, slot);

                NativeRuntimeStatus::PENDING
            }
            None => NativeRuntimeStatus::UNKNOWN_TASK,
        }
    }

    pub(super) fn report_cleanup_incidents(&self) -> NativeRuntimeStatus {
        let stderr = std::io::stderr();
        let mut stderr = stderr.lock();
        let mut status = NativeRuntimeStatus::SUCCESS;

        self.cleanup_reports.drain(|incident| {
            if write_cleanup_incident_report(&mut stderr, &incident).is_err() {
                status = NativeRuntimeStatus::RUNTIME_FAILURE;
            }
        });

        status
    }

    #[cfg(test)]
    pub(super) fn pending_cleanup_incidents(&self) -> usize {
        self.cleanup_reports.pending_count()
    }

    pub(super) fn join(
        &self,
        handle: NativeTaskHandle,
        callback: NativeWakeCallback,
        context: usize,
    ) -> NativeRunOutcome {
        let outcome = self.observe(handle);

        if outcome.state() != NativeRunState::PENDING {
            return outcome;
        }

        let status = self
            .with_started(handle, |task| {
                task.task
                    .register_join_waiter(Arc::new(move || callback(context)))
                    .map_or(NativeRuntimeStatus::RUNTIME_FAILURE, |()| {
                        NativeRuntimeStatus::SUCCESS
                    })
            })
            .unwrap_or_else(|status| status);

        if status.is_success() {
            outcome
        } else {
            runtime_failure(status)
        }
    }

    pub(super) fn observe(&self, handle: NativeTaskHandle) -> NativeRunOutcome {
        let slot = self.tasks.borrow_mut().remove(&handle);

        let Some(slot) = slot else {
            return runtime_failure(NativeRuntimeStatus::UNKNOWN_TASK);
        };

        let task = match slot {
            NativeTaskSlot::Started(task) => task,
            NativeTaskSlot::Terminal { outcome, task } => {
                self.tasks
                    .borrow_mut()
                    .insert(handle, NativeTaskSlot::Terminal { outcome, task });

                return outcome;
            }
            NativeTaskSlot::Allocated => {
                return runtime_failure(NativeRuntimeStatus::UNKNOWN_TASK);
            }
        };

        match task.task.take_outcome() {
            Ok(outcome) => {
                self.transfer_cleanup_incidents(&task);

                let outcome = task_outcome(outcome, &task.terminal);
                self.release_resolved_awaits(handle);

                self.tasks
                    .borrow_mut()
                    .insert(handle, NativeTaskSlot::Terminal { outcome, task });

                outcome
            }
            Err(TaskObservationError::Pending) => {
                self.tasks
                    .borrow_mut()
                    .insert(handle, NativeTaskSlot::Started(task));

                NativeRunOutcome::new(NativeRunState::PENDING, 0)
            }
            Err(TaskObservationError::RuntimeFailed(_)) => {
                self.transfer_cleanup_incidents(&task);

                let outcome = runtime_failure(NativeRuntimeStatus::RUNTIME_FAILURE);

                self.tasks
                    .borrow_mut()
                    .insert(handle, NativeTaskSlot::Terminal { outcome, task });

                outcome
            }
            Err(
                TaskObservationError::AlreadyObserved
                | TaskObservationError::SynchronizationPoisoned,
            ) => runtime_failure(NativeRuntimeStatus::RUNTIME_FAILURE),
        }
    }

    pub(super) fn lane(&self, handle: NativeTaskHandle, state: u32) -> NativeExecutionLaneResult {
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
        let tasks = self.tasks.borrow();

        let Some(NativeTaskSlot::Started(task)) = tasks.get(&handle) else {
            return Err(NativeRuntimeStatus::UNKNOWN_TASK);
        };

        Ok(callback(task))
    }

    fn clear_tasks(&self) {
        self.awaited.borrow_mut().clear();
        self.resolved_awaits.borrow_mut().clear();

        let tasks = std::mem::take(&mut *self.tasks.borrow_mut());

        drop(tasks);
    }

    fn wait_main_thread(&self) -> NativeRuntimeStatus {
        let ready = self.drive_main_thread();

        if ready != NativeRuntimeStatus::PENDING {
            return ready;
        }

        let thread = self.thread.runtime().id();

        let lane = ExecutionLane::new(
            ExecutionLanePlacement::MainThread(thread),
            ExecutionWorkload::Cooperative,
        );

        match self.scheduler.wait_ready(lane, None) {
            Ok(Some(ready)) => self.drive_ready(ready),
            Ok(None) | Err(_) => NativeRuntimeStatus::RUNTIME_FAILURE,
        }
    }

    fn transfer_cleanup_incidents(&self, task: &StartedTask) {
        let producer = CleanupIncidentProducer::Task(task.task.id());

        let origin = CleanupIncidentOrigin::new(
            task.task.descriptor().frame(),
            task.task.state_id_for_reporting(),
        );

        for incident in task.terminal.take_cleanup_incidents() {
            self.cleanup_reports
                .transfer_erased(producer, origin, incident);
        }
    }

    fn register_awaited_wake(
        &self,
        parent: NativeTaskHandle,
        state: ProtectedFrameStateId,
    ) -> NativeRuntimeStatus {
        let Some(child) = self.awaited.borrow().get(&parent).copied() else {
            return NativeRuntimeStatus::SUCCESS;
        };

        let wake = self.with_started(parent, |task| task.registration.wake_handle());

        let Ok(wake) = wake else {
            return NativeRuntimeStatus::RUNTIME_FAILURE;
        };

        self.with_started(child, |task| {
            task.task
                .register_join_waiter(Arc::new(move || {
                    let _ = wake.wake(state);
                }))
                .map_or(NativeRuntimeStatus::RUNTIME_FAILURE, |()| {
                    NativeRuntimeStatus::SUCCESS
                })
        })
        .unwrap_or_else(|status| status)
    }

    fn release_resolved_awaits(&self, parent: NativeTaskHandle) {
        let Some(children) = self.resolved_awaits.borrow_mut().remove(&parent) else {
            return;
        };

        let mut tasks = self.tasks.borrow_mut();

        for child in children {
            tasks.remove(&child);
        }
    }
}

fn current_native_task() -> Option<NativeTaskHandle> {
    CURRENT_NATIVE_TASK.with(Cell::get)
}

fn write_cleanup_incident_report(
    writer: &mut dyn Write,
    incident: &crate::CleanupIncident,
) -> std::io::Result<()> {
    write!(
        writer,
        "cleanup_incident ordinal={} producer=",
        incident.ordinal()
    )?;

    match incident.producer() {
        CleanupIncidentProducer::SynchronousRoot => write!(writer, "synchronous_root")?,
        CleanupIncidentProducer::Task(task) => write!(writer, "task:{}", task.raw())?,
    }

    write!(writer, " frame=")?;

    for byte in incident.origin().frame().digest() {
        write!(writer, "{byte:02x}")?;
    }

    writeln!(writer, " state={}", incident.origin().state().raw())
}

fn task_outcome(outcome: RunOutcome<usize>, terminal: &NativeTerminalState) -> NativeRunOutcome {
    match outcome {
        RunOutcome::Completed(payload) => NativeRunOutcome::new(NativeRunState::COMPLETED, payload),
        RunOutcome::Cancelled => NativeRunOutcome::new(NativeRunState::CANCELLED, 0),
        RunOutcome::Panicked(_) => NativeRunOutcome::new(
            NativeRunState::PANICKED,
            terminal
                .take_payload()
                .as_ref()
                .map(NativeTerminalPayload::handle)
                .unwrap_or(0),
        ),
    }
}

fn lane_result(lane: ExecutionLane) -> NativeExecutionLaneResult {
    let lane = match (lane.placement(), lane.workload()) {
        (_, ExecutionWorkload::Blocking) => NativeExecutionLane::BLOCKING,
        (_, ExecutionWorkload::Compute) => NativeExecutionLane::COMPUTE,
        (ExecutionLanePlacement::MainThread(_), _) => NativeExecutionLane::MAIN_THREAD,
        (ExecutionLanePlacement::OriginThread(_), _) => NativeExecutionLane::ORIGIN_THREAD,
        (ExecutionLanePlacement::Migratable | ExecutionLanePlacement::PinnedWorker(_), _) => {
            NativeExecutionLane::COOPERATIVE
        }
    };

    NativeExecutionLaneResult::success(lane)
}

pub(super) fn runtime_failure(status: NativeRuntimeStatus) -> NativeRunOutcome {
    NativeRunOutcome::new(NativeRunState::RUNTIME_FAILURE, status.code() as usize)
}

#[cfg(test)]
mod tests {
    use super::write_cleanup_incident_report;
    use crate::{CleanupIncidentOrigin, CleanupIncidentProducer, CleanupReportSink};

    #[test]
    fn cleanup_incident_reports_are_stable_and_observable() {
        let reports = CleanupReportSink::new();

        let origin = CleanupIncidentOrigin::new(
            bray_runtime_interface::ProtectedAsyncFrameId::new([5; 32]),
            bray_runtime_interface::ProtectedFrameStateId::new(7),
        );

        reports.transfer(
            CleanupIncidentProducer::SynchronousRoot,
            origin,
            "cleanup failed",
        );

        let mut output = Vec::new();

        reports.drain(|incident| {
            write_cleanup_incident_report(&mut output, &incident)
                .unwrap_or_else(|error| panic!("test report must write: {error}"));
        });

        let expected = format!(
            "cleanup_incident ordinal=0 producer=synchronous_root frame={} state=7\n",
            "05".repeat(32)
        );

        assert_eq!(String::from_utf8(output), Ok(expected));
    }
}
