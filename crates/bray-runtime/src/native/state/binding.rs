use std::cell::Cell;
use std::io::Write;
use std::rc::Rc;
use std::sync::Arc;

use bray_platform::{RuntimeThreadId, RuntimeThreadScope};
use bray_runtime_abi::{
    NativeExecutionLane, NativeExecutionLaneResult, NativeRunOutcome, NativeRunState,
    NativeRuntimeStatus, NativeTaskHandle,
};

use crate::{
    CleanupIncidentProducer, ExecutionLane, ExecutionLanePlacement, ExecutionWorkload, RunOutcome,
};

use super::core::{
    NativeRuntime, RetainedRuntime, current_runtime, current_task, retain_runtime,
    with_bound_runtime,
};

pub(in crate::native) fn current_thread_lanes(
    thread: RuntimeThreadId,
    main_thread_lane: bool,
    cleanup_workloads: bool,
) -> Vec<ExecutionLane> {
    let mut placements = Vec::with_capacity(4);

    if main_thread_lane {
        placements.push(ExecutionLanePlacement::MainThread(thread));
    }

    placements.extend([
        ExecutionLanePlacement::OriginThread(thread),
        ExecutionLanePlacement::PinnedWorker(thread),
    ]);

    if cleanup_workloads {
        placements.push(ExecutionLanePlacement::Migratable);
    }

    let workloads = [
        ExecutionWorkload::Cooperative,
        ExecutionWorkload::Blocking,
        ExecutionWorkload::Compute,
    ];

    placements
        .into_iter()
        .flat_map(|placement| {
            workloads
                .into_iter()
                .map(move |workload| ExecutionLane::new(placement, workload))
        })
        .collect()
}

pub(in crate::native) fn with_cleanup_runtime<T>(
    retained: Option<&RetainedRuntime>,
    callback: impl FnOnce() -> T,
) -> Result<(T, NativeRuntimeStatus), NativeRuntimeStatus> {
    let owned = retained.is_none().then(retain_runtime).transpose()?;
    let _owned = owned.as_ref().map(OwnedRuntimeScope);

    let retained = retained
        .or(owned.as_ref())
        .ok_or(NativeRuntimeStatus::RUNTIME_FAILURE)?;

    let result = with_retained_runtime(retained, callback)?;

    Ok((result, NativeRuntimeStatus::SUCCESS))
}

fn with_retained_runtime<T>(
    retained: &RetainedRuntime,
    callback: impl FnOnce() -> T,
) -> Result<T, NativeRuntimeStatus> {
    let current = current_runtime();

    if let Some(runtime) = current.as_ref()
        && Arc::ptr_eq(&runtime.core, &retained.core)
    {
        return Ok(with_bound_runtime(Rc::clone(runtime), || {
            runtime.with_cleanup_driving(callback)
        }));
    }

    let thread =
        RuntimeThreadScope::enter_or_reuse().map_err(|_| NativeRuntimeStatus::RUNTIME_FAILURE)?;

    let main_thread_lane = retained.main_thread == Some(thread.runtime().id());

    let runtime = Rc::new(NativeRuntime {
        thread,
        main_thread_lane,
        cleanup_workloads: Cell::new(true),
        worker: None,
        core: Arc::clone(&retained.core),
        #[cfg(test)]
        _test_isolation: None,
    });

    Ok(with_bound_runtime(runtime, callback))
}

pub(in crate::native) struct CleanupWorkloadScope<'a> {
    pub(in crate::native) runtime: &'a NativeRuntime,
    pub(in crate::native) previous: bool,
}

impl Drop for CleanupWorkloadScope<'_> {
    fn drop(&mut self) {
        self.runtime.cleanup_workloads.set(self.previous);
    }
}

struct OwnedRuntimeScope<'a>(&'a RetainedRuntime);

impl Drop for OwnedRuntimeScope<'_> {
    fn drop(&mut self) {
        self.0.release();
    }
}

pub(in crate::native) fn current_native_task() -> Option<NativeTaskHandle> {
    current_task()
}

pub(in crate::native) fn write_cleanup_incident_report(
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

pub(in crate::native) fn task_outcome(
    outcome: RunOutcome<usize>,
    task: &crate::TaskControlBlock<usize>,
) -> NativeRunOutcome {
    match outcome {
        RunOutcome::Completed(payload) => NativeRunOutcome::new(NativeRunState::COMPLETED, payload),
        RunOutcome::Cancelled => NativeRunOutcome::new(NativeRunState::CANCELLED, 0),
        RunOutcome::Panicked(panic) => NativeRunOutcome::panicked(task.native_panic(panic)),
    }
}

pub(in crate::native) fn lane_result(lane: ExecutionLane) -> NativeExecutionLaneResult {
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

pub(in crate::native) fn runtime_failure(status: NativeRuntimeStatus) -> NativeRunOutcome {
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
            bray_runtime_model::ProtectedAsyncFrameId::new([5; 32]),
            bray_runtime_model::ProtectedFrameStateId::new(7),
        );

        reports.transfer(
            CleanupIncidentProducer::SynchronousRoot,
            origin,
            crate::RuntimePanic::new("cleanup failed"),
            &mut crate::outgoing::OutgoingRecords::admit(1).unwrap(),
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
