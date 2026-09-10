use bray_ir::{
    MirAsyncOperation, MirBlockId, MirBlockKind, MirEdge, MirFrameReference, MirFrameStateId,
    MirOperand, MirOperationKind, MirPlace, MirRunResultVariants, MirRuntimeReference,
    MirSourceAnchor, MirStorageKind, MirStoreKind, MirSuspensionKind, MirTerminatorKind,
    MirUnitBuildError, MirUnitBuilder,
};
use bray_runtime_interface::RuntimeAbiRole;
use bray_symbols::TypeId;

/// Selects an existing future entry for asynchronous ownership resolution.
pub(crate) fn future_cleanup_entry(
    role: bray_ir::MirGeneratedLifecycleRole,
) -> Option<bray_ir::MirFrameEntry> {
    match role {
        bray_ir::MirGeneratedLifecycleRole::Destroy
        | bray_ir::MirGeneratedLifecycleRole::Cleanup(
            bray_ir::MirCleanupPhase::LifecycleResolution,
        ) => Some(bray_ir::MirFrameEntry::CaptureCleanup),
        bray_ir::MirGeneratedLifecycleRole::Abandon(bray_ir::MirAbandonmentAction::Quiesce) => {
            Some(bray_ir::MirFrameEntry::CaptureQuiescence)
        }
        _ => None,
    }
}

/// Attempts to construct an inactive lifecycle helper from its already borrowed receiver.
pub(crate) fn create_lifecycle_frame(
    builder: &mut MirUnitBuilder,
    block: MirBlockId,
    source: &MirSourceAnchor,
    role: bray_ir::MirGeneratedLifecycleRole,
    ty: TypeId,
    receiver: MirOperand,
    result: bray_bound_tree::BoundFutureConstruction,
    boolean: TypeId,
) -> Result<(MirBlockId, MirBlockId, MirPlace), MirUnitBuildError> {
    crate::frame_creation::create_frame(
        builder,
        block,
        source,
        bray_ir::MirFrameInitializer::Lifecycle {
            role,
            ty,
            receiver,
            result,
        },
        bray_ir::MirFrameStorageSource::Fresh,
        boolean,
    )
}

/// The owner whose terminal outcome is awaited by a cleanup continuation.
pub(crate) enum CleanupAwait {
    Frame(MirOperand, bray_ir::MirFrameEntry),
    AttachedFrame,
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

    let task = match awaited {
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

            None
        }
        CleanupAwait::AttachedFrame => None,
        CleanupAwait::Task(task) => Some(task),
    };

    let (kind, payload, resolution) = match task {
        None => (
            MirSuspensionKind::Awaited,
            None,
            MirAsyncOperation::ResolveAwaitedFrame {
                variants,
                runtime: MirRuntimeReference::new(RuntimeAbiRole::AwaitedFrameResolution, abi),
            },
        ),
        Some(task) => {
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

    let place = resolve_cleanup_result(builder, resume, source, resolution, result)?;

    Ok((resume, place))
}

/// Moves a resumed cleanup result into storage retained across any nested payload cleanup.
pub(crate) fn resolve_cleanup_result(
    builder: &mut MirUnitBuilder,
    resume: MirBlockId,
    source: &MirSourceAnchor,
    resolution: MirAsyncOperation,
    result: TypeId,
) -> Result<MirPlace, MirUnitBuildError> {
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

    Ok(place)
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

/// Publishes a generated cleanup frame's outcome to its awaiting owner.
pub(crate) fn finish_cleanup_frame(
    builder: &mut MirUnitBuilder,
    block: MirBlockId,
    source: &MirSourceAnchor,
    state: bray_ir::MirTaskTerminalState,
) -> Result<(), MirUnitBuildError> {
    builder.push_operation(
        block,
        source.clone(),
        MirOperationKind::Async(MirAsyncOperation::PublishTerminalState {
            state,
            runtime: MirRuntimeReference::new(
                RuntimeAbiRole::TerminalPublication,
                builder.target().runtime_abi(),
            ),
        }),
        None,
    )?;

    builder.set_terminator(block, source.clone(), MirTerminatorKind::Return(None))
}

/// Resolves the pointer representations used while a task retains its completed payload.
pub(crate) fn task_completion_borrow_types(
    values: &bray_symbols::SemanticValueStore,
    completion: TypeId,
) -> Result<(TypeId, TypeId), bray_symbols::SemanticValueStoreError> {
    let pointer = values.intern_type(bray_symbols::TypeData::Borrow {
        kind: bray_symbols::BorrowKind::Mutable,
        target: completion,
    })?;

    let nullable = values.intern_type(bray_symbols::TypeData::Nullable(pointer))?;

    Ok((pointer, nullable))
}

/// Borrows a completed task payload, branching past cleanup when no completed value exists.
pub(crate) fn borrow_task_completion(
    builder: &mut MirUnitBuilder,
    block: MirBlockId,
    source: &MirSourceAnchor,
    task: MirOperand,
    completion: TypeId,
    borrow_types: (TypeId, TypeId),
) -> Result<(MirBlockId, MirBlockId, MirPlace), MirUnitBuildError> {
    let (pointer, nullable) = borrow_types;

    let borrowed = resolve_cleanup_result(
        builder,
        block,
        source,
        MirAsyncOperation::BorrowTaskCompletion {
            task,
            runtime: MirRuntimeReference::new(
                RuntimeAbiRole::TaskCompletionBorrow,
                builder.target().runtime_abi(),
            ),
        },
        nullable,
    )?;

    let completed = builder.push_block(source.clone(), MirBlockKind::LifecycleResolution)?;
    let finished = builder.push_block(source.clone(), MirBlockKind::LifecycleResolution)?;

    // The branch owns its path while the caller retains the payload projection for quiescence.
    builder.set_terminator(
        block,
        source.clone(),
        MirTerminatorKind::PatternBranch {
            subject: MirOperand::Copy(borrowed.clone()),
            predicate: bray_ir::MirPatternPredicate::NullablePresent,
            matched: MirEdge::new(completed, []),
            unmatched: MirEdge::new(finished, []),
        },
    )?;

    let payload = borrowed
        .project(bray_ir::MirProjectionKind::NullableValue, pointer)
        .project(bray_ir::MirProjectionKind::Dereference, completion);

    Ok((completed, finished, payload))
}

/// Releases a retained completion after every selected nested quiescence path has rejoined.
pub(crate) fn release_task_completion_borrow(
    builder: &mut MirUnitBuilder,
    block: MirBlockId,
    source: &MirSourceAnchor,
    task: MirOperand,
) -> Result<(), MirUnitBuildError> {
    builder.push_operation(
        block,
        source.clone(),
        MirOperationKind::Async(MirAsyncOperation::ReleaseTaskCompletionBorrow {
            task,
            runtime: MirRuntimeReference::new(
                RuntimeAbiRole::TaskCompletionBorrowRelease,
                builder.target().runtime_abi(),
            ),
        }),
        None,
    )?;

    Ok(())
}
