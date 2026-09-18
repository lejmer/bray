use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::ops::Deref;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use bray_platform::{RuntimeThreadEntry, RuntimeThreadId, RuntimeThreadScope};
use bray_runtime_abi::{
    NativeRunState, NativeRuntimeConfiguration, NativeRuntimeStatus, NativeTaskHandle,
};
use bray_runtime_model::RuntimeCapability;

use crate::{
    CleanupReportSink, ExecutionLane, ExecutionLanePlacement, ExecutionWorkload,
    JoinWaitRegistration, RuntimeEvent, RuntimeEventGeneration, RuntimeEventRegistration,
    Scheduler, SchedulerLimits, TaskControlBlock, TaskRegistration,
};

use super::super::frame::NativeTerminalState;
thread_local! {
    static NATIVE_CONTEXT: RefCell<NativeExecutionContext> =
        const { RefCell::new(NativeExecutionContext::empty()) };
}

struct NativeExecutionContext {
    runtime: Option<Rc<NativeRuntime>>,
    task: Option<NativeTaskHandle>,
}

impl NativeExecutionContext {
    const fn empty() -> Self {
        Self {
            runtime: None,
            task: None,
        }
    }
}

pub(in crate::native) fn current_runtime() -> Option<Rc<NativeRuntime>> {
    NATIVE_CONTEXT.with_borrow(|context| context.runtime.clone())
}

pub(in crate::native) fn current_task() -> Option<NativeTaskHandle> {
    NATIVE_CONTEXT
        .try_with(|context| context.borrow().task)
        .ok()
        .flatten()
}

fn replace_runtime(runtime: Option<Rc<NativeRuntime>>) -> Option<Rc<NativeRuntime>> {
    NATIVE_CONTEXT.with_borrow_mut(|context| std::mem::replace(&mut context.runtime, runtime))
}

pub(in crate::native) fn with_bound_runtime<T>(
    runtime: Rc<NativeRuntime>,
    callback: impl FnOnce() -> T,
) -> T {
    let previous = NATIVE_CONTEXT.replace(NativeExecutionContext {
        runtime: Some(runtime),
        task: None,
    });

    let _scope = NativeExecutionContextScope(Some(previous));

    with_independent_execution_context(callback)
}

pub(in crate::native) fn with_independent_execution_context<T>(callback: impl FnOnce() -> T) -> T {
    let task = NATIVE_CONTEXT.with_borrow_mut(|context| context.task.take());
    let _scope = NativeTaskScope(task);

    crate::context::with_independent_execution_context(callback)
}

pub(in crate::native) fn with_current_task<T>(
    task: NativeTaskHandle,
    callback: impl FnOnce() -> T,
) -> T {
    let previous = NATIVE_CONTEXT.with_borrow_mut(|context| context.task.replace(task));
    let _scope = NativeTaskScope(previous);

    callback()
}

struct NativeExecutionContextScope(Option<NativeExecutionContext>);

impl Drop for NativeExecutionContextScope {
    fn drop(&mut self) {
        let previous = self
            .0
            .take()
            .expect("native execution context scope must restore exactly once");

        NATIVE_CONTEXT.set(previous);
    }
}

struct NativeTaskScope(Option<NativeTaskHandle>);

impl Drop for NativeTaskScope {
    fn drop(&mut self) {
        NATIVE_CONTEXT.with_borrow_mut(|context| context.task = self.0);
    }
}

type NativeTask = TaskControlBlock<usize>;

pub(in crate::native) struct NativeRuntime {
    pub(in crate::native) thread: RuntimeThreadEntry,
    pub(in crate::native) main_thread_lane: bool,
    pub(in crate::native) cleanup_workloads: Cell<bool>,
    pub(in crate::native) worker: Option<Arc<super::super::workers::WorkerControl>>,
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
        HOLDS_TEST_RUNTIME_ISOLATION.set(false);
    }
}

