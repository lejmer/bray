use bray_ir::{
    MirAsyncOperation, MirBlockId, MirEdge, MirFrameInitializer, MirFrameReference, MirOperand,
    MirOperationKind, MirPanicCause, MirPlace, MirSourceAnchor, MirStorageKind, MirTerminatorKind,
    MirUnitBuildError, MirUnitBuilder,
};
use bray_symbols::TypeId;

/// Splits frame construction into ownership-committing success and caller-owned failure paths.
pub(crate) fn create_frame(
    builder: &mut MirUnitBuilder,
    block: MirBlockId,
    source: &MirSourceAnchor,
    initializer: MirFrameInitializer,
    storage_source: bray_ir::MirFrameStorageSource,
    boolean: TypeId,
) -> Result<(MirBlockId, MirBlockId, MirPlace), MirUnitBuildError> {
    let future = initializer
        .future_type()
        .ok_or(MirUnitBuildError::ProtectedFrameMismatch)?;

    let storage = builder.push_storage(source.clone(), MirStorageKind::Temporary, future)?;
    let destination = MirPlace::new(storage, [], future);
    let kind = builder.block_kind(block)?;
    let created = builder.push_block(source.clone(), kind)?;
    let rejected = builder.push_block(source.clone(), kind)?;

    let operation = builder.push_operation(
        block,
        source.clone(),
        MirOperationKind::Async(MirAsyncOperation::CreateFrame {
            storage: storage_source,
            frame: MirFrameReference::Erased,
            initializer,
            destination: destination.clone(),
        }),
        Some(boolean),
    )?;

    let admitted = operation
        .result()
        .ok_or(MirUnitBuildError::MissingOperationResult(
            operation.operation(),
        ))?;

    builder.set_terminator(
        block,
        source.clone(),
        MirTerminatorKind::Branch {
            condition: MirOperand::Value(admitted),
            then_edge: MirEdge::new(created, []),
            else_edge: MirEdge::new(rejected, []),
        },
    )?;

    Ok((created, rejected, destination))
}

/// Builds the language panic associated with failed protected-frame allocation.
pub(crate) fn allocation_panic(
    builder: &mut MirUnitBuilder,
    block: MirBlockId,
    source: &MirSourceAnchor,
    report: TypeId,
) -> Result<MirOperand, MirUnitBuildError> {
    let operation = builder.push_operation(
        block,
        source.clone(),
        MirOperationKind::PanicReport(MirPanicCause::FrameAllocation),
        Some(report),
    )?;

    let value = operation
        .result()
        .ok_or(MirUnitBuildError::MissingOperationResult(
            operation.operation(),
        ))?;

    Ok(MirOperand::Value(value))
}
