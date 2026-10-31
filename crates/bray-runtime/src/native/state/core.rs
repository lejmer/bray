use std::cell::{Cell, RefCell};
use std::mem::ManuallyDrop;
use std::num::NonZeroUsize;
use std::ops::Deref;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
#[cfg(test)]
use std::sync::Mutex;

use crate::bootstrap::{RuntimeThreadEntry, RuntimeThreadScope};
use bray_platform::RuntimeThreadId;
use bray_runtime_abi::{
    NativeRuntimeConfiguration, NativeRuntimeStatus, NativeTaskHandle,
};
use bray_runtime_model::RuntimeCapability;

use crate::{
    CleanupReportSink, ExecutionLane, ExecutionLanePlacement, ExecutionWorkload,
    Scheduler, SchedulerLimits,
};

// Context contents belong to explicit scopes and runtime retirement. The slot itself
// remains available to native TLS callbacks regardless of Rust destructor order.
thread_local! {
    static NATIVE_CONTEXT: ManuallyDrop<RefCell<NativeExecutionContext>> = {
        crate::context::initialize_thread_context();
        #[cfg(feature = "test-output")]
        super::super::host::initialize_thread_context();
        ManuallyDrop::new(RefCell::new(NativeExecutionContext::empty()))
    };
    static RETIREMENT: ThreadRetirement = const { ThreadRetirement };
}

/// Establishes context storage before an attachment can publish its cleanup owner.
#[cfg(feature = "test-output")]
pub(crate) fn initialize_thread_context() {
    NATIVE_CONTEXT.with(|_| {});
}

struct ThreadRetirement;

impl Drop for ThreadRetirement {
    fn drop(&mut self) {
        // Retire this owner explicitly. Product and frame owners retain their cleanup
        // backing, and context slots remain available across native TLS callbacks.
        retire_runtime(replace_runtime(None));
    }
}

enum NativeRuntimeBinding {
    Owned(Rc<NativeRuntime>),
    Borrowed(Rc<NativeRuntime>),
}

struct NativeExecutionContext {
    runtime: Option<NativeRuntimeBinding>,
    task: Option<NativeTaskHandle>,
    services: Option<Arc<super::super::NativeTaskServices>>,
}

impl NativeExecutionContext {
    const fn empty() -> Self {
        Self {
            runtime: None,
            task: None,
            services: None,
        }
    }
}

pub(in crate::native) fn current_runtime() -> Option<Rc<NativeRuntime>> {
    NATIVE_CONTEXT.with(|cell| {
        let context = &*cell.borrow();

        context.runtime.as_ref().map(|binding| {
            let (NativeRuntimeBinding::Owned(runtime) | NativeRuntimeBinding::Borrowed(runtime)) =
                binding;

            Rc::clone(runtime)
        })
    })
}

pub(in crate::native) fn current_task() -> Option<NativeTaskHandle> {
    NATIVE_CONTEXT
        .try_with(|context| context.borrow().task)
        .ok()
        .flatten()
}

pub(in crate::native) fn current_services() -> Option<Arc<super::super::NativeTaskServices>> {
    NATIVE_CONTEXT.try_with(|context| context.borrow().services.as_ref().map(Arc::clone))
        .ok().flatten()
}

pub(in crate::native) fn current_reference_address() -> Option<u64> {
    NATIVE_CONTEXT.try_with(|context| {
        context.borrow().services.as_ref().map(|task| {
            u64::try_from(std::ptr::from_ref(&task.control).addr())
                .expect("native task-reference addresses fit the execution ABI")
        })
    }).ok().flatten()
}

pub(in crate::native) fn park_current_execution() -> Option<crate::scheduler::ExecutionPark> {
    current_services().and_then(|task| task.registration.park_execution())
}

fn replace_runtime(runtime: Option<NativeRuntimeBinding>) -> Option<NativeRuntimeBinding> {
    NATIVE_CONTEXT.with(|cell| {
        let context = &mut *cell.borrow_mut();

        std::mem::replace(&mut context.runtime, runtime)
    })
}

fn retire_runtime(runtime: Option<NativeRuntimeBinding>) {
    if let Some(NativeRuntimeBinding::Owned(runtime)) = runtime {
        runtime.core.release_owner();
    }
}

