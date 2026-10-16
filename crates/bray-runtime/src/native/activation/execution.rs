use bray_runtime_abi::{
    NativeActivationExecution, NativeFrameState, NativeLaneRequirements, NativeRuntimeStatus,
};
use bray_runtime_model::{
    ExecutionLaneRequirement, ProtectedAsyncFrameId, ProtectedFrameAffinity,
    ProtectedFrameStateDescriptor, ProtectedFrameStateId,
};

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

pub(in crate::native) fn frame_state(
    native: NativeFrameState,
    state: ProtectedFrameStateId,
) -> Option<ProtectedFrameStateDescriptor> {
    let affinity = ProtectedFrameAffinity::from_code(native.affinity().code())?;
    let requirements = native.lane_requirements().bits();

    let known = NativeLaneRequirements::BLOCKING.bits()
        | NativeLaneRequirements::COMPUTE.bits()
        | NativeLaneRequirements::MAIN_THREAD.bits();

    if requirements & !known != 0 {
        return None;
    }

    let lanes = ExecutionLaneRequirement::ALL
        .into_iter()
        .filter(|requirement| {
            let flag = match requirement {
                ExecutionLaneRequirement::Blocking => NativeLaneRequirements::BLOCKING,
                ExecutionLaneRequirement::Compute => NativeLaneRequirements::COMPUTE,
                ExecutionLaneRequirement::MainThread => NativeLaneRequirements::MAIN_THREAD,
            };

            requirements & flag.bits() != 0
        });

    Some(ProtectedFrameStateDescriptor::new(
        state,
        lanes,
        [],
        [],
        affinity,
    ))
}

// The Bray dispatcher supplies the checked immutable snapshot while retaining
// its exact active owner. This conversion establishes no new dispatch authority.
fn execution(snapshot: &NativeActivationExecution) -> FrameExecutionState {
    let state = frame_state(
        snapshot.requirements,
        ProtectedFrameStateId::new(snapshot.state),
    )
    .expect("Bray activation supplies checked execution requirements");

    let origin_affine = state.affinity() == ProtectedFrameAffinity::OriginThread;
    let execution = FrameExecutionState::new(ProtectedAsyncFrameId::new(snapshot.identity), state);

    if origin_affine {
        let origin = bray_platform::RuntimeThread::from_native_identity(snapshot.origin)
            .expect("origin-affine activation retains its native thread identity");

        execution.with_origin(origin.id())
    } else {
        execution
    }
}

/// Checks the selected lane before the Bray dispatcher enters a producer callback.
/// The exact live snapshot and exclusive activation owner remain with the caller.
pub(in crate::native) extern "C" fn readiness(snapshot: &NativeActivationExecution) -> u32 {
    let Some(current) = crate::context::current_task_execution_lane() else {
        return NativeRuntimeStatus::NOT_INITIALIZED.code();
    };

    let execution = execution(snapshot);

    let required = match execution_lane(&execution) {
        Ok(required) => required,
        Err(status) => return status.code(),
    };

    if lane_allows(current, required) {
        NativeRuntimeStatus::SUCCESS.code()
    } else {
        NativeRuntimeStatus::PENDING.code()
    }
}
