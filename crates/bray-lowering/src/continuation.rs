use bray_ir::{
    MirAsyncOperation, MirBlockId, MirCapacityError, MirCleanupEdge, MirEdge, MirFrameReference,
    MirFrameState, MirOperand, MirOperationKind, MirRuntimeReference, MirSourceAnchor,
    MirStorageKind, MirSuspensionKind, MirTerminatorKind, MirUnitBuilder, MirValueId,
};
use bray_runtime_interface::{ProtectedAsyncFrameId, RuntimeAbiRole};
use bray_symbols::TypeId;

/// Composes a child into its existing run and moves its result after resumption.
/// The caller supplies established retention and cancellation state, and transfers
/// any source initialization guard before handing over the future operand.
#[expect(
    clippy::too_many_arguments,
    reason = "composition retains the enclosing frame, checked state and exact child completion"
)]
pub(crate) fn await_frame(
    builder: &mut MirUnitBuilder,
    current: MirBlockId,
    source: &MirSourceAnchor,
    parent: ProtectedAsyncFrameId,
    frame: MirOperand,
    completion: TypeId,
    state: &MirFrameState,
    cancellation: MirCleanupEdge,
) -> Result<MirValueId, MirCapacityError> {
    // Emitted nodes independently retain the same immutable source provenance.
    let destination =
        builder.push_storage(source.clone(), MirStorageKind::Temporary, completion)?;

    let child = MirFrameReference::Erased;
    let abi = builder.target().runtime_abi();

    builder.push_operation(
        current,
        source.clone(),
        MirOperationKind::Async(MirAsyncOperation::ComposeAwaitedFrame {
            parent,
            child,
            resume_state: state.state(),
            frame,
            completion: destination,
            abandoning: false,
        }),
        None,
    )?;

    builder.set_terminator(
        current,
        source.clone(),
        MirTerminatorKind::Suspend {
            kind: MirSuspensionKind::Awaited,
            payload: None,
            resume_state: state.state(),
            resume: MirEdge::new(state.entry(), []),
            cancellation,
            registration: MirRuntimeReference::new(RuntimeAbiRole::SuspensionRegistration, abi),
            wake: MirRuntimeReference::new(RuntimeAbiRole::Wake, abi),
        },
    );

    let value = builder.push_operation(
        state.entry(),
        source.clone(),
        MirOperationKind::Async(MirAsyncOperation::CommitAwaitedCompletion {
            child,
            completion: destination,
        }),
        Some(completion),
    )?;

    Ok(value
        .result()
        .expect("awaited completion publishes its checked result"))
}
