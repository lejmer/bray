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
#[expect(
    clippy::too_many_arguments,
    reason = "numeric callable synthesis joins semantic stores, checked callable types, implementation hook, and target"
)]
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

    let input = match parameters {
        [] => None,
        [ty] => {
            let storage = builder
                .push_storage(source.clone(), MirStorageKind::Parameter(0), *ty)
                .map_err(SyntheticLoweringError::Capacity)?;

            Some((MirOperand::Copy(MirPlace::new(storage, [], *ty)), *ty))
        }
        _ => panic!("checked numeric callable has zero or one input"),
    };

    let plan = NumericTruncation::for_instance(
        values,
        symbols,
        callable,
        target.machine().pointer_width_bits().get(),
    );

    let value = crate::numeric::lower_numeric_value(
        values,
        hook,
        plan,
        input,
        result,
        |operation, ty| {
            let commit = builder
                .push_operation(entry, source.clone(), operation, Some(ty))
                .map_err(SyntheticLoweringError::Capacity)?;

            Ok(MirOperand::Value(
                commit
                    .result()
                    .expect("numeric operation produces its typed value"),
            ))
        },
        SyntheticLoweringError::SemanticValue,
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
