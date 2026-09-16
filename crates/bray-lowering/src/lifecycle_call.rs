use bray_bound_tree::BoundCallResult;
use bray_ir::{
    MirAsyncOperation, MirCall, MirCallTarget, MirCallableReference, MirFrameInitializer,
    MirCapacityError, MirFrameReference, MirOperand, MirOperationCommit, MirOperationKind,
    MirPlace, MirValueId,
};
use bray_symbols::{BorrowKind, TypeId};

/// Expands a selected invocation through the caller's ownership-aware operation sink.
pub(crate) fn lower_lifecycle_call(
    callable: MirCallableReference,
    receiver_type: TypeId,
    place: MirPlace,
    borrow: Option<BorrowKind>,
    result: BoundCallResult,
    cleanup: bool,
    mut emit: impl FnMut(
        MirOperationKind,
        Option<TypeId>,
    ) -> Result<MirOperationCommit, MirCapacityError>,
) -> Result<MirValueId, MirCapacityError> {
    let receiver =
        match borrow {
            Some(kind) => {
                let commit = emit(
                    MirOperationKind::Borrow { kind, place },
                    Some(receiver_type),
                )?;

                MirOperand::Value(
                    commit
                        .result()
                        .expect("value-producing MIR operation must publish a result"),
                )
            }
            None => MirOperand::Move(place),
        };

    let call = MirCall::protocol(MirCallTarget::Direct(callable), result, [receiver], []);

    let operation = match result {
        BoundCallResult::Immediate(_) => MirOperationKind::Call(call.with_cleanup(cleanup)),
        BoundCallResult::LazyFuture(_) => MirOperationKind::Async(MirAsyncOperation::CreateFrame {
            frame: MirFrameReference::Erased,
            initializer: MirFrameInitializer::Callable(call),
        }),
    };

    let commit = emit(operation, Some(result.ty()))?;

    Ok(commit
        .result()
        .expect("value-producing MIR operation must publish a result"))
}
