use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use crate::bootstrap::RuntimeThreadScope;
use bray_platform::RuntimeThreadId;
use bray_runtime_abi::{
    NativeExecutionLane, NativeExecutionLaneResult, NativeRunOutcome, NativeRunState,
    NativeRuntimeStatus,
};

use crate::{ExecutionLane, ExecutionLanePlacement, ExecutionWorkload};

use super::core::{NativeRuntime, RetainedRuntime, current_runtime, with_bound_runtime};

pub(in crate::native) fn current_thread_lanes(
    thread: RuntimeThreadId,
    main_thread_lane: bool,
    cleanup_workloads: bool,
) -> impl Iterator<Item = ExecutionLane> {
    let placements = [
        main_thread_lane.then_some(ExecutionLanePlacement::MainThread(thread)),
        Some(ExecutionLanePlacement::OriginThread(thread)),
        Some(ExecutionLanePlacement::PinnedWorker(thread)),
        cleanup_workloads.then_some(ExecutionLanePlacement::Migratable),
    ];

    let workloads = [
        ExecutionWorkload::Cooperative,
        ExecutionWorkload::Blocking,
        ExecutionWorkload::Compute,
        ExecutionWorkload::BlockingCompute,
    ];

    placements.into_iter().flatten().flat_map(move |placement| {
        workloads
            .into_iter()
            .map(move |workload| ExecutionLane::new(placement, workload))
    })
}

pub(in crate::native) fn with_cleanup_runtime<T>(
    retained: &RetainedRuntime,
    callback: impl FnOnce() -> T,
) -> Result<(T, NativeRuntimeStatus), NativeRuntimeStatus> {
    // A retiring attachment already owns this callback. Its native TLS slot is clear and
    // its closing row rejects nested entry, so it cannot install or need another bridge.
    let bridge = (!crate::bootstrap::in_thread_cleanup())
        .then(super::super::bridge::Scope::enter)
        .transpose()?;

    let current = current_runtime();

    let result = if let Some(runtime) = current.as_ref()
        && Arc::ptr_eq(&runtime.core, &retained.core)
    {
        with_bound_runtime(Rc::clone(runtime), || {
            runtime.with_cleanup_driving(callback)
        })
    } else {
        let thread = RuntimeThreadScope::enter_or_reuse()
            .map_err(|_| NativeRuntimeStatus::RUNTIME_FAILURE)?;

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

        with_bound_runtime(runtime, callback)
    };

    let status = if bridge.map_or(0, super::super::bridge::Scope::finish) == 0 {
        NativeRuntimeStatus::SUCCESS
    } else {
        NativeRuntimeStatus::RUNTIME_FAILURE
    };

    Ok((result, status))
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

pub(in crate::native) fn lane_result(lane: ExecutionLane) -> NativeExecutionLaneResult {
    let lane = match (lane.placement(), lane.workload()) {
        (_, ExecutionWorkload::Blocking) => NativeExecutionLane::BLOCKING,
        (_, ExecutionWorkload::Compute) => NativeExecutionLane::COMPUTE,
        (_, ExecutionWorkload::BlockingCompute) => NativeExecutionLane::BLOCKING_COMPUTE,
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