#[cfg(test)]
pub(in crate::native) fn test_runtime_isolation() -> Option<TestRuntimeIsolation> {
    if HOLDS_TEST_RUNTIME_ISOLATION.get() || current_runtime().is_some() {
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
    pub(in crate::native) tasks: Mutex<BTreeMap<NativeTaskHandle, NativeTaskSlot>>,
    pub(in crate::native) task_capacity: NonZeroUsize,
    pub(in crate::native) next_task: AtomicU64,
    pub(in crate::native) cleanup_reports: CleanupReportSink,
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    use bray_runtime_abi::NativeTaskHandle;
    use bray_runtime_model::{ProtectedFrameStateId, RuntimeCapability};

    use crate::test_support::TestFrame;
    use crate::{
        CancellationContext, ExecutionLane, ExecutionLanePlacement, ExecutionWorkload, Scheduler,
        SchedulerLimits, TaskControlBlock, TaskExecutionContext,
    };

    extern "C" fn cancellation_requested(_: usize) -> u32 {
        1
    }

    #[test]
    fn callback_isolation_allows_nested_runtime_creation() {
        let isolation = super::test_runtime_isolation();

        assert!(isolation.is_some());
        assert!(super::test_runtime_isolation().is_none());

        let retained = super::retain_runtime()
            .unwrap_or_else(|status| panic!("nested runtime must initialize: {status:?}"));

        retained.release();
        assert!(super::test_runtime_isolation().is_none());

        drop(isolation);

        assert!(super::test_runtime_isolation().is_some());
    }

    #[test]
    fn independent_native_entry_restores_the_outer_native_task_after_unwind() {
        let task = NativeTaskHandle::new(7).expect("fixed task handle is nonzero");

        super::with_current_task(task, || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                super::with_independent_execution_context(|| {
                    assert_eq!(super::current_task(), None);
                    panic!("exercise native-task restoration");
                });
            }));

            assert!(result.is_err());
            assert_eq!(super::current_task(), Some(task));
        });

        assert_eq!(super::current_task(), None);
    }

    #[test]
    fn independent_entry_restores_complete_context_after_unwind() {
        let _isolation = super::test_runtime_isolation();

        let retained = super::retain_runtime()
            .unwrap_or_else(|status| panic!("runtime must initialize: {status:?}"));

        let ((), status) = super::super::binding::with_cleanup_runtime(Some(&retained), || {
            let runtime = super::current_runtime().expect("cleanup runtime must be bound");

            let task = TaskControlBlock::start(
                crate::test_support::admit_task(),
                TestFrame::completing(1),
            );

            let cancellation = CancellationContext::root();

            assert!(cancellation.request());

            let lane = ExecutionLane::new(
                ExecutionLanePlacement::PinnedWorker(runtime.thread.runtime().id()),
                ExecutionWorkload::Cooperative,
            );

            let scheduler = Scheduler::new(
                [RuntimeCapability::CooperativeExecution],
                runtime.thread.runtime().id(),
                SchedulerLimits::new(
                    NonZeroUsize::new(1).expect("fixed worker limit is nonzero"),
                    NonZeroUsize::new(1).expect("fixed timer limit is nonzero"),
                ),
            );

            let state = ProtectedFrameStateId::new(0);

            let registration = scheduler
                .register_task(
                    task.id(),
                    task.descriptor().clone(),
                    runtime.thread.runtime().id(),
                    state,
                    &cancellation,
                )
                .expect("test task must register");

            #[cfg(feature = "test-output")]
            let output = Some(bray_platform::RunOutputContext::discarded());

            #[cfg(not(feature = "test-output"))]
            let output = ();

            let execution = TaskExecutionContext::new(
                task.id(),
                state,
                cancellation,
                output,
                lane,
                registration.wake_handle(),
            );

            let native_task = NativeTaskHandle::new(7).expect("fixed task handle is nonzero");

            crate::context::with_native_thread_cancellation(cancellation_requested, 0, || {
                crate::context::with_task_execution_context(execution, || {
                    super::with_current_task(native_task, || {
                        assert_eq!(crate::context::current_task_execution_lane(), Some(lane));
                        assert!(crate::current_run_cancellation_requested());

                        #[cfg(feature = "test-output")]
                        assert!(crate::context::current_task_output().is_some());

                        let result = catch_unwind(AssertUnwindSafe(|| {
                            super::super::binding::with_cleanup_runtime(Some(&retained), || {
                                assert!(super::current_runtime().is_some());
                                assert_eq!(super::current_task(), None);
                                assert!(crate::context::current_task_execution_context().is_none());
                                assert_eq!(crate::context::current_task_execution_lane(), None);
                                assert!(!crate::current_run_cancellation_requested());

                                #[cfg(feature = "test-output")]
                                assert!(crate::context::current_task_output().is_none());

                                panic!("exercise unwinding restoration");
                            })
                            .expect("nested cleanup entry must bind");
                        }));

                        assert!(result.is_err());
                        assert_eq!(super::current_task(), Some(native_task));
                        assert_eq!(crate::context::current_task_execution_lane(), Some(lane));
                        assert!(crate::current_run_cancellation_requested());

                        #[cfg(feature = "test-output")]
                        assert!(crate::context::current_task_output().is_some());
                    });
                });
            });
        })
        .unwrap_or_else(|status| panic!("cleanup entry must bind: {status:?}"));

        assert!(status.is_success());
        assert!(super::current_runtime().is_none());
        assert_eq!(super::current_task(), None);

        retained.release();
    }
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

        let current = current_runtime()
            .filter(|runtime| std::ptr::eq(Arc::as_ptr(&runtime.core), self))
            .and_then(|runtime| runtime.worker.clone());

        self.workers.stop(&self.scheduler, current.as_ref());
        self.clear_tasks();

        super::super::event::clear(self);
    }
}

