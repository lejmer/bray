use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use bray_platform::RuntimeThreadScope;
use bray_runtime_interface::{
    NativeExecutionLane, NativeExecutionLaneResult, NativeProtectedFrame,
    NativeRunOutcome, NativeRunState, NativeRuntimeConfiguration,
    NativeRuntimeStatus, NativeTaskAllocation, NativeTaskHandle,
    NativeWakeCallback, ProtectedFrameStateId, RuntimeCapability,
};

use crate::{
    ExecutionLane, ExecutionLanePlacement, ExecutionWorkload, RunOutcome,
    Scheduler, SchedulerLimits, TaskControlBlock, TaskExecutionContext,
    TaskObservationError, TaskRegistration, TaskResumeStatus,
};
use crate::context::with_task_execution_context;

use super::frame::{NativeFrame, NativeTerminalPayload};

thread_local! {
    static NATIVE_RUNTIME: RefCell<Option<Rc<NativeRuntime>>> =
        const { RefCell::new(None) };
}

type NativeTask = TaskControlBlock<usize>;

pub(super) struct NativeRuntime {
    thread: RuntimeThreadScope,
    scheduler: Scheduler,
    tasks: RefCell<BTreeMap<NativeTaskHandle, NativeTaskSlot>>,
    task_capacity: NonZeroUsize,
    next_task: Cell<u64>,
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
    terminal_payload: Arc<Mutex<Option<NativeTerminalPayload>>>,
}

pub(super) fn initialize(
    configuration: NativeRuntimeConfiguration,
) -> NativeRuntimeStatus {
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
            task_capacity,
            next_task: Cell::new(1),
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
            return NativeTaskAllocation::failure(
                NativeRuntimeStatus::RUNTIME_FAILURE,
            );
        }

        let next = self.next_task.get();

        let Some(handle) = NativeTaskHandle::new(next) else {
            return NativeTaskAllocation::failure(
                NativeRuntimeStatus::RUNTIME_FAILURE,
            );
        };

        let Some(next) = next.checked_add(1) else {
            return NativeTaskAllocation::failure(
                NativeRuntimeStatus::RUNTIME_FAILURE,
            );
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

        let terminal_payload = frame.terminal_payload();

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
                terminal_payload,
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
        let selected = self.tasks.borrow().iter().find_map(|(_, slot)| {
            let NativeTaskSlot::Started(task) = slot else {
                return None;
            };

            (task.task.id() == ready.task()).then(|| {
                (
                    task.task.clone(),
                    task.registration.wake_handle(),
                )
            })
        });

        let Some((task, wake)) = selected else {
            return NativeRuntimeStatus::RUNTIME_FAILURE;
        };

        let context = TaskExecutionContext::new(
            task.id(),
            ready.state(),
            task.cancellation_context().clone(),
            ready.lane(),
            wake,
        );

        let Ok(status) =
            with_task_execution_context(context, || task.resume())
        else {
            return NativeRuntimeStatus::RUNTIME_FAILURE;
        };

        match status {
            TaskResumeStatus::Suspended(suspension) => ready
                .suspend(suspension)
                .map_or(NativeRuntimeStatus::RUNTIME_FAILURE, |()| {
                    NativeRuntimeStatus::SUCCESS
                }),
            TaskResumeStatus::Terminal(_) => NativeRuntimeStatus::SUCCESS,
        }
    }

    pub(super) fn wake(
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

    pub(super) fn request_cancellation(
        &self,
        handle: NativeTaskHandle,
    ) -> NativeRuntimeStatus {
        self.with_started(handle, |task| {
            task.task.request_cancellation();

            NativeRuntimeStatus::SUCCESS
        })
        .unwrap_or_else(|status| status)
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

        let status = self.with_started(handle, |task| {
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

    pub(super) fn observe(
        &self,
        handle: NativeTaskHandle,
    ) -> NativeRunOutcome {
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
                let outcome = task_outcome(outcome, &task.terminal_payload);

                self.tasks.borrow_mut().insert(
                    handle,
                    NativeTaskSlot::Terminal { outcome, task },
                );

                outcome
            }
            Err(TaskObservationError::Pending) => {
                self.tasks
                    .borrow_mut()
                    .insert(handle, NativeTaskSlot::Started(task));

                NativeRunOutcome::new(NativeRunState::PENDING, 0)
            }
            Err(
                TaskObservationError::AlreadyObserved
                | TaskObservationError::RuntimeFailed(_)
                | TaskObservationError::SynchronizationPoisoned,
            ) => runtime_failure(NativeRuntimeStatus::RUNTIME_FAILURE),
        }
    }

    pub(super) fn lane(
        &self,
        handle: NativeTaskHandle,
        state: u32,
    ) -> NativeExecutionLaneResult {
        self.with_started(handle, |task| {
            task.registration
                .lane(ProtectedFrameStateId::new(state))
                .map(lane_result)
                .unwrap_or_else(|_| {
                    NativeExecutionLaneResult::failure(
                        NativeRuntimeStatus::RUNTIME_FAILURE,
                    )
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
        let tasks = std::mem::take(&mut *self.tasks.borrow_mut());

        drop(tasks);
    }
}

fn task_outcome(
    outcome: RunOutcome<usize>,
    terminal_payload: &Mutex<Option<NativeTerminalPayload>>,
) -> NativeRunOutcome {
    match outcome {
        RunOutcome::Completed(payload) => {
            NativeRunOutcome::new(NativeRunState::COMPLETED, payload)
        }
        RunOutcome::Cancelled => {
            NativeRunOutcome::new(NativeRunState::CANCELLED, 0)
        }
        RunOutcome::Panicked(_) => NativeRunOutcome::new(
            NativeRunState::PANICKED,
            terminal_payload
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take()
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
        (ExecutionLanePlacement::MainThread(_), _) => {
            NativeExecutionLane::MAIN_THREAD
        }
        (ExecutionLanePlacement::OriginThread(_), _) => {
            NativeExecutionLane::ORIGIN_THREAD
        }
        (
            ExecutionLanePlacement::Migratable
            | ExecutionLanePlacement::PinnedWorker(_),
            _,
        ) => NativeExecutionLane::COOPERATIVE,
    };

    NativeExecutionLaneResult::success(lane)
}

pub(super) fn runtime_failure(status: NativeRuntimeStatus) -> NativeRunOutcome {
    NativeRunOutcome::new(
        NativeRunState::RUNTIME_FAILURE,
        status.code() as usize,
    )
}
