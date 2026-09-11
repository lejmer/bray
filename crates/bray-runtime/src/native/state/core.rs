use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::ops::Deref;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use triomphe::Arc as RuntimeArc;

use bray_platform::{RuntimeThreadEntry, RuntimeThreadScope};
use bray_runtime_abi::{
    NativeRunOutcome, NativeRuntimeConfiguration, NativeRuntimeStatus, NativeTaskHandle,
};
use bray_runtime_model::RuntimeCapability;

use crate::{
    CleanupReportSink, ExecutionLanePlacement, ExecutionWorkload, JoinWaitRegistration, Scheduler,
    SchedulerLimits, TaskControlBlock, TaskRegistration,
};

use super::super::frame::NativeTerminalState;
thread_local! {
    pub(in crate::native) static NATIVE_RUNTIME: RefCell<Option<RuntimeArc<NativeRuntime>>> =
        const { RefCell::new(None) };
    pub(in crate::native) static CURRENT_NATIVE_TASK: Cell<Option<NativeTaskHandle>> =
        const { Cell::new(None) };
}

// Cleanup borrows a stack-owned context for exactly the callback's dynamic scope.
scoped_tls::scoped_thread_local!(pub(super) static CLEANUP_RUNTIME: NativeRuntime);

type NativeTask = TaskControlBlock<usize>;

pub(in crate::native) struct NativeRuntime {
    pub(in crate::native) thread: RuntimeThreadEntry,
    pub(in crate::native) main_thread_lane: bool,
    pub(in crate::native) cleanup_workloads: Cell<bool>,
    pub(in crate::native) worker: Option<triomphe::Arc<super::super::workers::WorkerControl>>,
    pub(in crate::native) core: Arc<NativeRuntimeCore>,
    #[cfg(test)]
    pub(in crate::native) _test_isolation: Option<TestRuntimeIsolation>,
}

#[cfg(test)]
static NATIVE_RUNTIME_TEST_ISOLATION: Mutex<()> = Mutex::new(());

#[cfg(test)]
thread_local! {
    static HOLDS_TEST_RUNTIME_ISOLATION: Cell<bool> = const { Cell::new(false) };
}

#[cfg(test)]
pub(in crate::native) struct TestRuntimeIsolation {
    _guard: std::sync::MutexGuard<'static, ()>,
}

#[cfg(test)]
impl Drop for TestRuntimeIsolation {
    fn drop(&mut self) {
        // A failed test can release its runtime while this thread's TLS is already being destroyed.
        let _ = HOLDS_TEST_RUNTIME_ISOLATION.try_with(|held| held.set(false));
    }
}

#[cfg(test)]
pub(in crate::native) fn test_runtime_isolation() -> Option<TestRuntimeIsolation> {
    if HOLDS_TEST_RUNTIME_ISOLATION.get() || with_runtime(|_| ()).is_ok() {
        None
    } else {
        let guard = NATIVE_RUNTIME_TEST_ISOLATION
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        HOLDS_TEST_RUNTIME_ISOLATION.set(true);

        Some(TestRuntimeIsolation { _guard: guard })
    }
}

pub(crate) struct NativeRuntimeCore {
    pub(in crate::native) scheduler: Scheduler,
    pub(in crate::native) workers: super::super::workers::WorkerPool,
    pub(in crate::native) owners: AtomicUsize,
    pub(in crate::native) tasks: Mutex<HashMap<NativeTaskHandle, NativeTaskSlot>>,
    pub(in crate::native) task_capacity: NonZeroUsize,
    pub(in crate::native) independent_tasks: AtomicUsize,
    pub(in crate::native) next_task: AtomicU64,
    pub(in crate::native) cleanup_reports: CleanupReportSink,
}