pub(in crate::native) fn with_bound_runtime<T>(
    runtime: Rc<NativeRuntime>,
    callback: impl FnOnce() -> T,
) -> T {
    let _parked = park_current_execution();

    let previous = NATIVE_CONTEXT.with(|cell| {
        cell.replace(NativeExecutionContext {
            runtime: Some(NativeRuntimeBinding::Borrowed(runtime)),
            task: None,
            services: None,
        })
    });

    crate::context::with_independent_execution_context(|| {
        let _scope = NativeExecutionContextScope(Some(previous));

        callback()
    })
}

pub(in crate::native) fn with_independent_execution_context<T>(callback: impl FnOnce() -> T) -> T {
    let _parked = park_current_execution();
    let previous = NATIVE_CONTEXT.with(|cell| cell.replace(NativeExecutionContext::empty()));

    crate::context::with_independent_execution_context(|| {
        let _scope = NativeExecutionContextScope(Some(previous));

        callback()
    })
}

/// A returned root handle remains owned by the caller's runtime after entry returns.
pub(in crate::native) fn with_root_execution_context<T>(callback: impl FnOnce() -> T) -> T {
    let _parked = park_current_execution();

    let previous = NATIVE_CONTEXT.with(|cell| {
        let context = &mut *cell.borrow_mut();

        (context.task.take(), context.services.take())
    });

    let _scope = NativeTaskScope(previous);

    crate::context::with_independent_execution_context(callback)
}

pub(in crate::native) fn with_current_task<T>(
    task: NativeTaskHandle,
    callback: impl FnOnce() -> T,
) -> T {
    let previous = NATIVE_CONTEXT.with(|cell| {
        let context = &mut *cell.borrow_mut();

        (context.task.replace(task), context.services.take())
    });

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

        // A callback can initialize its own runtime. Retire that owner before
        // restoring the caller; borrowed cleanup bindings carry no owner count.
        let runtime = replace_runtime(None);

        let cleanup =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| retire_runtime(runtime)));

        NATIVE_CONTEXT.with(|cell| drop(cell.replace(previous)));

        if let Err(payload) = cleanup {
            std::panic::resume_unwind(payload);
        }
    }
}

pub(in crate::native) fn with_task_services<T>(
    services: Arc<super::super::NativeTaskServices>,
    callback: impl FnOnce() -> T,
) -> T {
    let previous = NATIVE_CONTEXT.with(|cell| {
        let context = &mut *cell.borrow_mut();

        (context.task.replace(services.handle()), context.services.replace(services))
    });

    let _scope = NativeTaskScope(previous);

    callback()
}

struct NativeTaskScope((Option<NativeTaskHandle>, Option<Arc<super::super::NativeTaskServices>>));

impl Drop for NativeTaskScope {
    fn drop(&mut self) {
        NATIVE_CONTEXT.with(|cell| {
            let context = &mut *cell.borrow_mut();

            context.task = self.0.0;
            context.services = self.0.1.take();
        });
    }
}

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
pub(crate) struct TestRuntimeIsolation {
    _guard: std::sync::MutexGuard<'static, ()>,
}

#[cfg(test)]
impl Drop for TestRuntimeIsolation {
    fn drop(&mut self) {
        HOLDS_TEST_RUNTIME_ISOLATION.set(false);
    }
}

