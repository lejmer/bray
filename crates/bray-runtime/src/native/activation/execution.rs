use bray_runtime_abi::NativeRuntimeStatus;

use crate::{ExecutionLane, ExecutionLanePlacement, ExecutionWorkload, FrameExecutionState};

use super::super::state::{current_native_task, with_runtime};

pub(in crate::native) fn execution_lane(
    execution: &FrameExecutionState,
) -> Result<ExecutionLane, NativeRuntimeStatus> {
    let task = current_native_task().ok_or(NativeRuntimeStatus::INVALID_ARGUMENT)?;

    with_runtime(|runtime| {
        runtime.with_started(task, |task| {
            task.registration
                .execution_lane(execution)
                .map_err(|_| NativeRuntimeStatus::RUNTIME_FAILURE)
        })
    })
    .and_then(|result| result)
    .and_then(|result| result)
}

pub(in crate::native) fn lane_allows(current: ExecutionLane, required: ExecutionLane) -> bool {
    if required.workload() != ExecutionWorkload::Cooperative
        && current.workload() != required.workload()
    {
        return false;
    }

    let required_placement = required.placement();

    match required_placement {
        ExecutionLanePlacement::Migratable => true,
        ExecutionLanePlacement::MainThread(required)
        | ExecutionLanePlacement::OriginThread(required)
        | ExecutionLanePlacement::PinnedWorker(required) => match current.placement() {
            ExecutionLanePlacement::MainThread(current)
            | ExecutionLanePlacement::OriginThread(current)
            | ExecutionLanePlacement::PinnedWorker(current) => current == required,
            ExecutionLanePlacement::Migratable => {
                !matches!(required_placement, ExecutionLanePlacement::MainThread(_))
                    && crate::bootstrap::current_runtime_thread()
                        .is_some_and(|thread| thread.id() == required)
            }
        },
    }
}