#[cfg(test)]
mod tests {
    #[test]
    fn independent_formation_preserves_the_installed_execution() {
        use std::sync::atomic::Ordering;

        assert!(
            super::initialize(bray_runtime_abi::NativeRuntimeConfiguration::new(4, 1)).is_success()
        );

        let installed =
            super::NATIVE_RUNTIME.with(|runtime| runtime.borrow().as_ref().unwrap().clone());

        let formed = super::form_runtime(
            crate::SchedulerLimits::new(
                std::num::NonZeroUsize::new(7).unwrap(),
                std::num::NonZeroUsize::new(2).unwrap(),
            ),
            [bray_runtime_model::RuntimeCapability::CooperativeExecution],
            false,
            false,
        )
        .unwrap();

        assert!(!std::sync::Arc::ptr_eq(&installed.core, &formed.core));
        assert_eq!(installed.task_capacity.get(), 4);
        assert_eq!(formed.task_capacity.get(), 7);

        assert!(super::NATIVE_RUNTIME.with(|runtime| {
            triomphe::Arc::ptr_eq(runtime.borrow().as_ref().unwrap(), &installed)
        }));

        let retained = crate::product::retain_execution().unwrap().unwrap();
        assert_eq!(installed.owners.load(Ordering::Acquire), 2);
        assert_eq!(formed.owners.load(Ordering::Acquire), 1);
        retained.release();
        drop(retained);

        formed.core.release_owner();
        assert_eq!(formed.owners.load(Ordering::Acquire), 0);
        assert!(formed.workers.is_stopping());
        assert_eq!(installed.owners.load(Ordering::Acquire), 1);
        assert!(!installed.workers.is_stopping());
        drop(formed);

        assert!(
            super::with_runtime(|runtime| std::sync::Arc::ptr_eq(&runtime.core, &installed.core))
                .unwrap()
        );

        assert!(super::shutdown().is_success());
    }

    #[test]
    fn isolation_guard_can_outlive_its_thread_local_flag() {
        std::thread::spawn(|| {
            thread_local! {
                static RETAINED_ISOLATION: std::cell::RefCell<Option<super::TestRuntimeIsolation>> = const { std::cell::RefCell::new(None) };
            }

            RETAINED_ISOLATION.with(|_| {});

            let isolation = super::test_runtime_isolation().unwrap();

            RETAINED_ISOLATION.with(|retained| *retained.borrow_mut() = Some(isolation));
        }).join().unwrap();
    }

    #[test]
    fn cleanup_binding_keeps_the_physical_worker_identity_for_retirement() {
        assert!(
            super::initialize(bray_runtime_abi::NativeRuntimeConfiguration::new(4, 1)).is_success()
        );

        let retained = super::retain_runtime().unwrap();
        assert!(super::shutdown().is_success());

        assert!(
            super::initialize(bray_runtime_abi::NativeRuntimeConfiguration::new(4, 1)).is_success()
        );

        let control = triomphe::Arc::new(crate::native::workers::WorkerControl::default());

        let core = super::NATIVE_RUNTIME.with(|active| {
            let mut active = active.borrow_mut();
            let runtime = triomphe::Arc::get_mut(active.as_mut().unwrap()).unwrap();
            runtime.worker = Some(control.clone());

            runtime.core.clone()
        });

        super::super::binding::with_cleanup_runtime(&retained, || {
            assert!(!retained.owns_current_worker());

            assert!(triomphe::Arc::ptr_eq(
                &core.current_worker().unwrap(),
                &control
            ));

            assert!(retained.core.current_worker().is_none());
        })
        .unwrap();

        super::NATIVE_RUNTIME.with(|active| {
            triomphe::Arc::get_mut(active.borrow_mut().as_mut().unwrap())
                .unwrap()
                .worker = None;
        });

        retained.release();
        assert!(super::shutdown().is_success());
    }

