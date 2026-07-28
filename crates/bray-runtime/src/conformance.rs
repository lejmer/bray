use std::num::NonZeroUsize;
use std::pin::pin;

use bray_platform::RuntimeThreadScope;
use bray_runtime_interface::{
    ExecutionLaneRequirement, ProtectedFrameStateId, RuntimeCapability,
};

use crate::test_support::TestFrame;
use crate::{
    CancellationContext, CleanupIncidentOrigin, CleanupIncidentProducer,
    CleanupReportSink, ExecutionLane, ExecutionLanePlacement,
    ExecutionLaneSelectionError, ExecutionWorkload, FrameContext, FrameProgress,
    RunOutcome, Scheduler, SchedulerLimits, TaskControlBlock, TaskWakeCause,
    erase_protected_frame, resume_direct,
};

trait RuntimeConformance {
    fn observe(&self) -> RuntimeConformanceReport;
}

#[derive(Debug, Eq, PartialEq)]
struct RuntimeConformanceReport {
    direct_await: i32,
    erased_direct_await: i32,
    task_result: i32,
    task_storage_stable: bool,
    panic_captured: bool,
    unobserved_result_resolved: bool,
    cancellation_shielded: bool,
    ready_order_stable: bool,
    duplicate_wake_coalesced: bool,
    wake_cause: TaskWakeCause,
    queue_timing_available: bool,
    missing_lane_capability: Option<RuntimeCapability>,
    cleanup_ordinals: Vec<u64>,
}

fn assert_runtime_conformance(runtime: &impl RuntimeConformance) {
    let report = runtime.observe();

    assert_eq!(
        report,
        RuntimeConformanceReport {
            direct_await: 11,
            erased_direct_await: 13,
            task_result: 17,
            task_storage_stable: true,
            panic_captured: true,
            unobserved_result_resolved: true,
            cancellation_shielded: true,
            ready_order_stable: true,
            duplicate_wake_coalesced: true,
            wake_cause: TaskWakeCause::Explicit,
            queue_timing_available: true,
            missing_lane_capability: Some(RuntimeCapability::BlockingLanes),
            cleanup_ordinals: vec![0, 1],
        }
    );
}

struct BrayRuntime;

impl RuntimeConformance for BrayRuntime {
    fn observe(&self) -> RuntimeConformanceReport {
        let direct_await = direct_await();
        let erased_direct_await = erased_direct_await();
        let task = task_lifecycle();
        let cancellation_shielded = cancellation_shielding();
        let scheduling = scheduling();
        let missing_lane_capability = missing_lane_capability();
        let cleanup_ordinals = cleanup_incidents();

        RuntimeConformanceReport {
            direct_await,
            erased_direct_await,
            task_result: task.result,
            task_storage_stable: task.storage_stable,
            panic_captured: task.panic_captured,
            unobserved_result_resolved: task.unobserved_result_resolved,
            cancellation_shielded,
            ready_order_stable: scheduling.ready_order_stable,
            duplicate_wake_coalesced: scheduling.duplicate_wake_coalesced,
            wake_cause: scheduling.wake_cause,
            queue_timing_available: scheduling.queue_timing_available,
            missing_lane_capability,
            cleanup_ordinals,
        }
    }
}

struct TaskLifecycle {
    result: i32,
    storage_stable: bool,
    panic_captured: bool,
    unobserved_result_resolved: bool,
}

struct Scheduling {
    ready_order_stable: bool,
    duplicate_wake_coalesced: bool,
    wake_cause: TaskWakeCause,
    queue_timing_available: bool,
}

fn direct_await() -> i32 {
    let mut frame = pin!(TestFrame::completing(11));

    let FrameProgress::Completed(result) =
        resume_direct(frame.as_mut(), FrameContext::new(false))
    else {
        panic!("direct await must complete");
    };

    result
}

fn erased_direct_await() -> i32 {
    let mut frame = erase_protected_frame(TestFrame::completing(13));

    let FrameProgress::Completed(result) =
        resume_direct(frame.as_mut(), FrameContext::new(false))
    else {
        panic!("erased direct await must complete");
    };

    result
}

fn task_lifecycle() -> TaskLifecycle {
    let task = TaskControlBlock::start(TestFrame::completing(17))
        .unwrap_or_else(|error| panic!("task must start: {error:?}"));

    let before = std::sync::Arc::as_ptr(&task);

    task.resume()
        .unwrap_or_else(|error| panic!("task must complete: {error:?}"));

    let after = std::sync::Arc::as_ptr(&task);

    let RunOutcome::Completed(result) = task
        .take_outcome()
        .unwrap_or_else(|error| panic!("task outcome must be available: {error:?}"))
    else {
        panic!("task must complete normally");
    };

    let panicking = TaskControlBlock::start(TestFrame::panicking())
        .unwrap_or_else(|error| panic!("panicking task must start: {error:?}"));

    panicking
        .resume()
        .unwrap_or_else(|error| panic!("task panic must be captured: {error:?}"));

    let panic_captured = matches!(
        panicking.take_outcome(),
        Ok(RunOutcome::Panicked(_))
    );

    let unobserved = TaskControlBlock::start(TestFrame::completing(19))
        .unwrap_or_else(|error| panic!("unobserved task must start: {error:?}"));

    unobserved
        .resume()
        .unwrap_or_else(|error| panic!("unobserved task must complete: {error:?}"));

    let mut unobserved_result_resolved = false;

    unobserved
        .resolve_unobserved(|outcome| {
            unobserved_result_resolved =
                matches!(outcome, RunOutcome::Completed(19));
        })
        .unwrap_or_else(|error| panic!("unobserved outcome must resolve: {error:?}"));

    TaskLifecycle {
        result,
        storage_stable: before == after,
        panic_captured,
        unobserved_result_resolved,
    }
}

