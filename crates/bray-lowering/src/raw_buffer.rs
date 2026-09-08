use bray_ir::{
    MirBlockId, MirOperand, MirOperationKind, MirPlace, MirProjectionKind, MirSourceAnchor,
    MirStoreKind, MirUnitBuildError, MirUnitBuilder,
};

/// Restores the empty, initialized representation after ownership transfer or storage release.
pub(crate) fn reset_raw_buffer(
    builder: &mut MirUnitBuilder,
    block: MirBlockId,
    source: &MirSourceAnchor,
    place: MirPlace,
    null: MirOperand,
    zero: MirOperand,
) -> Result<(), MirUnitBuildError> {
    let pointer = builder.operand_type(&null)?;
    let integer = builder.operand_type(&zero)?;

    for (field, ty, value) in [
        (0, pointer, null),
        (1, integer, zero.clone()),
        (2, integer, zero),
    ] {
        builder.push_operation(
            block,
            source.clone(),
            MirOperationKind::Store {
                kind: MirStoreKind::Assign,
                destination: place.project(MirProjectionKind::TupleField(field), ty),
                value,
            },
            None,
        )?;
    }

    Ok(())
}