    #[test]
    fn worker_context_admission_failure_preserves_the_active_runtime() {
        assert!(
            super::initialize(bray_runtime_abi::NativeRuntimeConfiguration::new(4, 1)).is_success()
        );

        let core =
            super::NATIVE_RUNTIME.with(|runtime| runtime.borrow().as_ref().unwrap().core.clone());

        core.workers.stop(&core.scheduler, None);
        let thread = bray_platform::current_runtime_thread().unwrap();
        let identity = thread.id();
        let control = triomphe::Arc::new(crate::native::workers::WorkerControl::default());

        crate::test_support::with_allocation_failure(|| {
            super::run_worker(
                core.clone(),
                thread,
                crate::ExecutionWorkload::Cooperative,
                triomphe::Arc::clone(&control),
            )
        });

        assert_eq!(
            control.wait_started(),
            bray_runtime_abi::NativeRuntimeStatus::ALLOCATION_FAILURE
        );

        assert!(super::NATIVE_RUNTIME.with(|runtime| {
            runtime
                .borrow()
                .as_ref()
                .is_some_and(|runtime| std::sync::Arc::ptr_eq(&runtime.core, &core))
        }));

        assert_eq!(
            bray_platform::current_runtime_thread().unwrap().id(),
            identity
        );

        assert!(super::shutdown().is_success());
    }

    #[test]
    fn retained_runtime_admission_failure_preserves_owner_count_and_release_is_once() {
        use bray_runtime_abi::{NativeRuntimeConfiguration, NativeRuntimeStatus};
        use std::sync::atomic::Ordering;
        assert!(super::initialize(NativeRuntimeConfiguration::new(4, 1)).is_success());

        let core =
            super::NATIVE_RUNTIME.with(|runtime| runtime.borrow().as_ref().unwrap().core.clone());

        let owners = core.owners.load(Ordering::Acquire);
        let failed = crate::test_support::with_allocation_failure(super::retain_runtime);

        if let Ok(retained) = &failed {
            retained.release();
        }

        assert!(matches!(
            failed,
            Err(NativeRuntimeStatus::ALLOCATION_FAILURE)
        ));

        assert_eq!(core.owners.load(Ordering::Acquire), owners);
        let retained = super::retain_runtime().unwrap();
        let duplicate = retained.clone();
        assert_eq!(core.owners.load(Ordering::Acquire), owners + 1);

        crate::test_support::with_allocation_failure(|| {
            retained.release();
            duplicate.release();
        });

        assert_eq!(core.owners.load(Ordering::Acquire), owners);
        assert!(super::shutdown().is_success());
        assert_eq!(core.owners.load(Ordering::Acquire), 0);

        assert!(matches!(
            crate::test_support::with_allocation_failure(super::retain_runtime),
            Err(NativeRuntimeStatus::NOT_INITIALIZED)
        ));

        assert!(super::NATIVE_RUNTIME.with(|runtime| runtime.borrow().is_none()));
        assert!(bray_platform::current_runtime_thread().is_none());
    }

    #[test]
    fn callback_isolation_allows_nested_runtime_creation() {
        let isolation = super::test_runtime_isolation();

        assert!(isolation.is_some());
        assert!(super::test_runtime_isolation().is_none());

        let retained = super::admit_cleanup_runtime()
            .unwrap_or_else(|status| panic!("nested runtime must initialize: {status:?}"));

        retained.release();

        assert!(super::test_runtime_isolation().is_none());

        drop(isolation);

        assert!(super::test_runtime_isolation().is_some());
    }
}

#[derive(Clone)]
pub(crate) struct RetainedRuntime {
    pub(in crate::native) core: Arc<NativeRuntimeCore>,
    pub(in crate::native) released: triomphe::Arc<AtomicBool>,
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
        let mut tasks = self
            .tasks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let removed = std::mem::take(&mut *tasks);
        self.independent_tasks.store(0, Ordering::Relaxed);

