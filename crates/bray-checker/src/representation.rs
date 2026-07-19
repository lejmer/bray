use bray_compiler_known::RepresentationRole;
use bray_symbols::{
    GenericArgument, GenericOwnerId, GenericParameterSymbolId, GenericSubstitutionData,
    NamedTypeSymbolId, StructSymbolId, TypeData, TypeId,
};

use crate::{CheckerInfrastructureError, CheckerRequestContext, UnitCheckRequest};

pub(crate) fn type_representation<C>(
    request: UnitCheckRequest<'_, C>,
    ty: TypeId,
) -> Result<Option<RepresentationRole>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let data = request
        .semantic_values()
        .type_data(ty)
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    let TypeData::Named {
        definition: NamedTypeSymbolId::Struct(definition),
        ..
    } = data.as_ref()
    else {
        return Ok(None);
    };

    Ok(request
        .available_compiler_known_symbols()
        .symbol_representation(*definition))
}

pub(crate) fn representation_type<C>(
    request: UnitCheckRequest<'_, C>,
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

pub(crate) fn named_type<C>(
    request: UnitCheckRequest<'_, C>,
    definition: NamedTypeSymbolId,
) -> Result<TypeId, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(owner) = GenericOwnerId::try_new(definition.into_any()) else {
        return Err(CheckerInfrastructureError::SemanticValueUnavailable);
    };

    let substitution = GenericSubstitutionData::try_new(
        owner,
        std::iter::empty::<GenericParameterSymbolId>(),
        std::iter::empty::<GenericArgument>(),
    )
    .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    let substitution = request
        .semantic_values()
        .intern_generic_substitution(substitution)
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    request
        .semantic_values()
        .intern_type(TypeData::Named {
            definition,
            substitution,
        })
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)
}
