use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirAsyncOperation, MirBlockId, MirCallPanicEdge, MirEdge, MirFrameEntry, MirFrameReference,
    MirFrameStateId, MirOperand, MirOperationId, MirOperationKind, MirPlace, MirRuntimeReference,
    MirSourceAnchor, MirSuspensionKind, MirTerminatorKind, MirUnitBuildError, MirUnitBuilder,
};
use bray_runtime_interface::RuntimeAbiRole;
use bray_symbols::TypeId;

use super::lifecycle::failure;
use crate::SyntheticLoweringContext;
use crate::cleanup_outcome::CleanupCancellation;

// Generated operations independently retain shared provenance and the caller's ownership paths.
pub(super) fn expand_future_cleanup<C: SyntheticLoweringContext + ?Sized>(
    context: &C,
    builder: &mut MirUnitBuilder,
    location: (MirBlockId, MirOperationId),
    source: &MirSourceAnchor,
    action: (MirFrameEntry, MirPlace, TypeId),
    state: MirFrameStateId,
    edges: (&MirEdge, MirCallPanicEdge, &MirEdge),
) -> Result<MirBlockId, C::Error> {
    let (block, operation) = location;

    let (entry, place, completion) = action;

    let invalid = |cause| failure(source, cause);

    let parent = builder
        .protected_frame()
        .ok_or_else(|| invalid(MirUnitBuildError::ProtectedFrameMismatch))?;

    let abi = builder.target().runtime_abi();
    let resolves = entry == MirFrameEntry::CaptureCleanup;

    let completion = if resolves {
        completion
    } else {
        context.representation_type(RepresentationRole::Unit)?
    };

    let operand = if resolves {
        MirOperand::Move(place)
    } else {
        MirOperand::Copy(place)
    };

    builder
        .replace_effect(
            operation,
            MirOperationKind::Async(MirAsyncOperation::ComposeAwaitedFrame {
                parent,
                child: MirFrameReference::Erased,
                frame: operand,
                entry,
            }),
            None,
        )
        .map_err(invalid)?;

    // Template cleanup already owns the shield surrounding its checked outcome continuation.
    let resume = crate::cleanup_await::suspend_cleanup(
        builder,
        block,
        source,
        state,
        MirSuspensionKind::Awaited,
        None,
    )
    .map_err(invalid)?;

    let lowerer = crate::synthetic::SyntheticLowerer { context };

    let (result, variants) = lowerer.run_result(completion)?;

    let result = crate::cleanup_await::resolve_cleanup_result(
        builder,
        resume,
        source,
        MirAsyncOperation::ResolveAwaitedFrame {
            variants,
            runtime: MirRuntimeReference::new(RuntimeAbiRole::AwaitedFrameResolution, abi),
        },
        result,
    )
    .map_err(invalid)?;

    let outcome = lowerer.cleanup_outcome(builder, resume, source)?;

    let finished = if resolves {
        lowerer.resolve_owner_completion(
            builder,
            resume,
            source,
            result,
            (variants, completion),
            &outcome,
        )?
    } else {
        let (completed, finished) = outcome
            .resolve_run_result(
                builder,
                resume,
                source,
                result,
                (variants, CleanupCancellation::Propagate),
            )
            .map_err(invalid)?;

        builder
            .set_terminator(
                completed,
                source.clone(),
                MirTerminatorKind::Goto(MirEdge::new(finished, [])),
            )
            .map_err(invalid)?;

        finished
    };

    super::outcome::forward_collected_outcome(builder, finished, source, &outcome, edges)
        .map_err(invalid)?;

    Ok(resume)
}
