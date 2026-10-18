use bray_ir::{
    MirAsyncOperation, MirBlockId, MirBlockKind, MirCallFailureEdge, MirCapacityError,
    MirCleanupEdge, MirCleanupPhase, MirEdge, MirFrameReference, MirFrameState, MirOperand,
    MirOperationKind, MirPlace, MirRuntimeReference, MirSourceAnchor, MirStorageKind,
    MirSuspensionKind, MirTerminatorKind, MirUnitBuilder, MirValueId,
};
use bray_runtime_interface::{ProtectedAsyncFrameId, RuntimeAbiRole};
use bray_symbols::TypeId;

/// Composes a child into its existing run and consumes its outcome after resumption.
/// A missing completion type abandons inactive captures without producing a result.
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
    completion: Option<TypeId>,
    state: &MirFrameState,
    cancellation: MirCleanupEdge,
) -> Result<Option<MirValueId>, MirCapacityError> {
    // Emitted nodes independently retain the same immutable source provenance.
    let destination = completion
        .map(|ty| builder.push_storage(source.clone(), MirStorageKind::Temporary, ty))
        .transpose()?;

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
        completion,
    )?;

    Ok(value.result())
}

/// Routes a cleanup call to its immediate parent's handlers without leaving the broadcast phase implicitly.
pub(crate) fn check_call_outcome(
    builder: &mut MirUnitBuilder,
    block: MirBlockId,
    source: &MirSourceAnchor,
    panicked: MirBlockId,
    cancelled: MirBlockId,
    report: MirPlace,
    value: Option<MirOperand>,
) -> Result<(MirBlockId, Option<MirOperand>), MirCapacityError> {
    let kind = builder.block_kind(block);

    let completed = builder.push_block(source.clone(), kind)?;

    let parameter = value
        .as_ref()
        .map(|value| {
            builder.push_block_parameter(completed, source.clone(), builder.operand_type(value))
        })
        .transpose()?;

    let (panicked, cancelled) =
        if kind == MirBlockKind::CleanupBroadcast && builder.block_kind(panicked) != kind {
            let panic_bridge = builder.push_block(source.clone(), kind)?;

            let cancel_bridge = builder.push_block(source.clone(), kind)?;

            builder.set_terminator(
                panic_bridge,
                source.clone(),
                MirTerminatorKind::ContinueCleanup(MirCleanupEdge::new(
                    MirCleanupPhase::LifecycleResolution,
                    MirEdge::new(panicked, []),
                )),
            );

            builder.set_terminator(
                cancel_bridge,
                source.clone(),
                MirTerminatorKind::ContinueCleanup(MirCleanupEdge::new(
                    MirCleanupPhase::LifecycleResolution,
                    MirEdge::new(cancelled, []),
                )),
            );

            (panic_bridge, cancel_bridge)
        } else {
            (panicked, cancelled)
        };

    builder.set_terminator(
        block,
        source.clone(),
        MirTerminatorKind::CheckCallOutcome {
            completed: MirEdge::new(completed, value),
            panicked: MirCallFailureEdge::new(panicked, report.clone()),
            cancelled: MirCallFailureEdge::new(cancelled, report),
        },
    );

    Ok((completed, parameter.map(MirOperand::Value)))
}

pub(crate) fn forward_cleanup_result(
    builder: &mut MirUnitBuilder,
    source: &MirSourceAnchor,
    target: MirBlockId,
    value: &mut Option<MirOperand>,
) -> Result<MirEdge, MirCapacityError> {
    let incoming = value.take();

    if let Some(incoming) = &incoming {
        let ty = builder.operand_type(incoming);
        let parameter = builder.push_block_parameter(target, source.clone(), ty)?;

        *value = Some(MirOperand::Value(parameter));
    }

    Ok(MirEdge::new(target, incoming))
}
