use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::ops::Deref;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use bray_platform::{RuntimeThreadEntry, RuntimeThreadId, RuntimeThreadScope};
use bray_runtime_abi::{
    NativeRunOutcome, NativeRuntimeConfiguration, NativeRuntimeStatus, NativeTaskHandle,
};
use bray_runtime_model::RuntimeCapability;

use crate::{
    CleanupReportSink, ExecutionLane, ExecutionLanePlacement, ExecutionWorkload,
    JoinWaitRegistration, Scheduler, SchedulerLimits, TaskControlBlock, TaskRegistration,
};

use super::super::frame::NativeTerminalState;
thread_local! {
    pub(in crate::native) static NATIVE_RUNTIME: RefCell<Option<Rc<NativeRuntime>>> =
        const { RefCell::new(None) };
    pub(in crate::native) static CURRENT_NATIVE_TASK: Cell<Option<NativeTaskHandle>> =
        const { Cell::new(None) };
}

type NativeTask = TaskControlBlock<usize>;

pub(in crate::native) struct NativeRuntime {
    pub(in crate::native) thread: RuntimeThreadEntry,
    pub(in crate::native) main_thread_lane: bool,
    pub(in crate::native) cleanup_workloads: Cell<bool>,
    pub(in crate::native) worker: Option<Arc<super::super::workers::WorkerControl>>,
    pub(in crate::native) core: Arc<NativeRuntimeCore>,
}

pub(crate) struct NativeRuntimeCore {
    pub(in crate::native) scheduler: Scheduler,
    pub(in crate::native) workers: super::super::workers::WorkerPool,
    pub(in crate::native) owners: AtomicUsize,
    pub(in crate::native) tasks: Mutex<BTreeMap<NativeTaskHandle, NativeTaskSlot>>,
    pub(in crate::native) awaited: Mutex<BTreeMap<NativeTaskHandle, NativeTaskHandle>>,
    pub(in crate::native) resolved_awaits: Mutex<BTreeMap<NativeTaskHandle, Vec<NativeTaskHandle>>>,
    pub(in crate::native) task_capacity: NonZeroUsize,
    pub(in crate::native) next_task: AtomicU64,
    pub(in crate::native) cleanup_reports: CleanupReportSink,
}

#[derive(Clone)]
pub(crate) struct RetainedRuntime {
    pub(in crate::native) core: Arc<NativeRuntimeCore>,
    pub(in crate::native) main_thread: Option<RuntimeThreadId>,
    pub(in crate::native) released: Arc<AtomicBool>,
}

impl Deref for NativeRuntime {
    type Target = NativeRuntimeCore;

    fn deref(&self) -> &Self::Target {
        &self.core
    }
}

impl NativeRuntimeCore {
    pub(in crate::native) const fn scheduler(&self) -> &Scheduler {
        &self.scheduler
    }

    fn clear_tasks(&self) {
        self.awaited
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();

        self.resolved_awaits
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();

        let tasks = std::mem::take(
            &mut *self
                .tasks
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );

        drop(tasks);
    }

    fn retain_owner(&self) -> bool {
        self.owners
            .try_update(Ordering::AcqRel, Ordering::Acquire, |owners| {
                owners.checked_add(1)
            })
            .is_ok()
    }

    fn release_owner(&self) {
        if self.owners.fetch_sub(1, Ordering::AcqRel) != 1 {
            return;
        }

        let current = NATIVE_RUNTIME.with(|runtime| {
            runtime
                .borrow()
                .as_ref()
                .filter(|runtime| std::ptr::eq(Arc::as_ptr(&runtime.core), self))
                .and_then(|runtime| runtime.worker.clone())
        });

        self.workers.stop(&self.scheduler, current.as_ref());
        self.clear_tasks();
    }
}

impl RetainedRuntime {
    pub(crate) fn owns_current_worker(&self) -> bool {
        NATIVE_RUNTIME.with(|runtime| {
            runtime.borrow().as_ref().is_some_and(|runtime| {
                runtime.worker.is_some() && Arc::ptr_eq(&runtime.core, &self.core)
            })
        })
    }

    pub(crate) fn detach_product_workers(&self, product: usize) {
        let current = NATIVE_RUNTIME.with(|runtime| {
            runtime
                .borrow()
                .as_ref()
                .filter(|runtime| Arc::ptr_eq(&runtime.core, &self.core))
                .and_then(|runtime| runtime.worker.clone())
        });

        self.core
            .workers
            .detach_product(product, current.as_ref(), &self.core.scheduler);
    }

    pub(crate) fn release(&self) {
        if self.released.swap(true, Ordering::AcqRel) {
            return;
        }

        self.core.release_owner();
    }
}

pub(in crate::native) enum NativeTaskSlot {
    Allocated,
    Started(Arc<StartedTask>),
    Terminal {
        outcome: NativeRunOutcome,
        _task: Arc<StartedTask>,
    },
}

pub(in crate::native) struct StartedTask {
    pub(in crate::native) task: Arc<NativeTask>,
    pub(in crate::native) registration: TaskRegistration,
    pub(in crate::native) waits: Mutex<Vec<JoinWaitRegistration<usize>>>,
    pub(in crate::native) observation_claimed: AtomicBool,
    pub(in crate::native) terminal: Arc<NativeTerminalState>,
}

