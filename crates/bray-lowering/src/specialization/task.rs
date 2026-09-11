use bray_ir::{
    MirAbandonmentAction, MirAsyncOperation, MirBlockId, MirCallPanicEdge, MirEdge,
    MirFrameStateId, MirGeneratedLifecycleRole, MirOperand, MirOperationId, MirOperationKind,
    MirPlace, MirProjectionKind, MirRuntimeReference, MirSourceAnchor, MirStorageKind,
    MirStoreKind, MirSuspensionKind, MirUnitBuildError, MirUnitBuilder,
};
use bray_runtime_interface::RuntimeAbiRole;
use bray_symbols::{BorrowKind, TypeData, TypeId};

use super::lifecycle::failure;
use crate::{SyntheticLoweringContext, SyntheticLoweringError};

pub(super) fn expand_task_cleanup<C: SyntheticLoweringContext + ?Sized>(
    context: &C,
    builder: &mut MirUnitBuilder,
    location: (MirBlockId, MirOperationId),
    source: &MirSourceAnchor,
    action: (MirGeneratedLifecycleRole, MirPlace, TypeId),
    state: MirFrameStateId,
    edges: (&MirEdge, MirCallPanicEdge, &MirEdge),
) -> Result<MirBlockId, C::Error> {
    let (block, _) = location;

    let (role, place, completion) = action;

    let invalid = |cause| failure(source, cause);
    let task = retain_owner(context, builder, location, source, place)?;
    let lowerer = crate::synthetic::SyntheticLowerer::new(context);
    let outcome = lowerer.cleanup_outcome(builder, block, source)?;

    // The template already owns the cleanup shield. Its owner pointer survives this suspension.
    let resume = crate::cleanup_await::suspend_cleanup(
        builder,
        block,
        source,
        state,
        MirSuspensionKind::TaskCompletion,
        Some(MirOperand::Copy(task.clone())),
    )
    .map_err(invalid)?;

    let finished = if role == MirGeneratedLifecycleRole::Abandon(MirAbandonmentAction::Quiesce) {
        lowerer.quiesce_task_completion(builder, resume, source, task, completion, &outcome)?
    } else {
        let (result, variants) = lowerer.run_result(completion)?;

        let result = crate::cleanup_await::resolve_cleanup_result(
            builder,
            resume,
            source,
            MirAsyncOperation::ResolveTask {
                task: MirOperand::Copy(task),
                variants,
                runtime: MirRuntimeReference::new(
                    RuntimeAbiRole::TaskResolution,
                    builder.target().runtime_abi(),
                ),
            },
            result,
        )
        .map_err(invalid)?;

        lowerer.resolve_owner_completion(
            builder,
            resume,
            source,
            result,
            (variants, completion),
            &outcome,
        )?
    };

    outcome
        .forward(builder, finished, source, edges)
        .map_err(invalid)?;

    Ok(resume)
}

fn retain_owner<C: SyntheticLoweringContext + ?Sized>(
    context: &C,
    builder: &mut MirUnitBuilder,
    location: (MirBlockId, MirOperationId),
    source: &MirSourceAnchor,
    place: MirPlace,
) -> Result<MirPlace, C::Error> {
    let (block, operation) = location;

    let invalid = |cause| failure(source, cause);
    let ty = place.ty();

    let receiver = context
        .semantic_values()
        .intern_type(TypeData::Borrow {
            kind: BorrowKind::Mutable,
            target: ty,
        })
        .map_err(SyntheticLoweringError::SemanticValue)?;

    let borrow = builder
        .replace_effect(
            operation,
            MirOperationKind::Borrow {
                kind: BorrowKind::Mutable,
                place,
            },
            Some(receiver),
        )
        .map_err(invalid)?;

    let value = borrow
        .result()
        .ok_or_else(|| invalid(MirUnitBuildError::MissingOperationResult(operation)))?;

    let storage = builder
        .push_storage(source.clone(), MirStorageKind::Temporary, receiver)
        .map_err(invalid)?;

    let receiver = MirPlace::new(storage, [], receiver);

    // Storage initialization and the resumed dereference independently own the template path.
    builder
        .push_operation(
            block,
            source.clone(),
            MirOperationKind::Store {
                kind: MirStoreKind::Initialize,
                destination: receiver.clone(),
                value: MirOperand::Value(value),
            },
            None,
        )
        .map_err(invalid)?;

    Ok(receiver.project(MirProjectionKind::Dereference, ty))
}
