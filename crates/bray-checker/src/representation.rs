use std::collections::BTreeSet;

use bray_compiler_known::RepresentationRole;
use bray_symbols::{
    AvailableCompilerKnownSymbols, GenericArgument, GenericOwnerId, GenericParameterSymbolId,
    GenericSubstitutionData, NamedTypeSymbolId, SemanticValueStore, StructSymbolId, SymbolProvider,
    TypeData, TypeId, UnionSymbolId,
};

use crate::{CheckerInfrastructureError, CheckerQueryError, CheckerRequestContext, CheckerUnitView};

pub(crate) fn type_supports_complete_fixed_layout<C>(
    request: CheckerUnitView<'_, C>,
    ty: TypeId,
) -> Result<bool, CheckerQueryError>
where
    C: CheckerRequestContext + ?Sized,
{
    type_supports_complete_fixed_layout_inner(request, ty, &mut BTreeSet::new())
}

fn type_supports_complete_fixed_layout_inner<C>(
    request: CheckerUnitView<'_, C>,
    ty: TypeId,
    pending: &mut BTreeSet<TypeId>,
) -> Result<bool, CheckerQueryError>
where
    C: CheckerRequestContext + ?Sized,
{
    if !pending.insert(ty) {
        return Ok(false);
    }

    let data = request
        .semantic_values()
        .type_data(ty)
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    let complete = match data.as_ref() {
        TypeData::Named { definition, .. } => {
            match type_representation(request, ty)? {
                Some(RepresentationRole::Uninit) => {
                    let element = request
                        .available_compiler_known_symbols()
                        .unary_representation_argument(
                            request.semantic_values(),
                            RepresentationRole::Uninit,
                            ty,
                        )
                        .ok_or(CheckerInfrastructureError::InvalidSemanticSelectionInput)?;

                    type_supports_complete_fixed_layout_inner(request, element, pending)?
                }
                Some(_) => true,
                None => {
                    let representation = request.declared_type_representation(*definition)?;

                    !representation.value().is_incomplete()
                        && representation.value().has_finite_size()
                }
            }
        }
        TypeData::Tuple(elements) => {
            let mut complete = true;

            for element in elements.iter().copied() {
                complete &= type_supports_complete_fixed_layout_inner(request, element, pending)?;
            }

            complete
        }
        TypeData::Array { element, .. }
        | TypeData::Generator(element)
        | TypeData::Nullable(element) => {
            type_supports_complete_fixed_layout_inner(request, *element, pending)?
        }
        TypeData::Borrow { .. }
        | TypeData::OwnedIndirection { .. }
        | TypeData::Callable(_) => true,
        TypeData::TypeParameter(_)
        | TypeData::ContextualSelf(_)
        | TypeData::TypeValuedMemberProjection { .. }
        | TypeData::Error => true,
        TypeData::FlexibleArray(_) | TypeData::Slice(_) | TypeData::TraitView(_) => false,
    };

    pending.remove(&ty);

    Ok(complete)
}

pub(crate) fn type_supports_flexible_c_layout<C>(
    request: CheckerUnitView<'_, C>,
    ty: TypeId,
) -> Result<bool, CheckerQueryError>
where
    C: CheckerRequestContext + ?Sized,
{
    let data = request
        .semantic_values()
        .type_data(ty)
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    let TypeData::Named { definition, .. } = data.as_ref() else {
        return Ok(matches!(
            data.as_ref(),
            TypeData::TypeParameter(_)
                | TypeData::ContextualSelf(_)
                | TypeData::TypeValuedMemberProjection { .. }
                | TypeData::Error
        ));
    };

    let representation = request.declared_type_representation(*definition)?;
    let representation = representation.value();

    Ok(representation.layout() == bray_symbols::DeclaredLayoutMode::C
        && !representation.is_incomplete()
        && representation.has_flexible_trailing_member())
}

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
    type_representation_for_values(
        request.semantic_values(),
        request.available_compiler_known_symbols(),
        ty,
    )
}

pub(crate) fn type_representation_for_values(
    values: &SemanticValueStore,
    available: &AvailableCompilerKnownSymbols,
    ty: TypeId,
) -> Result<Option<RepresentationRole>, CheckerInfrastructureError> {
    let data = values
        .type_data(ty)
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    let TypeData::Named { definition, .. } = data.as_ref() else {
        return Ok(None);
    };

    let representation = match definition {
        NamedTypeSymbolId::Struct(definition) => available.symbol_representation(*definition),
        NamedTypeSymbolId::Union(definition) => available.symbol_representation(*definition),
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
    representation_type_for_context(request.context(), role)
}

pub(crate) fn representation_type_for_context<C>(
    request: &C,
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

    intern_named_type(
        request.semantic_values(),
        NamedTypeSymbolId::Struct(definition),
    )
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
    values: &SemanticValueStore,
    definition: NamedTypeSymbolId,
) -> Result<TypeId, CheckerInfrastructureError> {
    values
        .intern_non_generic_named_type(definition)
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)
}