fn cancellation_shielding() -> bool {
    let cancellation = CancellationContext::root();
    let shield = cancellation.shield();

    cancellation.request();

    let hidden_while_shielded = !cancellation.is_requested();

    drop(shield);

    hidden_while_shielded && cancellation.is_requested()
}

fn scheduling() -> Scheduling {
    let runtime = RuntimeThreadScope::enter()
        .unwrap_or_else(|error| panic!("runtime thread must initialize: {error:?}"));

    let scheduler = Scheduler::new_observed(
        [
            RuntimeCapability::CooperativeExecution,
            RuntimeCapability::MigratableLanes,
        ],
        runtime.runtime().id(),
        SchedulerLimits::new(nonzero(4), nonzero(4)),
    );

    let first = TaskControlBlock::start(TestFrame::completing(1))
        .unwrap_or_else(|error| panic!("first task must start: {error:?}"));

    let second = TaskControlBlock::start(TestFrame::completing(2))
        .unwrap_or_else(|error| panic!("second task must start: {error:?}"));

    let first_registration = scheduler
        .register_task(
            first.id(),
            first.descriptor().clone(),
            runtime.runtime().id(),
            ProtectedFrameStateId::new(0),
            first.cancellation_context(),
        )
        .unwrap_or_else(|error| panic!("first task must register: {error:?}"));

    let second_registration = scheduler
        .register_task(
            second.id(),
            second.descriptor().clone(),
            runtime.runtime().id(),
            ProtectedFrameStateId::new(0),
            second.cancellation_context(),
        )
        .unwrap_or_else(|error| panic!("second task must register: {error:?}"));

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

    let lane = cooperative_lane();

    let first_ready = scheduler
        .take_ready(lane)
        .unwrap_or_else(|error| panic!("first dispatch must succeed: {error:?}"))
        .unwrap_or_else(|| panic!("first task must be ready"));

    let second_ready = scheduler
        .take_ready(lane)
        .unwrap_or_else(|error| panic!("second dispatch must succeed: {error:?}"))
        .unwrap_or_else(|| panic!("second task must be ready"));

    Scheduling {
        ready_order_stable: first_ready.task() == first.id()
            && second_ready.task() == second.id(),
        duplicate_wake_coalesced: first_queued && duplicate_coalesced,
        wake_cause: first_ready.wake_cause(),
        queue_timing_available: first_ready.queue_latency().is_some()
            && second_ready.queue_latency().is_some(),
    }
}

fn missing_lane_capability() -> Option<RuntimeCapability> {
    let runtime = RuntimeThreadScope::enter()
        .unwrap_or_else(|error| panic!("runtime thread must initialize: {error:?}"));

    let scheduler = Scheduler::new(
        [RuntimeCapability::CooperativeExecution],
        runtime.runtime().id(),
        SchedulerLimits::new(nonzero(1), nonzero(1)),
    );

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
        Some(crate::SchedulerError::LaneSelection(
            ExecutionLaneSelectionError::MissingCapability(capability),
        )) => Some(capability),
        _ => None,
    }
}

fn cleanup_incidents() -> Vec<u64> {
    let reports = CleanupReportSink::new();

    let origin = CleanupIncidentOrigin::new(
        bray_runtime_interface::ProtectedAsyncFrameId::new([3; 32]),
        ProtectedFrameStateId::new(0),
    );

    reports.transfer(
        CleanupIncidentProducer::SynchronousRoot,
        origin,
        "first",
    );

    reports.transfer(
        CleanupIncidentProducer::SynchronousRoot,
        origin,
        "second",
    );

    let mut ordinals = Vec::new();

    reports.drain(|incident| ordinals.push(incident.ordinal()));

    ordinals
}

fn cooperative_lane() -> ExecutionLane {
    ExecutionLane::new(
        ExecutionLanePlacement::Migratable,
        ExecutionWorkload::Cooperative,
    )
}

fn nonzero(value: usize) -> NonZeroUsize {
    NonZeroUsize::new(value)
        .unwrap_or_else(|| panic!("test scheduler capacity must be nonzero"))
}

#[test]
fn target_independent_runtime_satisfies_the_shared_conformance_suite() {
    assert_runtime_conformance(&BrayRuntime);
}