impl RetainedRuntime {
    pub(crate) fn owns_current_worker(&self) -> bool {
        current_runtime().is_some_and(|runtime| {
            runtime.worker.is_some() && Arc::ptr_eq(&runtime.core, &self.core)
        })
    }

    pub(crate) fn detach_product_workers(&self, product: usize) {
        let current = current_runtime()
            .filter(|runtime| Arc::ptr_eq(&runtime.core, &self.core))
            .and_then(|runtime| runtime.worker.clone());

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
    Allocated(crate::TaskAdmission),
    Started(Arc<StartedTask>),
    Terminal {
        state: NativeRunState,
        payload: usize,
        _task: Arc<StartedTask>,
    },
}

pub(in crate::native) struct StartedTask {
    pub(in crate::native) task: Arc<NativeTask>,
    pub(in crate::native) run: Arc<super::super::run::NativeRun>,
    pub(in crate::native) registration: TaskRegistration,
    pub(in crate::native) waits: Mutex<Vec<JoinWaitRegistration<usize>>>,
    pub(in crate::native) suspended_wait: Mutex<Option<SuspendedWait>>,
    pub(in crate::native) observation_claimed: AtomicBool,
    pub(in crate::native) terminal: Arc<NativeTerminalState>,
}

pub(in crate::native) enum SuspendedWait {
    Event {
        event: RuntimeEvent,
        observed: RuntimeEventGeneration,
        _registration: RuntimeEventRegistration,
    },
}

impl SuspendedWait {
    pub(in crate::native) fn is_ready(&self) -> bool {
        match self {
            Self::Event {
                event, observed, ..
            } => {
                let (generation, closed) = event.observation();

                closed || generation != *observed
            }
        }
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

    if current_runtime().is_some() {
        return NativeRuntimeStatus::ALREADY_INITIALIZED;
    }

    #[cfg(test)]
    let test_isolation = test_runtime_isolation();

    let Ok(thread) = RuntimeThreadScope::enter_or_reuse() else {
        return NativeRuntimeStatus::RUNTIME_FAILURE;
    };

    if main_thread_lane && !bray_platform::mark_current_runtime_thread_as_main() {
        return NativeRuntimeStatus::RUNTIME_FAILURE;
    }

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
        task_capacity,
        next_task: AtomicU64::new(1),
        cleanup_reports: CleanupReportSink::new(),
    });

    if !core.workers.start(&core) {
        return NativeRuntimeStatus::RUNTIME_FAILURE;
    }

    let previous = replace_runtime(Some(Rc::new(NativeRuntime {
        thread,
        main_thread_lane,
        cleanup_workloads: Cell::new(cleanup_workloads),
        worker: None,
        core,
        #[cfg(test)]
        _test_isolation: test_isolation,
    })));

    debug_assert!(
        previous.is_none(),
        "runtime initialization checked current context"
    );

    NativeRuntimeStatus::SUCCESS
}

pub(in crate::native) fn with_runtime<T>(
    callback: impl FnOnce(&NativeRuntime) -> T,
) -> Result<T, NativeRuntimeStatus> {
    let runtime = current_runtime();
    let runtime = runtime.ok_or(NativeRuntimeStatus::NOT_INITIALIZED)?;

    Ok(callback(&runtime))
}

pub(in crate::native) fn shutdown() -> NativeRuntimeStatus {
    let runtime = replace_runtime(None);

    let Some(runtime) = runtime else {
        return NativeRuntimeStatus::NOT_INITIALIZED;
    };

    runtime.core.release_owner();

    NativeRuntimeStatus::SUCCESS
}

pub(crate) fn retain_runtime() -> Result<RetainedRuntime, NativeRuntimeStatus> {
    if let Some(runtime) = current_runtime() {
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

    let runtime = replace_runtime(None);

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
        #[cfg(test)]
        _test_isolation: None,
    });

    let previous = replace_runtime(Some(Rc::clone(&runtime)));

    debug_assert!(
        previous.is_none(),
        "worker starts without a runtime context"
    );

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

    replace_runtime(None);
}
