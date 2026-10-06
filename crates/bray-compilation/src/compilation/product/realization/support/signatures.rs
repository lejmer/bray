use bray_codegen::{CodegenCallableSignature, CodegenParameterMapping, CodegenResultMapping};
use bray_compiler_known::RepresentationRole;
use bray_ir::{MirOperation, MirUnit};
use bray_symbols::{
    BorrowKind, CallableAbi, CallableExecution, ReceiverMode, SemanticValueStore, TypeData, TypeId,
};

use crate::compilation::{
    CodegenPreparationError, Compilation, ProductDataKind, ProductQueryContext, ProductQueryFailure,
};
use crate::fact::FactQueryError;

pub(in crate::compilation::product::realization) fn signature_types(
    signature: &CodegenCallableSignature,
) -> impl Iterator<Item = TypeId> + '_ {
    signature
        .parameters()
        .iter()
        .filter_map(|parameter| match parameter {
            CodegenParameterMapping::Ignore => None,
            CodegenParameterMapping::Direct { ty, .. } => Some(*ty),
            CodegenParameterMapping::Indirect { pointer, .. } => Some(*pointer),
        })
        .chain(match signature.result() {
            CodegenResultMapping::Void => None,
            CodegenResultMapping::Direct { ty, .. } => Some(*ty),
            CodegenResultMapping::Indirect { pointer, .. } => Some(*pointer),
        })
}

pub(in crate::compilation::product::realization) fn callable_type_signature(
    compilation: &Compilation,
    callable: &bray_symbols::CallableTypeData,
) -> Result<CodegenCallableSignature, CodegenPreparationError> {
    let result_type = if callable.execution() == CallableExecution::Asynchronous {
        compilation
            .available_compiler_known_symbols()
            .unary_representation_type(
                compilation.semantic_value_store()?,
                RepresentationRole::Future,
                callable.result(),
            )
            .map_err(FactQueryError::SemanticValueStore)?
            .ok_or_else(|| {
                ProductQueryFailure::missing(
                    ProductQueryContext::UnaryRepresentation {
                        role: RepresentationRole::Future,
                        argument: callable.result(),
                    },
                    ProductDataKind::CompilerKnownRepresentation,
                )
            })?
    } else {
        callable.result()
    };

    let result = if is_void_result(compilation, result_type)? {
        CodegenResultMapping::Void
    } else {
        CodegenResultMapping::direct(result_type, None, [])
    };

    let signature = CodegenCallableSignature::new(
        callable
            .parameters()
            .iter()
            .map(|parameter| CodegenParameterMapping::direct(parameter.ty(), None, [])),
        result,
        callable.abi(),
        false,
    );

    Ok(synchronous_bray_signature(
        signature,
        callable.abi(),
        callable.execution(),
    ))
}

pub(in crate::compilation::product::realization) fn synchronous_bray_signature(
    signature: CodegenCallableSignature,
    abi: CallableAbi,
    execution: CallableExecution,
) -> CodegenCallableSignature {
    if abi == CallableAbi::Bray && execution == CallableExecution::Synchronous {
        signature.with_panic_report_context()
    } else {
        signature
    }
}

pub(in crate::compilation::product::realization) fn operation_result_type(
    mir: &MirUnit,
    operation: &MirOperation,
) -> Option<TypeId> {
    operation
        .result()
        .and_then(|result| mir.value(result))
        .map(bray_ir::MirValue::ty)
}

pub(in crate::compilation::product::realization) fn void_signature(
    abi: CallableAbi,
) -> CodegenCallableSignature {
    CodegenCallableSignature::new([], CodegenResultMapping::Void, abi, false)
}

pub(in crate::compilation::product::realization) fn receiver_codegen_type(
    values: &SemanticValueStore,
    ty: TypeId,
    mode: ReceiverMode,
) -> Result<TypeId, FactQueryError> {
    let data = match mode {
        ReceiverMode::Shared => Some(TypeData::Borrow {
            kind: BorrowKind::Shared,
            target: ty,
        }),
        ReceiverMode::Mutable => Some(TypeData::Borrow {
            kind: BorrowKind::Mutable,
            target: ty,
        }),
        ReceiverMode::Consuming | ReceiverMode::ConsumingMutable => None,
    };

    match data {
        Some(data) => values
            .intern_type(data)
            .map_err(FactQueryError::SemanticValueStore),
        None => Ok(ty),
    }
}

pub(in crate::compilation::product::realization) fn is_void_result(
    compilation: &Compilation,
    ty: TypeId,
) -> Result<bool, FactQueryError> {
    let values = compilation.semantic_value_store()?;

    let data = values.type_data(ty);

    let TypeData::Named { definition, .. } = data.as_ref() else {
        return Ok(false);
    };

    Ok(matches!(
        crate::compilation::foreign::compiler_known_representation(compilation, *definition),
        Some(RepresentationRole::Unit | RepresentationRole::Never)
    ))
}
