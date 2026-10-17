use bray_ir::MirOperand;
use bray_symbols::{
    ConstantValueData, ConstantValueKind, IntegerConstant, SemanticValueStore,
    SemanticValueStoreError, TypeId,
};

pub(crate) fn integer_constant(
    values: &SemanticValueStore,
    ty: TypeId,
    value: u64,
) -> Result<MirOperand, SemanticValueStoreError> {
    let value = values.intern_constant_value(ConstantValueData::new(
        ty,
        ConstantValueKind::Integer(IntegerConstant::from_u64(value)),
    ))?;

    Ok(MirOperand::Constant { value, ty })
}

/// Builds Result payload branches for the caller to install through its ownership-aware sink.
// Each branch owns the same Arc-backed provenance and only its active payload.
pub(crate) fn branch_result(
    builder: &mut bray_ir::MirUnitBuilder,
    current: bray_ir::MirBlockId,
    source: &bray_ir::MirSourceAnchor,
    value: MirOperand,
    types: [TypeId; 2],
    representation: bray_symbols::CompilerKnownResultRepresentation,
) -> Result<
    (
        bray_ir::MirTerminatorKind,
        [(bray_ir::MirBlockId, MirOperand); 2],
    ),
    bray_ir::MirCapacityError,
> {
    let result_type = builder.operand_type(&value);
    let kind = builder.block_kind(current);

    let [success, error] = [
        (
            types[0],
            representation.success_variant(),
            representation.success_field(),
        ),
        (
            types[1],
            representation.error_variant(),
            representation.error_field(),
        ),
    ]
    .map(
        |(ty, variant, field)| -> Result<_, bray_ir::MirCapacityError> {
            let block = builder.push_block(source.clone(), kind)?;
            let parameter = builder.push_block_parameter(block, source.clone(), result_type)?;

            let payload = builder
                .push_operation(
                    block,
                    source.clone(),
                    bray_ir::MirOperationKind::PatternProjection {
                        subject: MirOperand::Value(parameter),
                        projection: bray_bound_tree::PatternProjection::ActiveUnionPayloadField {
                            variant,
                            field,
                        },
                        operation: bray_bound_tree::PatternOperation::Consume,
                    },
                    Some(ty),
                )?
                .result()
                .expect("Result payload projection produces its checked value");

            Ok((block, MirOperand::Value(payload)))
        },
    );

    let (success, success_value) = success?;

    let (error, error_value) = error?;

    let branch = bray_ir::MirTerminatorKind::PatternBranch {
        // The branch forwards this owned value along exactly one successor.
        subject: value.clone(),
        predicate: bray_ir::MirPatternPredicate::ActiveUnionVariant(
            representation.success_variant(),
        ),
        matched: bray_ir::MirEdge::new(success, [value.clone()]),
        unmatched: bray_ir::MirEdge::new(error, [value]),
    };

    Ok((branch, [(success, success_value), (error, error_value)]))
}
