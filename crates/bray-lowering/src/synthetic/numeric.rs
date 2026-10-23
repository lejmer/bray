use bray_compiler_known::ImplementationHook;
use bray_ir::{
    MirBlockKind, MirOperand, MirPlace, MirSourceAnchor, MirStorageKind, MirTargetContract,
    MirTerminatorKind, MirUnit, MirUnitBuilder, MirUnitId,
};
use bray_symbols::{
    AvailableCompilerKnownSymbols, CallableInstanceData, NumericTruncation, SemanticValueStore,
    TypeId,
};

use crate::SyntheticLoweringError;

/// Realizes a demanded numeric policy as a normal callable without adding a runtime wrapper.
/// Callers have already established its checked type constraint and ordinary input requirements.
pub fn lower_numeric_callable(
    values: &SemanticValueStore,
    symbols: &AvailableCompilerKnownSymbols,
    unit: MirUnitId,
    callable: CallableInstanceData,
    hook: ImplementationHook,
    parameters: &[TypeId],
    result: TypeId,
    target: &MirTargetContract,
) -> Result<MirUnit, SyntheticLoweringError> {
    let source = MirSourceAnchor::CompilerProvidedCallable(callable.definition());

    let mut builder =
        MirUnitBuilder::for_compiler_provided_callable(unit, callable.definition(), target.clone());

    let entry = builder
        .push_block(source.clone(), MirBlockKind::Ordinary)
        .map_err(SyntheticLoweringError::Capacity)?;

    let mut inputs = Vec::with_capacity(parameters.len());

    for (ordinal, ty) in parameters.iter().copied().enumerate() {
        let storage = builder
            .push_storage(
                source.clone(),
                MirStorageKind::Parameter(
                    u32::try_from(ordinal).expect("numeric callable input ordinal fits u32"),
                ),
                ty,
            )
            .map_err(SyntheticLoweringError::Capacity)?;

        inputs.push(MirOperand::Copy(MirPlace::new(storage, [], ty)));
    }

    let plan = NumericTruncation::for_instance(
        values,
        symbols,
        callable,
        target.machine().pointer_width_bits().get(),
    );

    let value = crate::numeric::lower_numeric_value(
        &mut builder,
        values,
        entry,
        &source,
        hook,
        plan,
        &inputs,
        result,
    )?;

    builder.set_terminator(
        entry,
        source,
        MirTerminatorKind::Return {
            value: Some(value),
            report: None,
        },
    );

    Ok(builder.finish(entry))
}