        drop(tasks);
        drop(removed);
    }

    fn retain_owner(&self) -> bool {
        self.owners
            .try_update(Ordering::AcqRel, Ordering::Acquire, |owners| {
                owners.checked_add(1)
            })
            .is_ok()
    }

    fn current_worker(&self) -> Option<triomphe::Arc<super::super::workers::WorkerControl>> {
        // Cleanup can temporarily select another runtime while this physical worker stays owned.
        NATIVE_RUNTIME.with(|active| {
            active
                .borrow()
                .as_ref()
                .filter(|runtime| std::ptr::eq(Arc::as_ptr(&runtime.core), self))
                .and_then(|runtime| runtime.worker.clone())
        })
    }

    fn release_owner(&self) {
        if self.owners.fetch_sub(1, Ordering::AcqRel) != 1 {
            return;
        }

        let current = self.current_worker();

        self.workers.stop(&self.scheduler, current.as_ref());
        self.clear_tasks();

        super::super::event::clear(self);
    }
}

impl RetainedRuntime {
    pub(crate) fn owns_current_worker(&self) -> bool {
        with_runtime(|runtime| runtime.worker.is_some() && Arc::ptr_eq(&runtime.core, &self.core))
            .unwrap_or(false)
    }

    pub(crate) fn admit_product_worker_cleanup(
        &self,
        product: usize,
    ) -> Result<(), NativeRuntimeStatus> {
        with_runtime(|runtime| {
            let worker = Arc::ptr_eq(&runtime.core, &self.core)
                .then_some(runtime.worker.as_ref())
                .flatten()
                .ok_or(NativeRuntimeStatus::NOT_INITIALIZED)?;

            worker.admit(product)
        })?
    }

    pub(crate) fn detach_product_workers(&self, product: usize) {
        let current = self.core.current_worker();

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
    Allocated(crate::task::TaskAdmissionKind),
    Starting(crate::task::TaskAdmissionKind),
    ReturnedValue(super::start::NativeRunReservation),
    Started(triomphe::Arc<StartedTask>),
    FailedRun {
        admission: crate::task::TaskAdmissionKind,
        _run: triomphe::Arc<super::super::run::NativeRun>,
    },
    Terminal {
        outcome: TerminalOutcome,
        _task: triomphe::Arc<StartedTask>,
    },
}

impl NativeTaskSlot {
    pub(in crate::native) fn admission(&self) -> crate::task::TaskAdmissionKind {
        match self {
            Self::Allocated(kind)
            | Self::Starting(kind)
            | Self::FailedRun {
                admission: kind, ..
            } => *kind,
            Self::ReturnedValue(_) => crate::task::TaskAdmissionKind::Continuation,
            Self::Started(task) | Self::Terminal { _task: task, .. } => task.admission,
        }
    }
}

impl NativeRuntimeCore {
    pub(in crate::native) fn release_admission(&self, slot: &NativeTaskSlot) {
        if slot.admission() == crate::task::TaskAdmissionKind::Independent {
            self.independent_tasks.fetch_sub(1, Ordering::Relaxed);
        }
    }
}

pub(in crate::native) enum TerminalOutcome {
    Available(NativeRunOutcome),
    Transferring,
    Borrowed(NativeRunOutcome),
    Consumed,
}

pub(in crate::native) struct StartedTask {
    pub(in crate::native) admission: crate::task::TaskAdmissionKind,
    pub(in crate::native) task: NativeTask,
    pub(super) registration: Option<TaskRegistration>,
    pub(in crate::native) waits: Mutex<Vec<JoinWaitRegistration>>,
    pub(super) continuation: super::continuation::ContinuationWait,
    pub(super) event_wait: super::event_wait::EventWait,
    pub(in crate::native) run: triomphe::Arc<super::super::run::NativeRun>,
    pub(in crate::native) observation_claimed: AtomicBool,
    pub(in crate::native) retained_failure: AtomicBool,
    pub(in crate::native) terminal: triomphe::Arc<NativeTerminalState>,
}

impl StartedTask {
    pub(super) fn registration(&self) -> &TaskRegistration {
        self.registration.as_ref().unwrap_or_else(|| {
            unreachable!("an executable native task must have a scheduler registration")
        })
    }
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

