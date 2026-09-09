use std::cell::{Cell, RefCell};
use std::num::NonZeroUsize;
use std::pin::pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use bray_platform::{PlatformError, PlatformErrorKind, PlatformOperation, RuntimeThreadScope};
use bray_runtime_model::{ExecutionLaneRequirement, ProtectedFrameStateId, RuntimeCapability};

use crate::context::with_task_execution_context;
use crate::test_support::{TestFrame, register_task};
use crate::{
    CancellationContext, CancellationObservation, CleanupIncidentOrigin, CleanupIncidentProducer,
    CleanupReportSink, ExecutionLane, ExecutionLanePlacement, ExecutionLaneSelectionError,
    ExecutionWorkload, FrameContext, FrameProgress, RunOutcome, RunOutcomeKind, Scheduler,
    SchedulerError, SchedulerLimits, TaskControlBlock, TaskExecutionContext, TaskStartSite,
    TaskWakeCause, erase_protected_frame, finish_product_shutdown, resume_direct,
};

trait RuntimeConformance {
    fn reset_costs(&self);
    fn costs(&self) -> RuntimeCosts;
    fn direct_await(&self, value: i32) -> i32;
    fn erased_direct_await(&self, value: i32) -> i32;
    fn run_started_task(&self, value: i32) -> TaskRun;
    fn capture_task_panic(&self) -> bool;
    fn resolve_unobserved_result(&self, value: i32) -> bool;
    fn observe_cancellation_shield(&self) -> ShieldObservation;
    fn observe_shielded_task(&self) -> ShieldedTaskObservation;
    fn schedule_ready_tasks(&self) -> SchedulingObservation;
    fn observe_join_cancellation(&self) -> JoinCancellationObservation;
    fn observe_nested_task(&self) -> Option<TaskStartSite>;
    fn reject_missing_lane_capability(&self) -> Option<RuntimeCapability>;
    fn enforce_task_limit(&self) -> bool;
    fn run_structured_shutdown(&self) -> ShutdownObservation;
    fn translate_platform_failure(&self) -> Option<(PlatformOperation, PlatformErrorKind)>;
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct RuntimeCosts {
    task_starts: usize,
    scheduler_initializations: usize,
    scheduler_registrations: usize,
    scheduler_dispatches: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TaskRun {
    result: i32,
    storage_stable: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ShieldObservation {
    before_request: CancellationObservation,
    while_shielded: CancellationObservation,
    after_shield: CancellationObservation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ShieldedTaskObservation {
    task: CancellationObservation,
    scheduler: CancellationObservation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SchedulingObservation {
    ready_order_stable: bool,
    duplicate_wake_coalesced: bool,
    wake_cause: TaskWakeCause,
    queue_timing_available: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct JoinCancellationObservation {
    outcome: RunOutcomeKind,
    waiter_wakes: usize,
}

#[derive(Debug, Eq, PartialEq)]
struct ShutdownObservation {
    exit_status: i32,
    cleanup_ordinals: Vec<u64>,
    infrastructure_stopped_last: bool,
}

fn assert_runtime_conformance(runtime: &impl RuntimeConformance) {
    runtime.reset_costs();

    assert_eq!(runtime.direct_await(11), 11);
    assert_eq!(runtime.erased_direct_await(13), 13);
    assert_eq!(runtime.costs(), RuntimeCosts::default());

    runtime.reset_costs();

    assert_eq!(
        runtime.run_started_task(17),
        TaskRun {
            result: 17,
            storage_stable: true,
        }
    );

    assert_eq!(
        runtime.costs(),
        RuntimeCosts {
            task_starts: 1,
            ..RuntimeCosts::default()
        }
    );

    assert!(runtime.capture_task_panic());
    assert!(runtime.resolve_unobserved_result(19));

    assert_eq!(
        runtime.observe_cancellation_shield(),
        ShieldObservation {
            before_request: CancellationObservation::new(false, false),
            while_shielded: CancellationObservation::new(true, false),
            after_shield: CancellationObservation::new(true, true),
        }
    );

    assert_eq!(
        runtime.observe_shielded_task(),
        ShieldedTaskObservation {
            task: CancellationObservation::new(true, false),
            scheduler: CancellationObservation::new(true, false),
        }
    );

    runtime.reset_costs();

    assert_eq!(
        runtime.schedule_ready_tasks(),
        SchedulingObservation {
            ready_order_stable: true,
            duplicate_wake_coalesced: true,
            wake_cause: TaskWakeCause::Explicit,
            queue_timing_available: true,
        }
    );

    assert_eq!(
        runtime.costs(),
        RuntimeCosts {
            task_starts: 2,
            scheduler_initializations: 1,
            scheduler_registrations: 2,
            scheduler_dispatches: 2,
        }
    );

    assert_eq!(
        runtime.observe_join_cancellation(),
        JoinCancellationObservation {
            outcome: RunOutcomeKind::Cancelled,
            waiter_wakes: 1,
        }
    );

    assert!(runtime.observe_nested_task().is_some());

    assert_eq!(
        runtime.reject_missing_lane_capability(),
        Some(RuntimeCapability::BlockingLanes)
    );

    assert!(runtime.enforce_task_limit());

    assert_eq!(
        runtime.run_structured_shutdown(),
        ShutdownObservation {
            exit_status: 7,
            cleanup_ordinals: vec![0, 1],
            infrastructure_stopped_last: true,
        }
    );

    assert_eq!(
        runtime.translate_platform_failure(),
        Some((PlatformOperation::Event, PlatformErrorKind::Unsupported,))
    );
}

#[derive(Default)]
struct BrayRuntime {
    costs: Cell<RuntimeCosts>,
}

impl RuntimeConformance for BrayRuntime {
    fn reset_costs(&self) {
        self.costs.set(RuntimeCosts::default());
    }

    fn costs(&self) -> RuntimeCosts {
        self.costs.get()
    }

    fn direct_await(&self, value: i32) -> i32 {
        let mut frame = pin!(TestFrame::completing(value));

        let FrameProgress::Completed(result) =
            resume_direct(frame.as_mut(), FrameContext::new(false))
        else {
            panic!("direct await must complete");
        };

        result
    }

    fn erased_direct_await(&self, value: i32) -> i32 {
        let mut frame = erase_protected_frame(TestFrame::completing(value)).unwrap();

        let FrameProgress::Completed(result) =
            resume_direct(frame.as_mut(), FrameContext::new(false))
        else {
            panic!("erased direct await must complete");
        };

        result
    }

    fn run_started_task(&self, value: i32) -> TaskRun {
        self.record_cost(|costs| costs.task_starts += 1);

        let task = TaskControlBlock::start(TestFrame::completing(value))
            .unwrap_or_else(|error| panic!("task must start: {error:?}"));

        let before = triomphe::Arc::as_ptr(&task);

        task.resume()
            .unwrap_or_else(|error| panic!("task must complete: {error:?}"));

        let after = triomphe::Arc::as_ptr(&task);

        let RunOutcome::Completed(result) = task
            .take_outcome()
            .unwrap_or_else(|error| panic!("task outcome must be available: {error:?}"))
        else {
            panic!("task must complete normally");
        };

        TaskRun {
            result,
            storage_stable: before == after,
        }
    }

    fn capture_task_panic(&self) -> bool {
        let task = TaskControlBlock::start(TestFrame::panicking())
            .unwrap_or_else(|error| panic!("panicking task must start: {error:?}"));

        task.resume()
            .unwrap_or_else(|error| panic!("task panic must be captured: {error:?}"));

        matches!(task.take_outcome(), Ok(RunOutcome::Panicked(_)))
    }

    fn resolve_unobserved_result(&self, value: i32) -> bool {
        let task = TaskControlBlock::start(TestFrame::completing(value))
            .unwrap_or_else(|error| panic!("unobserved task must start: {error:?}"));

        task.resume()
            .unwrap_or_else(|error| panic!("unobserved task must complete: {error:?}"));

        let resolved = Cell::new(None);

        task.resolve_unobserved(|outcome| {
            let RunOutcome::Completed(value) = outcome else {
                panic!("unobserved task must complete normally");
            };

            resolved.set(Some(value));
        })
        .unwrap_or_else(|error| panic!("unobserved outcome must resolve: {error:?}"));

        resolved.get() == Some(value)
    }

    fn observe_cancellation_shield(&self) -> ShieldObservation {
        let cancellation = CancellationContext::root().unwrap();
        let before_request = cancellation.observation();
        let shield = cancellation.shield();

        cancellation.request();

        let while_shielded = cancellation.observation();

        drop(shield);

        ShieldObservation {
            before_request,
            while_shielded,
            after_shield: cancellation.observation(),
        }
    }

    fn observe_shielded_task(&self) -> ShieldedTaskObservation {
        let runtime = runtime_thread();
        let scheduler = scheduler(runtime.runtime().id(), 2);

        let task = TaskControlBlock::start(TestFrame::cancellation_aware())
            .unwrap_or_else(|error| panic!("task must start: {error:?}"));

        let _registration = register_task(&scheduler, &task, runtime.runtime().id());

        let shield = task.cancellation_context().shield();

        task.request_cancellation();

        let task_snapshot = task
            .snapshot()
            .unwrap_or_else(|error| panic!("task snapshot must succeed: {error:?}"));

        let scheduler_snapshot = scheduler
            .snapshot()
            .unwrap_or_else(|error| panic!("scheduler snapshot must succeed: {error:?}"));

        let [scheduled] = scheduler_snapshot.tasks() else {
            panic!("scheduler snapshot must contain one task");
        };

        let observation = ShieldedTaskObservation {
            task: task_snapshot.cancellation(),
            scheduler: scheduled.cancellation(),
        };

        drop(shield);

        observation
    }

    fn schedule_ready_tasks(&self) -> SchedulingObservation {
        self.record_cost(|costs| costs.scheduler_initializations += 1);

        let runtime = runtime_thread();

        let scheduler = Scheduler::new_observed(
            [
                RuntimeCapability::CooperativeExecution,
                RuntimeCapability::MigratableLanes,
            ],
            runtime.runtime().id(),
            SchedulerLimits::new(nonzero(4), nonzero(4)),
        );

        self.record_cost(|costs| costs.task_starts += 2);

        let first = TaskControlBlock::start(TestFrame::completing(1))
            .unwrap_or_else(|error| panic!("first task must start: {error:?}"));

        let second = TaskControlBlock::start(TestFrame::completing(2))
            .unwrap_or_else(|error| panic!("second task must start: {error:?}"));

        self.record_cost(|costs| costs.scheduler_registrations += 1);

        let first_registration = register_task(&scheduler, &first, runtime.runtime().id());

        self.record_cost(|costs| costs.scheduler_registrations += 1);

        let second_registration = register_task(&scheduler, &second, runtime.runtime().id());

        let first_wake = first_registration.wake_handle();

        let first_queued = first_wake
            .wake(ProtectedFrameStateId::new(0))
            .unwrap_or_else(|error| panic!("first task must wake: {error:?}"));

        let duplicate_coalesced = !first_wake
            .wake(ProtectedFrameStateId::new(0))
            .unwrap_or_else(|error| panic!("duplicate wake must coalesce: {error:?}"));

        second_registration
            .wake_handle()
            .wake(ProtectedFrameStateId::new(0))
            .unwrap_or_else(|error| panic!("second task must wake: {error:?}"));

        self.record_cost(|costs| costs.scheduler_dispatches += 1);

        let first_ready = scheduler
            .take_ready(cooperative_lane())
            .unwrap_or_else(|error| panic!("first dispatch must succeed: {error:?}"))
            .unwrap_or_else(|| panic!("first task must be ready"));

        self.record_cost(|costs| costs.scheduler_dispatches += 1);

        let second_ready = scheduler
            .take_ready(cooperative_lane())
            .unwrap_or_else(|error| panic!("second dispatch must succeed: {error:?}"))
            .unwrap_or_else(|| panic!("second task must be ready"));

        SchedulingObservation {
            ready_order_stable: first_ready.task() == first.id()
                && second_ready.task() == second.id(),
            duplicate_wake_coalesced: first_queued && duplicate_coalesced,
            wake_cause: first_ready.wake_cause(),
            queue_timing_available: first_ready.queue_latency().is_some()
                && second_ready.queue_latency().is_some(),
        }
    }

    fn observe_join_cancellation(&self) -> JoinCancellationObservation {
        let task = TaskControlBlock::start(TestFrame::cancellation_aware())
            .unwrap_or_else(|error| panic!("task must start: {error:?}"));

        let waiter_wakes = Arc::new(AtomicUsize::new(0));
        let observed_wakes = Arc::clone(&waiter_wakes);

        let _registration = task
            .register_join_waiter(Arc::new(move || {
                observed_wakes.fetch_add(1, Ordering::Relaxed);
            }))
            .unwrap_or_else(|error| panic!("join waiter must register: {error:?}"));

        task.request_cancellation();

        task.resume()
            .unwrap_or_else(|error| panic!("cancelled task must terminate: {error:?}"));

        JoinCancellationObservation {
            outcome: task
                .take_outcome()
                .unwrap_or_else(|error| panic!("cancelled outcome must exist: {error:?}"))
                .kind(),
            waiter_wakes: waiter_wakes.load(Ordering::Relaxed),
        }
    }

    fn observe_nested_task(&self) -> Option<TaskStartSite> {
        let runtime = runtime_thread();
        let scheduler = scheduler(runtime.runtime().id(), 2);

        let parent = TaskControlBlock::start(TestFrame::completing(1))
            .unwrap_or_else(|error| panic!("parent task must start: {error:?}"));

        let registration = register_task(&scheduler, &parent, runtime.runtime().id());

        let context = TaskExecutionContext::new(
            parent.id(),
            ProtectedFrameStateId::new(0),
            parent.cancellation_context().clone(),
            parent.output_context().clone(),
            cooperative_lane(),
            registration.wake_handle(),
        );

        let child = with_task_execution_context(context, || {
            TaskControlBlock::start(TestFrame::completing(2))
        })
        .unwrap_or_else(|error| panic!("child task must start: {error:?}"));

        child
            .snapshot()
            .unwrap_or_else(|error| panic!("child snapshot must succeed: {error:?}"))
            .start_site()
    }

    fn reject_missing_lane_capability(&self) -> Option<RuntimeCapability> {
        let runtime = runtime_thread();
        let scheduler = scheduler(runtime.runtime().id(), 1);

        let task = TaskControlBlock::start(TestFrame::requiring(
            [ExecutionLaneRequirement::Blocking],
            1,
        ))
        .unwrap_or_else(|error| panic!("blocking task must start: {error:?}"));

        let error = scheduler
            .register_task(
                task.id(),
                task.descriptor().clone(),
                runtime.runtime().id(),
                ProtectedFrameStateId::new(0),
                task.cancellation_context(),
            )
            .err();

        match error {
            Some(SchedulerError::LaneSelection(
                ExecutionLaneSelectionError::MissingCapability(capability),
            )) => Some(capability),
            _ => None,
        }
    }

    fn enforce_task_limit(&self) -> bool {
        let runtime = runtime_thread();
        let scheduler = scheduler(runtime.runtime().id(), 1);

        let first = TaskControlBlock::start(TestFrame::completing(1))
            .unwrap_or_else(|error| panic!("first task must start: {error:?}"));

        let second = TaskControlBlock::start(TestFrame::completing(2))
            .unwrap_or_else(|error| panic!("second task must start: {error:?}"));

        let _first_registration = register_task(&scheduler, &first, runtime.runtime().id());

        matches!(
            scheduler.register_task(
                second.id(),
                second.descriptor().clone(),
                runtime.runtime().id(),
                ProtectedFrameStateId::new(0),
                second.cancellation_context(),
            ),
            Err(SchedulerError::TaskCapacityReached)
        )
    }

    fn run_structured_shutdown(&self) -> ShutdownObservation {
        let reports = CleanupReportSink::new();

        let origin = CleanupIncidentOrigin::new(
            bray_runtime_model::ProtectedAsyncFrameId::new([3; 32]),
            ProtectedFrameStateId::new(0),
        );

        reports.transfer(CleanupIncidentProducer::SynchronousRoot, origin, "first");

        reports.transfer(CleanupIncidentProducer::SynchronousRoot, origin, "second");

        let cleanup_ordinals = RefCell::new(Vec::new());
        let events = RefCell::new(Vec::new());

        let exit_status = finish_product_shutdown(
            RunOutcome::Completed(7),
            &reports,
            |outcome| {
                events.borrow_mut().push("map");

                let RunOutcome::Completed(status) = outcome else {
                    panic!("root must complete");
                };

                status
            },
            |incident| {
                cleanup_ordinals.borrow_mut().push(incident.ordinal());
                events.borrow_mut().push("cleanup");
            },
            || {
                events.borrow_mut().push("stop");

                Ok::<(), ()>(())
            },
        )
        .unwrap_or_else(|()| panic!("runtime shutdown must succeed"));

        let infrastructure_stopped_last = events.borrow().last() == Some(&"stop");

        ShutdownObservation {
            exit_status,
            cleanup_ordinals: cleanup_ordinals.into_inner(),
            infrastructure_stopped_last,
        }
    }

    fn translate_platform_failure(&self) -> Option<(PlatformOperation, PlatformErrorKind)> {
        let error = SchedulerError::from(PlatformError::new(
            PlatformOperation::Event,
            PlatformErrorKind::Unsupported,
        ));

        let SchedulerError::Platform(error) = error else {
            return None;
        };

        Some((error.operation(), error.kind()))
    }
}

impl BrayRuntime {
    fn record_cost(&self, update: impl FnOnce(&mut RuntimeCosts)) {
        let mut costs = self.costs.get();

        update(&mut costs);

        self.costs.set(costs);
    }
}

fn runtime_thread() -> RuntimeThreadScope {
    RuntimeThreadScope::enter()
        .unwrap_or_else(|error| panic!("runtime thread must initialize: {error:?}"))
}

fn scheduler(main_thread: bray_platform::RuntimeThreadId, task_limit: usize) -> Scheduler {
    Scheduler::new(
        [RuntimeCapability::CooperativeExecution],
        main_thread,
        SchedulerLimits::new(nonzero(task_limit), nonzero(4)),
    )
}

fn cooperative_lane() -> ExecutionLane {
    ExecutionLane::new(
        ExecutionLanePlacement::Migratable,
        ExecutionWorkload::Cooperative,
    )
}

fn nonzero(value: usize) -> NonZeroUsize {
    NonZeroUsize::new(value).unwrap_or_else(|| panic!("test scheduler capacity must be nonzero"))
}

#[test]
fn target_independent_runtime_satisfies_the_shared_conformance_suite() {
    assert_runtime_conformance(&BrayRuntime::default());
}
