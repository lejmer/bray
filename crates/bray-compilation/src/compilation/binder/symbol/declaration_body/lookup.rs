use bray_binder::{BinderFactContext, BinderFactError, BinderFactResult};
use bray_symbols::{
    AnySymbolId, CallableParameterSymbolId, StructFieldSymbolId, UnionPayloadFieldSymbolId,
    UnionVariantSymbolId,
};

use crate::compilation::binder::CompilationBindingContext;

pub(in crate::compilation) fn runtime_default_provider(
    context: &CompilationBindingContext<'_>,
    owner: AnySymbolId,
) -> BinderFactResult<Option<AnySymbolId>> {
    if let Some(provider) = context.symbols().runtime_default_provider(owner) {
        return Ok(Some(provider));
    }

    let Some(imported) = context.imported_symbols()? else {
        return Ok(None);
    };

    Ok(match owner {
        AnySymbolId::CallableParameter(owner) => imported
            .callable_parameter(owner)
            .and_then(|record| record.default_provider())
            .map(Into::into),
        AnySymbolId::StructField(owner) => imported
            .struct_field(owner)
            .and_then(|record| record.default_provider())
            .map(Into::into),
        AnySymbolId::UnionPayloadField(owner) => imported
            .union_payload_field(owner)
            .and_then(|record| record.default_provider())
            .map(Into::into),
        _ => None,
    })
}

pub(super) fn callable_parameter<'facts>(
    context: &'facts CompilationBindingContext<'_>,
    owner: CallableParameterSymbolId,
) -> BinderFactResult<&'facts bray_symbols::CallableParameterSymbol> {
    if let Some(record) = context.symbols().callable_parameter(owner) {
        return Ok(record);
    }

    context
        .imported_symbols()?
        .and_then(|symbols| symbols.callable_parameter(owner))
        .ok_or(BinderFactError::DependencyUnavailable)
}

pub(super) fn struct_field<'facts>(
    context: &'facts CompilationBindingContext<'_>,
    owner: StructFieldSymbolId,
) -> BinderFactResult<&'facts bray_symbols::StructFieldSymbol> {
    if let Some(record) = context.symbols().struct_field(owner) {
        return Ok(record);
    }

    context
        .imported_symbols()?
        .and_then(|symbols| symbols.struct_field(owner))
        .ok_or(BinderFactError::DependencyUnavailable)
}

pub(super) fn union_payload_field<'facts>(
    context: &'facts CompilationBindingContext<'_>,
    owner: UnionPayloadFieldSymbolId,
) -> BinderFactResult<&'facts bray_symbols::UnionPayloadFieldSymbol> {
    if let Some(record) = context.symbols().union_payload_field(owner) {
        return Ok(record);
    }

    context
        .imported_symbols()?
        .and_then(|symbols| symbols.union_payload_field(owner))
        .ok_or(BinderFactError::DependencyUnavailable)
}

pub(super) fn union_variant<'facts>(
    context: &'facts CompilationBindingContext<'_>,
    owner: UnionVariantSymbolId,
) -> BinderFactResult<&'facts bray_symbols::UnionVariantSymbol> {
    if let Some(record) = context.symbols().union_variant(owner) {
        return Ok(record);
    }

    context
        .imported_symbols()?
        .and_then(|symbols| symbols.union_variant(owner))
        .ok_or(BinderFactError::DependencyUnavailable)
}
