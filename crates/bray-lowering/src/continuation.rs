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

    let composition = destination.map_or(
        bray_ir::MirFrameComposition::Abandon,
        bray_ir::MirFrameComposition::Completion,
    );

    compose_frame(
        builder,
        current,
        source,
        parent,
        frame,
        composition,
        state,
        cancellation,
        completion,
    )
}

/// Composes the checked child disposition using the enclosing continuation and collection.
#[expect(
    clippy::too_many_arguments,
    reason = "composition preserves the child ownership disposition and checked parent continuation"
)]
pub(crate) fn compose_frame(
    builder: &mut MirUnitBuilder,
    current: MirBlockId,
    source: &MirSourceAnchor,
    parent: ProtectedAsyncFrameId,
    frame: MirOperand,
    composition: bray_ir::MirFrameComposition,
    state: &MirFrameState,
    cancellation: MirCleanupEdge,
    completion: Option<TypeId>,
) -> Result<Option<MirValueId>, MirCapacityError> {
    let destination = composition.completion();

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
            composition,
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

            transfer_cleanup(builder, panic_bridge, source, MirEdge::new(panicked, []));

            transfer_cleanup(builder, cancel_bridge, source, MirEdge::new(cancelled, []));

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
            completed_report: None,
        },
    );

    Ok((completed, parameter.map(MirOperand::Value)))
}

/// Emits the explicit phase transition when a cleanup outcome enters resolution.
pub(crate) fn transfer_cleanup(
    builder: &mut MirUnitBuilder,
    block: MirBlockId,
    source: &MirSourceAnchor,
    edge: MirEdge,
) {
    let next = if builder.block_kind(block) == MirBlockKind::CleanupBroadcast
        && builder.block_kind(edge.target()) == MirBlockKind::LifecycleResolution
    {
        MirTerminatorKind::ContinueCleanup(MirCleanupEdge::new(
            MirCleanupPhase::LifecycleResolution,
            edge,
        ))
    } else {
        MirTerminatorKind::Goto(edge)
    };

    builder.set_terminator(block, source.clone(), next);
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
