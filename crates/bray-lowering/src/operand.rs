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

/// Returns the checked payload types and representation of a Result completion.
pub(crate) fn result_payloads(
    values: &bray_symbols::SemanticValueStore,
    symbols: &bray_symbols::AvailableCompilerKnownSymbols,
    ty: TypeId,
) -> Option<([TypeId; 2], bray_symbols::CompilerKnownResultRepresentation)> {
    let data = values.type_data(ty);

    let bray_symbols::TypeData::Named {
        definition: bray_symbols::NamedTypeSymbolId::Union(definition),
        substitution,
    } = data.as_ref()
    else {
        return None;
    };

    if symbols.symbol_representation(*definition)
        != Some(bray_compiler_known::RepresentationRole::Result)
    {
        return None;
    }

    let substitution = values.generic_substitution_data(*substitution);

    let mut arguments =
        substitution
            .bindings()
            .iter()
            .filter_map(|binding| match binding.argument() {
                bray_symbols::GenericArgument::Type(ty) => Some(ty),
                bray_symbols::GenericArgument::Constant(_) => None,
            });

    let success = arguments
        .next()
        .expect("checked Result retains its success type");

    let error = arguments
        .next()
        .expect("checked Result retains its error type");

    assert!(
        arguments.next().is_none(),
        "checked Result retains exactly two type arguments"
    );

    Some((
        [success, error],
        symbols
            .result_representation()
            .expect("checked Result retains its compiler-known representation"),
    ))
}
