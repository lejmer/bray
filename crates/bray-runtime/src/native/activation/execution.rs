use bray_runtime_abi::{
    NativeActivationExecution, NativeFrameState, NativeLaneRequirements, NativeRuntimeStatus,
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

/// Checks the selected lane before the Bray dispatcher enters a producer callback.
/// The exact live snapshot and exclusive activation owner remain with the caller.
pub(in crate::native) extern "C" fn readiness(snapshot: &NativeActivationExecution) -> u32 {
    bray_runtime_activation_action_readiness(snapshot)
}

/// Checks a synchronous action without replacing the enclosing run's continuation.
pub extern "C" fn bray_runtime_activation_action_readiness(
    snapshot: &NativeActivationExecution,
) -> u32 {
    let current = crate::context::current_task_execution_lane()
        .map(crate::scheduler::native::to_native)
        .unwrap_or_default();

    let required = bray_runtime_abi::NativeSchedulerRequirements {
        affinity: snapshot.requirements.affinity().code(),
        workloads: snapshot.requirements.lane_requirements().bits(),
        origin: snapshot.origin,
    };

    let thread = if required.affinity != 0
        || required.workloads & NativeLaneRequirements::MAIN_THREAD.bits() != 0
    {
        match super::super::state::current_runtime() {
            Some(runtime) => runtime.thread.runtime().id().raw(),
            // Synchronous foreign entry has an attachment without a scheduler.
            None => {
                crate::bootstrap::current_runtime_thread().map_or(0, |thread| thread.id().raw())
            }
        }
    } else {
        0
    };

    if crate::scheduler::native::bray_runtime_scheduler_compatibility(&current, &required, thread)
        != 0
    {
        NativeRuntimeStatus::SUCCESS.code()
    } else {
        NativeRuntimeStatus::PENDING.code()
    }
}

// Admission checks the selected runtime before the inactive child transfers.
// The current dispatch already retains its native services; no registry lookup
// or runtime-derived replacement of the checked affinity is needed.
pub(in crate::native) extern "C" fn admission(snapshot: &NativeActivationExecution) -> u32 {
    let Some(task) = super::super::state::current_services() else {
        return NativeRuntimeStatus::INVALID_ARGUMENT.code();
    };

    task.registration
        .execution_lane(&execution(snapshot))
        .map_or_else(
            |_| NativeRuntimeStatus::RUNTIME_FAILURE.code(),
            |_| NativeRuntimeStatus::SUCCESS.code(),
        )
}

#[cfg(test)]
mod tests {
    use bray_runtime_abi::{
        NativeActivationExecution, NativeFrameAffinity, NativeFrameState, NativeLaneRequirements,
        NativeRuntimeStatus,
    };

    #[test]
    fn synchronous_entry_does_not_grant_unestablished_execution_capabilities() {
        for (affinity, requirements, expected) in [
            (
                NativeFrameAffinity::MOVABLE,
                NativeLaneRequirements::NONE,
                NativeRuntimeStatus::SUCCESS,
            ),
            (
                NativeFrameAffinity::MOVABLE,
                NativeLaneRequirements::BLOCKING,
                NativeRuntimeStatus::PENDING,
            ),
            (
                NativeFrameAffinity::MOVABLE,
                NativeLaneRequirements::COMPUTE,
                NativeRuntimeStatus::PENDING,
            ),
            (
                NativeFrameAffinity::MOVABLE,
                NativeLaneRequirements::MAIN_THREAD,
                NativeRuntimeStatus::PENDING,
            ),
            (
                NativeFrameAffinity::MAIN_THREAD,
                NativeLaneRequirements::NONE,
                NativeRuntimeStatus::PENDING,
            ),
            (
                NativeFrameAffinity::ORIGIN_THREAD,
                NativeLaneRequirements::NONE,
                NativeRuntimeStatus::PENDING,
            ),
        ] {
            let snapshot = NativeActivationExecution {
                requirements: NativeFrameState::new(affinity, requirements),
                ..NativeActivationExecution::default()
            };

            assert_eq!(
                super::bray_runtime_activation_action_readiness(&snapshot),
                expected.code()
            );
        }
    }
}
