use bray_ir::{
    MirAsyncOperation, MirBlockId, MirBlockKind, MirEdge, MirFrameReference, MirFrameStateId,
    MirOperand, MirOperationKind, MirPlace, MirRunResultVariants, MirRuntimeReference,
    MirSourceAnchor, MirStorageKind, MirStoreKind, MirSuspensionKind, MirTerminatorKind,
    MirUnitBuildError, MirUnitBuilder,
};
use bray_runtime_interface::RuntimeAbiRole;
use bray_symbols::TypeId;

/// Constructs an inactive lifecycle helper from its already borrowed receiver.
pub(crate) fn create_lifecycle_frame(
    builder: &mut MirUnitBuilder,
    block: MirBlockId,
    source: &MirSourceAnchor,
    role: bray_ir::MirGeneratedLifecycleRole,
    ty: TypeId,
    receiver: MirOperand,
    result: bray_bound_tree::BoundFutureConstruction,
) -> Result<MirOperand, MirUnitBuildError> {
    let future = result.future_type();

    let operation = builder.push_operation(
        block,
        source.clone(),
        MirOperationKind::Async(MirAsyncOperation::CreateFrame {
            frame: MirFrameReference::Erased,
            initializer: bray_ir::MirFrameInitializer::Lifecycle {
                role,
                ty,
                receiver,
                result,
            },
        }),
        Some(future),
    )?;

    let value = operation
        .result()
        .ok_or(MirUnitBuildError::MissingOperationResult(
            operation.operation(),
        ))?;

    Ok(MirOperand::Value(value))
}

/// The owner whose terminal outcome is awaited by a cleanup continuation.
pub(crate) enum CleanupAwait {
    Frame(MirOperand, bray_ir::MirFrameEntry),
    Task(MirOperand),
}

/// Waits for a cleanup child and moves its terminal outcome after shielded resumption.
/// The caller owns the shield, source initialization guards, and frame-state descriptor.
pub(crate) fn await_cleanup(
    builder: &mut MirUnitBuilder,
    block: MirBlockId,
    source: &MirSourceAnchor,
    state: MirFrameStateId,
    awaited: CleanupAwait,
    result: TypeId,
    variants: MirRunResultVariants,
) -> Result<(MirBlockId, MirPlace), MirUnitBuildError> {
    let parent = builder
        .protected_frame()
        .ok_or(MirUnitBuildError::ProtectedFrameMismatch)?;

    let abi = builder.target().runtime_abi();

    let (kind, payload, resolution) = match awaited {
        CleanupAwait::Frame(frame, entry) => {
            builder.push_operation(
                block,
                source.clone(),
                MirOperationKind::Async(MirAsyncOperation::ComposeAwaitedFrame {
                    parent,
                    child: MirFrameReference::Erased,
                    frame,
                    entry,
                }),
                None,
            )?;

            (
                MirSuspensionKind::Awaited,
                None,
                MirAsyncOperation::ResolveAwaitedFrame {
                    variants,
                    runtime: MirRuntimeReference::new(RuntimeAbiRole::AwaitedFrameResolution, abi),
                },
            )
        }
        CleanupAwait::Task(task) => {
            // Suspension borrows the task identity and the resumed transfer consumes its result.
            (
                MirSuspensionKind::TaskCompletion,
                Some(task.clone()),
                MirAsyncOperation::ResolveTask {
                    task,
                    variants,
                    runtime: MirRuntimeReference::new(RuntimeAbiRole::TaskResolution, abi),
                },
            )
        }
    };

    let resume = suspend_cleanup(builder, block, source, state, kind, payload)?;

    let value = builder.push_operation(
        resume,
        source.clone(),
        MirOperationKind::Async(resolution),
        Some(result),
    )?;

    let value = value
        .result()
        .ok_or(MirUnitBuildError::MissingOperationResult(value.operation()))?;

    let storage = builder.push_storage(source.clone(), MirStorageKind::Temporary, result)?;
    let place = MirPlace::new(storage, [], result);

    builder.push_operation(
        resume,
        source.clone(),
        MirOperationKind::Store {
            kind: MirStoreKind::Initialize,
            destination: place.clone(),
            value: MirOperand::Value(value),
        },
        None,
    )?;

    Ok((resume, place))
}

/// Suspends a shielded cleanup continuation while preserving its owner's values in place.
pub(crate) fn suspend_cleanup(
    builder: &mut MirUnitBuilder,
    block: MirBlockId,
    source: &MirSourceAnchor,
    state: MirFrameStateId,
    kind: MirSuspensionKind,
    payload: Option<MirOperand>,
) -> Result<MirBlockId, MirUnitBuildError> {
    builder
        .protected_frame()
        .ok_or(MirUnitBuildError::ProtectedFrameMismatch)?;

    let abi = builder.target().runtime_abi();
    let resume = builder.push_block(source.clone(), MirBlockKind::LifecycleResolution)?;

    builder.set_terminator(
        block,
        source.clone(),
        MirTerminatorKind::Suspend {
            kind,
            payload,
            resume_state: state,
            resume: MirEdge::new(resume, []),
            cancellation: None,
            registration: MirRuntimeReference::new(RuntimeAbiRole::SuspensionRegistration, abi),
            wake: MirRuntimeReference::new(RuntimeAbiRole::Wake, abi),
        },
    )?;

    Ok(resume)
}
