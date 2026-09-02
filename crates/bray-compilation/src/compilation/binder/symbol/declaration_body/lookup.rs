use crate::compilation::binder::BindingQueryResult;
use bray_binder::{BindingError, BindingQueryContext, BindingQueryError};
use bray_symbols::{
    AnySymbolId, CallableParameterSymbolId, StructFieldSymbolId, UnionPayloadFieldSymbolId,
    UnionVariantSymbolId,
};

use crate::compilation::binder::CompilationBindingContext;

pub(in crate::compilation) fn runtime_default_provider(
    context: &CompilationBindingContext<'_>,
    owner: AnySymbolId,
) -> BindingQueryResult<Option<AnySymbolId>> {
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

pub(super) fn callable_parameter<'binding>(
    context: &'binding CompilationBindingContext<'_>,
    owner: CallableParameterSymbolId,
) -> BindingQueryResult<&'binding bray_symbols::CallableParameterSymbol> {
    if let Some(record) = context.symbols().callable_parameter(owner) {
        return Ok(record);
    }

    context
        .imported_symbols()?
        .and_then(|symbols| symbols.callable_parameter(owner))
        .ok_or(BindingQueryError::Binding(
            BindingError::SymbolRecordUnavailable(owner.into()),
        ))
}

pub(super) fn struct_field<'binding>(
    context: &'binding CompilationBindingContext<'_>,
    owner: StructFieldSymbolId,
) -> BindingQueryResult<&'binding bray_symbols::StructFieldSymbol> {
    if let Some(record) = context.symbols().struct_field(owner) {
        return Ok(record);
    }

    context
        .imported_symbols()?
        .and_then(|symbols| symbols.struct_field(owner))
        .ok_or(BindingQueryError::Binding(
            BindingError::SymbolRecordUnavailable(owner.into()),
        ))
}

pub(super) fn union_payload_field<'binding>(
    context: &'binding CompilationBindingContext<'_>,
    owner: UnionPayloadFieldSymbolId,
) -> BindingQueryResult<&'binding bray_symbols::UnionPayloadFieldSymbol> {
    if let Some(record) = context.symbols().union_payload_field(owner) {
        return Ok(record);
    }

    context
        .imported_symbols()?
        .and_then(|symbols| symbols.union_payload_field(owner))
        .ok_or(BindingQueryError::Binding(
            BindingError::SymbolRecordUnavailable(owner.into()),
        ))
}

pub(super) fn union_variant<'binding>(
    context: &'binding CompilationBindingContext<'_>,
    owner: UnionVariantSymbolId,
) -> BindingQueryResult<&'binding bray_symbols::UnionVariantSymbol> {
    if let Some(record) = context.symbols().union_variant(owner) {
        return Ok(record);
    }

    context
        .imported_symbols()?
        .and_then(|symbols| symbols.union_variant(owner))
        .ok_or(BindingQueryError::Binding(
            BindingError::SymbolRecordUnavailable(owner.into()),
        ))
}
