use bray_compiler_known::RepresentationRole;
use bray_symbols::{
    GenericArgument, GenericOwnerId, GenericParameterSymbolId, GenericSubstitutionData,
    NamedTypeSymbolId, StructSymbolId, SymbolProvider, TypeData, TypeId, UnionSymbolId,
};

use crate::{CheckerInfrastructureError, CheckerRequestContext, CheckerUnitView};

pub(crate) fn type_representation<C>(
    request: CheckerUnitView<'_, C>,
    ty: TypeId,
) -> Result<Option<RepresentationRole>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    type_representation_for_context(request.context(), ty)
}

pub(crate) fn type_representation_for_context<C>(
    request: &C,
    ty: TypeId,
) -> Result<Option<RepresentationRole>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let data = request
        .semantic_values()
        .type_data(ty)
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    let TypeData::Named { definition, .. } = data.as_ref() else {
        return Ok(None);
    };

    let representation = match definition {
        NamedTypeSymbolId::Struct(definition) => request
            .available_compiler_known_symbols()
            .symbol_representation(*definition),
        NamedTypeSymbolId::Union(definition) => request
            .available_compiler_known_symbols()
            .symbol_representation(*definition),
    };

    Ok(representation)
}

pub(crate) fn representation_type<C>(
    request: CheckerUnitView<'_, C>,
    role: RepresentationRole,
) -> Result<TypeId, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(definition) = request
        .available_compiler_known_symbols()
        .representation_symbol::<StructSymbolId>(role)
    else {
        return Err(CheckerInfrastructureError::CompilerKnownRepresentationUnavailable { role });
    };

    named_type(request, NamedTypeSymbolId::Struct(definition))
}

pub(crate) fn representation_union_type<C>(
    request: CheckerUnitView<'_, C>,
    role: RepresentationRole,
    arguments: impl IntoIterator<Item = TypeId>,
) -> Result<TypeId, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(definition) = request
        .available_compiler_known_symbols()
        .representation_symbol::<UnionSymbolId>(role)
    else {
        return Err(CheckerInfrastructureError::CompilerKnownRepresentationUnavailable { role });
    };

    let Some(symbol) = SymbolProvider::<UnionSymbolId>::symbol(request.symbols(), definition)
    else {
        return Err(CheckerInfrastructureError::SemanticValueUnavailable);
    };

    if !symbol.generic_const_parameters().is_empty() {
        return Err(CheckerInfrastructureError::SemanticValueUnavailable);
    }

    let parameters = symbol
        .generic_type_parameters()
        .iter()
        .copied()
        .map(GenericParameterSymbolId::Type);

    let arguments = arguments.into_iter().map(GenericArgument::Type);

    let Some(owner) = GenericOwnerId::try_new(definition.into()) else {
        return Err(CheckerInfrastructureError::SemanticValueUnavailable);
    };

    let substitution = GenericSubstitutionData::try_new(owner, parameters, arguments)
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    let substitution = request
        .semantic_values()
        .intern_generic_substitution(substitution)
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    request
        .semantic_values()
        .intern_type(TypeData::Named {
            definition: NamedTypeSymbolId::Union(definition),
            substitution,
        })
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)
}

pub(crate) fn named_type<C>(
    request: CheckerUnitView<'_, C>,
    definition: NamedTypeSymbolId,
) -> Result<TypeId, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    intern_named_type(request.semantic_values(), definition)
}

pub(crate) fn intern_named_type(
    values: &bray_symbols::SemanticValueStore,
    definition: NamedTypeSymbolId,
) -> Result<TypeId, CheckerInfrastructureError> {
    values
        .intern_non_generic_named_type(definition)
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)
}
