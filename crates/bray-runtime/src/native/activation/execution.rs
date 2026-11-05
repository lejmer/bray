use bray_runtime_abi::{
    NativeActivationExecution, NativeFrameState, NativeLaneRequirements,
};
use bray_runtime_model::{
    ExecutionLaneRequirement, ProtectedAsyncFrameId, ProtectedFrameAffinity,
    ProtectedFrameStateDescriptor, ProtectedFrameStateId,
};

use crate::FrameExecutionState;

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
pub(in crate::native) fn execution(snapshot: &NativeActivationExecution) -> FrameExecutionState {
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

// Admission already checked this snapshot against the selected lane. Installing
// it changes only the active continuation, and the context guard restores reentry.
pub(super) fn with_execution<T>(
    snapshot: &NativeActivationExecution,
    callback: impl FnOnce() -> T,
) -> T {
    let lane = crate::context::current_task_execution_lane()
        .expect("admitted activation callback retains its selected native lane");

    let execution = execution(snapshot);

    crate::context::with_task_frame_execution(&execution, lane, callback)
        .expect("admitted activation callback retains its native task context")
}