#[cfg(test)]
pub(crate) fn test_runtime_isolation() -> Option<TestRuntimeIsolation> {
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
    pub(in crate::native) identity: u64,
    pub(in crate::native) cleanup_reports: CleanupReportSink,
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::rc::Rc;
    use std::sync::atomic::Ordering;

    use bray_runtime_abi::{NativeRuntimeConfiguration, NativeTaskHandle};
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
    fn thread_exit_retires_the_runtime_owner_and_stops_workers() {
        for observe_first in [false, true] {
            let cores = std::thread::spawn(move || {
                if observe_first {
                    assert!(super::current_runtime().is_none());
                }

                let mut cores = Vec::new();

                for explicit_shutdown in [true, false] {
                    assert!(super::initialize(NativeRuntimeConfiguration::new(4, 4)).is_success());

                    let runtime = super::current_runtime().expect("runtime is initialized");

                    cores.push(std::sync::Arc::clone(&runtime.core));
                    assert_eq!(runtime.core.owners.load(Ordering::Acquire), 1);
                    drop(runtime);

                    if explicit_shutdown {
                        assert!(super::shutdown().is_success());
                    }
                }

                cores
            })
            .join()
            .expect("native owner retirement completes before thread exit");

            for core in cores {
                assert_eq!(core.owners.load(Ordering::Acquire), 0);
                assert!(core.workers.is_stopping());

                assert_eq!(core.scheduler.task_count().expect("fixture scheduler is available"), 0);
            }
        }
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

        let ((), status) = super::super::binding::with_cleanup_runtime(&retained, || {
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
                    task.execution_state(),
                    runtime.thread.runtime().id(),
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
                &output,
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

                        for unwind in [false, true] {
                            let result = catch_unwind(AssertUnwindSafe(|| {
                                super::with_independent_execution_context(|| {
                                    assert!(super::current_runtime().is_none());
                                    assert_eq!(super::current_task(), None);
                                    assert!(crate::current_task_execution_context().is_none());
                                    assert!(!crate::current_run_cancellation_requested());

                                    if unwind {
                                        panic!("exercise complete entry restoration");
                                    }
                                });
                            }));

                            assert_eq!(result.is_err(), unwind);

                            assert!(std::rc::Rc::ptr_eq(
                                &super::current_runtime().expect("fixture `super::super::binding::with_cleanup_runtime` call must succeed"),
                                &runtime
                            ));

                            assert_eq!(super::current_task(), Some(native_task));
                            assert_eq!(crate::context::current_task_execution_lane(), Some(lane));
                            assert!(crate::current_run_cancellation_requested());
                        }

                        let result = catch_unwind(AssertUnwindSafe(|| {
                            super::super::binding::with_cleanup_runtime(&retained, || {
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

    #[test]
    fn independent_runtime_owners_retire_without_releasing_borrowed_outer_bindings() {
        let _isolation = super::test_runtime_isolation();

        assert!(super::initialize(NativeRuntimeConfiguration::new(4, 4)).is_success());

        let outer = super::current_runtime().expect("outer runtime initialized");

        super::with_bound_runtime(Rc::clone(&outer), || {
            assert_eq!(outer.core.owners.load(Ordering::Acquire), 1);
        });

        assert_eq!(outer.core.owners.load(Ordering::Acquire), 1);

        for explicit_shutdown in [false, true] {
            let inner = super::with_independent_execution_context(|| {
                assert!(super::initialize(NativeRuntimeConfiguration::new(4, 4)).is_success());

                let inner = super::current_runtime().expect("inner runtime initialized");
                let core = std::sync::Arc::clone(&inner.core);

                if explicit_shutdown {
                    assert!(super::shutdown().is_success());
                }

                core
            });

            assert_eq!(inner.owners.load(Ordering::Acquire), 0);
            assert!(inner.workers.is_stopping());

            assert!(Rc::ptr_eq(
                &super::current_runtime().expect("fixture runtime must be active"),
                &outer
            ));

            assert_eq!(outer.core.owners.load(Ordering::Acquire), 1);
        }

        let mut unwound = None;

        let result = catch_unwind(AssertUnwindSafe(|| {
            super::with_independent_execution_context(|| {
                assert!(super::initialize(NativeRuntimeConfiguration::new(4, 4)).is_success());

                let runtime = super::current_runtime().expect("unwinding runtime initialized");

                unwound = Some(std::sync::Arc::clone(&runtime.core));
                panic!("exercise owned-runtime retirement while unwinding");
            });
        }));

        assert!(result.is_err());

        let unwound = unwound.expect("fixture `unwound` call must succeed");

        assert_eq!(unwound.owners.load(Ordering::Acquire), 0);
        assert!(unwound.workers.is_stopping());

        assert!(Rc::ptr_eq(
            &super::current_runtime().expect("fixture runtime must be active"),
            &outer
        ));

        let retained = super::with_independent_execution_context(|| {
            assert!(super::initialize(NativeRuntimeConfiguration::new(4, 4)).is_success());

            super::retain_current_runtime()
                .expect("fixture runtime must remain retained")
                .expect("successful fixture query must contain its expected result")
        });

        assert_eq!(retained.core.owners.load(Ordering::Acquire), 1);
        assert!(!retained.core.workers.is_stopping());
        retained.release();
        assert_eq!(retained.core.owners.load(Ordering::Acquire), 0);
        assert!(retained.core.workers.is_stopping());
        assert!(super::shutdown().is_success());
        assert_eq!(outer.core.owners.load(Ordering::Acquire), 0);
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

/// Borrows the native runtime whose final admission owner is retiring.
/// Only the identity is transported; the native runtime layout stays private.
#[repr(C)]
pub(crate) struct RuntimeRetirement<'a> {
    identity: u64,
    scope: std::marker::PhantomData<&'a NativeRuntimeCore>,
}

impl NativeRuntimeCore {
    pub(in crate::native) const fn scheduler(&self) -> &Scheduler {
        &self.scheduler
    }

    fn clear_tasks(&self) {
        assert_eq!(self.scheduler.task_count().expect("native scheduler is available"), 0,
            "runtime retirement follows resolution of every source-owned task and result");

        let context = RuntimeRetirement { identity: self.identity, scope: std::marker::PhantomData };
        let status = crate::report_provider::bray_runtime_task_control_runtime_retirement(&context);

        assert!(status.is_success(), "runtime retirement releases unstarted admissions");
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

    // Install explicit retirement for this runtime owner. Cleanup retains its own
    // backing and context independently of native TLS destructor order.
    RETIREMENT.with(|_| {});

    #[cfg(test)]
    let test_isolation = test_runtime_isolation();

    let Ok(thread) = RuntimeThreadScope::enter_or_reuse() else {
        return NativeRuntimeStatus::RUNTIME_FAILURE;
    };

    if main_thread_lane && !crate::bootstrap::mark_current_runtime_thread_as_main() {
        return NativeRuntimeStatus::RUNTIME_FAILURE;
    }

    let scheduler = Scheduler::new(
        capabilities,
        thread.runtime().id(),
        SchedulerLimits::new(task_capacity, timer_capacity),
    );

    let identity = crate::report_provider::bray_runtime_task_control_creation();

    if identity == 0 {
        return NativeRuntimeStatus::RUNTIME_FAILURE;
    }

    let core = Arc::new(NativeRuntimeCore {
        scheduler,
        workers: super::super::workers::WorkerPool::new(),
        owners: AtomicUsize::new(1),
        identity,
        cleanup_reports: CleanupReportSink::new(),
    });

    if !core.workers.start(&core) {
        return NativeRuntimeStatus::RUNTIME_FAILURE;
    }

    let previous = replace_runtime(Some(NativeRuntimeBinding::Owned(Rc::new(NativeRuntime {
        thread,
        main_thread_lane,
        cleanup_workloads: Cell::new(cleanup_workloads),
        worker: None,
        core,
        #[cfg(test)]
        _test_isolation: test_isolation,
    }))));

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

    if runtime.is_none() {
        return NativeRuntimeStatus::NOT_INITIALIZED;
    }

    retire_runtime(runtime);

    NativeRuntimeStatus::SUCCESS
}

pub(crate) fn retain_current_runtime() -> Result<Option<RetainedRuntime>, NativeRuntimeStatus> {
    let Some(runtime) = current_runtime() else {
        return Ok(None);
    };

    if !runtime.core.retain_owner() {
        return Err(NativeRuntimeStatus::RUNTIME_FAILURE);
    }

    Ok(Some(RetainedRuntime {
        core: Arc::clone(&runtime.core),
        main_thread: runtime
            .main_thread_lane
            .then(|| runtime.thread.runtime().id()),
        released: Arc::new(AtomicBool::new(false)),
    }))
}

pub(crate) fn retain_runtime() -> Result<RetainedRuntime, NativeRuntimeStatus> {
    if let Some(runtime) = retain_current_runtime()? {
        return Ok(runtime);
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

    let Some(NativeRuntimeBinding::Owned(runtime)) = replace_runtime(None) else {
        panic!("retaining a newly initialized runtime transfers its owner count");
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
    thread: RuntimeThreadEntry,
    workload: ExecutionWorkload,
    control: Arc<super::super::workers::WorkerControl>,
) {
    let runtime = Rc::new(NativeRuntime {
        thread,
        main_thread_lane: false,
        cleanup_workloads: Cell::new(false),
        worker: Some(Arc::clone(&control)),
        core,
        #[cfg(test)]
        _test_isolation: None,
    });

    let previous = replace_runtime(Some(NativeRuntimeBinding::Borrowed(Rc::clone(&runtime))));

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