pub(in crate::native) fn initialize(
    configuration: NativeRuntimeConfiguration,
) -> NativeRuntimeStatus {
    initialize_with_capabilities(
        configuration,
        [
            RuntimeCapability::CooperativeExecution,
            RuntimeCapability::LocalLanes,
            RuntimeCapability::MigratableLanes,
            RuntimeCapability::BlockingLanes,
            RuntimeCapability::ComputeLanes,
            RuntimeCapability::MainThreadLane,
        ],
        true,
        false,
    )
}

fn initialize_with_capabilities(
    configuration: NativeRuntimeConfiguration,
    capabilities: impl IntoIterator<Item = RuntimeCapability>,
    main_thread_lane: bool,
    cleanup_workloads: bool,
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

        let Ok(thread) = RuntimeThreadScope::enter_or_reuse() else {
            return NativeRuntimeStatus::RUNTIME_FAILURE;
        };

        let scheduler = Scheduler::new(
            capabilities,
            thread.runtime().id(),
            SchedulerLimits::new(task_capacity, timer_capacity),
        );

        let core = Arc::new(NativeRuntimeCore {
            scheduler,
            workers: super::super::workers::WorkerPool::new(),
            owners: AtomicUsize::new(1),
            tasks: Mutex::new(BTreeMap::new()),
            awaited: Mutex::new(BTreeMap::new()),
            resolved_awaits: Mutex::new(BTreeMap::new()),
            task_capacity,
            next_task: AtomicU64::new(1),
            cleanup_reports: CleanupReportSink::new(),
        });

        if !core.workers.start(&core) {
            return NativeRuntimeStatus::RUNTIME_FAILURE;
        }

        runtime.replace(Some(Rc::new(NativeRuntime {
            thread,
            main_thread_lane,
            cleanup_workloads: Cell::new(cleanup_workloads),
            worker: None,
            core,
        })));

        NativeRuntimeStatus::SUCCESS
    })
}

pub(in crate::native) fn with_runtime<T>(
    callback: impl FnOnce(&NativeRuntime) -> T,
) -> Result<T, NativeRuntimeStatus> {
    let runtime = NATIVE_RUNTIME.with(|runtime| runtime.borrow().clone());
    let runtime = runtime.ok_or(NativeRuntimeStatus::NOT_INITIALIZED)?;

    Ok(callback(&runtime))
}

pub(in crate::native) fn shutdown() -> NativeRuntimeStatus {
    let runtime = NATIVE_RUNTIME.take();

    let Some(runtime) = runtime else {
        return NativeRuntimeStatus::NOT_INITIALIZED;
    };

    runtime.core.release_owner();

    NativeRuntimeStatus::SUCCESS
}

pub(crate) fn retain_runtime() -> Result<RetainedRuntime, NativeRuntimeStatus> {
    if let Some(runtime) = NATIVE_RUNTIME.with(|runtime| runtime.borrow().clone()) {
        if !runtime.core.retain_owner() {
            return Err(NativeRuntimeStatus::RUNTIME_FAILURE);
        }

        return Ok(RetainedRuntime {
            core: Arc::clone(&runtime.core),
            main_thread: runtime
                .main_thread_lane
                .then(|| runtime.thread.runtime().id()),
            released: Arc::new(AtomicBool::new(false)),
        });
    }

    let status = initialize_with_capabilities(
        NativeRuntimeConfiguration::new(usize::MAX, usize::MAX),
        [
            RuntimeCapability::CooperativeExecution,
            RuntimeCapability::LocalLanes,
            RuntimeCapability::MigratableLanes,
            RuntimeCapability::BlockingLanes,
            RuntimeCapability::ComputeLanes,
        ],
        false,
        false,
    );

    if !status.is_success() {
        return Err(status);
    }

    let runtime = NATIVE_RUNTIME.take();

    let Some(runtime) = runtime else {
        return Err(NativeRuntimeStatus::RUNTIME_FAILURE);
    };

    let retained = RetainedRuntime {
        core: Arc::clone(&runtime.core),
        main_thread: None,
        released: Arc::new(AtomicBool::new(false)),
    };

    drop(runtime);

    Ok(retained)
}

pub(in crate::native) fn run_worker(
    core: Arc<NativeRuntimeCore>,
    thread: bray_platform::RuntimeThread,
    workload: ExecutionWorkload,
    control: Arc<super::super::workers::WorkerControl>,
) {
    let runtime = Rc::new(NativeRuntime {
        thread: RuntimeThreadEntry::Current(thread),
        main_thread_lane: false,
        cleanup_workloads: Cell::new(false),
        worker: Some(Arc::clone(&control)),
        core,
    });

    NATIVE_RUNTIME.with(|current| {
        current.replace(Some(Rc::clone(&runtime)));
    });

    let lane = ExecutionLane::new(ExecutionLanePlacement::Migratable, workload);
    let mut accounted_as_idle = workload == ExecutionWorkload::Blocking;

    while !runtime.workers.is_stopping() {
        control.drain();

        let deadline =
            bray_platform::MonotonicClock.deadline_after(std::time::Duration::from_millis(50));

        match runtime.scheduler.wait_ready(lane, deadline) {
            Ok(Some(ready)) => {
                if workload == ExecutionWorkload::Blocking {
                    runtime.workers.begin_blocking_work(&runtime.core);
                    accounted_as_idle = false;
                }

                let _ = runtime.drive_ready(ready);

                if workload == ExecutionWorkload::Blocking {
                    if !runtime.workers.finish_blocking_work() {
                        break;
                    }

                    accounted_as_idle = true;
                }
            }
            Ok(None) => {}
            Err(_) => break,
        }
    }

    control.drain();

    runtime
        .workers
        .retire(workload, &control, accounted_as_idle);

    NATIVE_RUNTIME.take();
}