    if with_runtime(|_| ()).is_ok() {
        return NativeRuntimeStatus::ALREADY_INITIALIZED;
    }

    let native = match form_runtime(
        SchedulerLimits::new(task_capacity, timer_capacity),
        capabilities,
        main_thread_lane,
        cleanup_workloads,
    ) {
        Ok(native) => native,
        Err(status) => return status,
    };

    NATIVE_RUNTIME.with(|runtime| {
        runtime.replace(Some(native));
    });

    crate::product::replace_execution_factory(Some(
        super::super::product_execution::retain_current_execution,
    ));

    NativeRuntimeStatus::SUCCESS
}

/// Forms a thread-owned runtime and its workers without installing a current binding.
/// The returned owner must call `core.release_owner()` exactly once before it is dropped.
pub(in crate::native) fn form_runtime(
    limits: SchedulerLimits,
    capabilities: impl IntoIterator<Item = RuntimeCapability>,
    main_thread_lane: bool,
    cleanup_workloads: bool,
) -> Result<RuntimeArc<NativeRuntime>, NativeRuntimeStatus> {
    #[cfg(test)]
    let test_isolation = test_runtime_isolation();

    let thread = match RuntimeThreadScope::enter_or_reuse() {
        Ok(thread) => thread,
        Err(error) => return Err(super::binding::thread_attachment_status(error)),
    };

    if main_thread_lane && !bray_platform::mark_current_runtime_thread_as_main() {
        return Err(NativeRuntimeStatus::RUNTIME_FAILURE);
    }

    let scheduler = Scheduler::new(capabilities, thread.runtime().id(), limits);

    let core = Arc::new(NativeRuntimeCore {
        scheduler,
        workers: super::super::workers::WorkerPool::new(),
        owners: AtomicUsize::new(1),
        tasks: Mutex::new(HashMap::new()),
        task_capacity: limits.tasks(),
        independent_tasks: AtomicUsize::new(0),
        next_task: AtomicU64::new(1),
        cleanup_reports: CleanupReportSink::new(),
    });

    let native = match crate::allocation::allocate_shared(NativeRuntime {
        thread,
        main_thread_lane,
        cleanup_workloads: Cell::new(cleanup_workloads),
        worker: None,
        core: Arc::clone(&core),
        #[cfg(test)]
        _test_isolation: test_isolation,
    }) {
        Ok(native) => native,
        Err(_) => return Err(NativeRuntimeStatus::ALLOCATION_FAILURE),
    };

    if let Err(status) = core.workers.start(&core) {
        return Err(status);
    }

    Ok(native)
}

pub(in crate::native) fn with_runtime<T>(
    callback: impl FnOnce(&NativeRuntime) -> T,
) -> Result<T, NativeRuntimeStatus> {
    if CLEANUP_RUNTIME.is_set() {
        return Ok(CLEANUP_RUNTIME.with(callback));
    }

    let runtime = NATIVE_RUNTIME.with(|runtime| runtime.borrow().clone());
    let runtime = runtime.ok_or(NativeRuntimeStatus::NOT_INITIALIZED)?;

    Ok(callback(&runtime))
}

pub(in crate::native) fn shutdown() -> NativeRuntimeStatus {
    // A temporary cleanup binding borrows its runtime owner and cannot shut it down.
    if CLEANUP_RUNTIME.is_set() {
        return NativeRuntimeStatus::INVALID_ARGUMENT;
    }

    let runtime = NATIVE_RUNTIME.take();
    crate::product::replace_execution_factory(None);

    let Some(runtime) = runtime else {
        return NativeRuntimeStatus::NOT_INITIALIZED;
    };

    runtime.core.release_owner();

    NativeRuntimeStatus::SUCCESS
}

pub(in crate::native) fn retain_runtime() -> Result<RetainedRuntime, NativeRuntimeStatus> {
    // Domain authority remains in the shared scheduler regardless of the retaining thread.
    let core = with_runtime(|runtime| Arc::clone(&runtime.core))?;

    let released = crate::allocation::allocate_shared(AtomicBool::new(false))
        .map_err(|_| NativeRuntimeStatus::ALLOCATION_FAILURE)?;

    if !core.retain_owner() {
        return Err(NativeRuntimeStatus::RUNTIME_FAILURE);
    }

    Ok(RetainedRuntime { core, released })
}

pub(in crate::native) fn admit_cleanup_runtime() -> Result<RetainedRuntime, NativeRuntimeStatus> {
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

    let retained = retain_runtime();
    let status = shutdown();

    if !status.is_success() {
        if let Ok(retained) = retained {
            retained.release();
        }

        return Err(status);
    }

    retained
}

pub(in crate::native) fn run_worker(
    core: Arc<NativeRuntimeCore>,
    thread: bray_platform::RuntimeThread,
    workload: ExecutionWorkload,
    control: triomphe::Arc<super::super::workers::WorkerControl>,
) {
    let startup = control.startup();

    let _thread_lanes = match core.scheduler.register_thread_lanes(thread.id()) {
        Ok(lanes) => lanes,
        Err(error) => {
            startup.finish(super::binding::scheduler_status(error));
            return;
        }
    };

    let runtime = match crate::allocation::allocate_shared(NativeRuntime {
        thread: RuntimeThreadEntry::Current(thread),
        main_thread_lane: false,
        cleanup_workloads: Cell::new(false),
        worker: Some(triomphe::Arc::clone(&control)),
        core,
        #[cfg(test)]
        _test_isolation: None,
    }) {
        Ok(runtime) => runtime,
        Err(_) => {
            startup.finish(NativeRuntimeStatus::ALLOCATION_FAILURE);
            return;
        }
    };

    NATIVE_RUNTIME.with(|current| {
        current.replace(Some(RuntimeArc::clone(&runtime)));
    });

    crate::product::with_execution_factory(
        super::super::product_execution::retain_current_execution,
        || {
            startup.finish(NativeRuntimeStatus::SUCCESS);

            let lanes =
                super::binding::current_thread_lanes(runtime.thread.runtime().id(), false, true);

            // Affined children stay on this worker even when their workload differs from its pool.
            // Only migratable work is restricted to the pool's workload class.
            let lanes = lanes.filter(|lane| {
                lane.placement() != ExecutionLanePlacement::Migratable
                    || lane.workload() == workload
            });

            let mut accounted_as_idle = workload == ExecutionWorkload::Blocking;

            let retain_thread = || {
                // A failed affinity query cannot authorize retiring a thread with live owners.
                // The next scheduler wait retains the synchronization failure path.
                runtime
                    .scheduler
                    .retains_thread(runtime.thread.runtime().id())
                    .unwrap_or(true)
            };

            while !runtime.workers.is_stopping() {
                control.drain();

                let deadline = bray_platform::MonotonicClock
                    .deadline_after(std::time::Duration::from_millis(50));

                match runtime.scheduler.wait_ready_from(lanes.clone(), deadline) {
                    Ok(Some(ready)) => {
                        if workload == ExecutionWorkload::Blocking {
                            runtime.workers.begin_blocking_work(&runtime.core);
                            accounted_as_idle = false;
                        }

                        let _ = runtime.drive_ready(ready);

                        if workload == ExecutionWorkload::Blocking {
                            if !runtime.workers.finish_blocking_work(retain_thread) {
                                break;
                            }

                            accounted_as_idle = true;
                        }
                    }
                    Ok(None) => {
                        if workload == ExecutionWorkload::Blocking
                            && runtime
                                .workers
                                .try_retire_idle_blocking_worker(retain_thread)
                        {
                            accounted_as_idle = false;

                            break;
                        }
                    }
                    Err(_) => break,
                }
            }

            control.drain();

            runtime
                .workers
                .retire(workload, &control, accounted_as_idle);
        },
    );

    NATIVE_RUNTIME.take();
}
