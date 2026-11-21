use bray_binder::{BindingQueryContext, SymbolQueryProvider};
use bray_symbols::{
    AnySymbolId, GenericArgument, GenericDeclarationTemplateQuery, GenericOwnerId, GenericParameterSymbolId,
    GenericSubstitutionData, GenericSubstitutionId, NamedTypeSymbolId, SelfTypeContext,
    SemanticValueStore, SymbolQueryRequest, TypeData, TypeId,
};

use super::binder::CompilationBindingContext;
use super::{
    SemanticQueryContext, SemanticQueryFailure, SemanticQueryViolation, SemanticSymbolCategory,
};
use crate::fact::FactQueryError;

pub(super) fn empty_substitution(
    values: &bray_symbols::SemanticValueStore,
    definition: AnySymbolId,
) -> Result<GenericSubstitutionId, FactQueryError> {
    let owner = GenericOwnerId::try_new(definition).ok_or_else(|| {
        SemanticQueryFailure::contract(
            SemanticQueryContext::Symbol(definition),
            SemanticQueryViolation::UnexpectedSymbolKind {
                expected: SemanticSymbolCategory::GenericOwner,
                actual: definition.kind(),
            },
        )
    })?;

    let substitution = GenericSubstitutionData::try_new(owner, [], []).map_err(|cause| {
        SemanticQueryFailure::GenericSubstitution {
            owner: Some(owner),
            cause,
        }
    })?;

    values
        .intern_generic_substitution(substitution)
        .map_err(FactQueryError::SemanticValueStore)
}

pub(super) fn substitution_for_owner(
    values: &SemanticValueStore,
    owner: AnySymbolId,
    substitutions: impl IntoIterator<Item = GenericSubstitutionId>,
) -> Result<GenericSubstitutionId, FactQueryError> {
    let symbol = owner;

    let owner = GenericOwnerId::try_new(symbol).ok_or_else(|| {
        SemanticQueryFailure::contract(
            SemanticQueryContext::Symbol(symbol),
            SemanticQueryViolation::UnexpectedSymbolKind {
                expected: SemanticSymbolCategory::GenericOwner,
                actual: symbol.kind(),
            },
        )
    })?;

    let mut parameters = Vec::new();
    let mut arguments = Vec::new();

    for substitution in substitutions {
        let substitution = values.generic_substitution_data(substitution);

        for binding in substitution.bindings() {
            parameters.push(binding.parameter());
            arguments.push(binding.argument());
        }
    }

    let substitution =
        GenericSubstitutionData::try_new(owner, parameters, arguments).map_err(|cause| {
            SemanticQueryFailure::GenericSubstitution {
                owner: Some(owner),
                cause,
            }
        })?;

    values
        .intern_generic_substitution(substitution)
        .map_err(FactQueryError::SemanticValueStore)
}

pub(super) fn generic_parameter_argument(
    values: &SemanticValueStore,
    parameter: GenericParameterSymbolId,
) -> Result<GenericArgument, FactQueryError> {
    values
        .intern_generic_parameter_argument(parameter)
        .map_err(FactQueryError::SemanticValueStore)
}

pub(super) fn identity_substitution(
    values: &SemanticValueStore,
    owner: GenericOwnerId,
    parameters: &[GenericParameterSymbolId],
) -> Result<GenericSubstitutionId, FactQueryError> {
    let arguments = parameters
        .iter()
        .copied()
        .map(|parameter| generic_parameter_argument(values, parameter))
        .collect::<Result<Vec<_>, _>>()?;

    let substitution =
        GenericSubstitutionData::try_new(owner, parameters.iter().copied(), arguments).map_err(
            |cause| SemanticQueryFailure::GenericSubstitution {
                owner: Some(owner),
                cause,
            },
        )?;

    values
        .intern_generic_substitution(substitution)
        .map_err(FactQueryError::SemanticValueStore)
}

pub(super) fn contextual_self_type(
    binding_context: &CompilationBindingContext<'_>,
    context: SelfTypeContext,
) -> Result<TypeId, FactQueryError> {
    let symbol = context.symbol();

    let definition = NamedTypeSymbolId::try_from_any(symbol).ok_or_else(|| {
        SemanticQueryFailure::contract(
            SemanticQueryContext::Symbol(symbol),
            SemanticQueryViolation::UnexpectedSymbolKind {
                expected: SemanticSymbolCategory::NamedType,
                actual: symbol.kind(),
            },
        )
    })?;

    open_named_type(binding_context, definition)
}

pub(super) fn open_named_type(
    binding_context: &CompilationBindingContext<'_>,
    definition: NamedTypeSymbolId,
) -> Result<TypeId, FactQueryError> {
    let values = binding_context.semantic_values();

    if let Some(ty) = values.intern_open_named_type(binding_context.symbols(), definition)
        .map_err(FactQueryError::SemanticValueStore)? {
        return Ok(ty);
    }

    let owner = GenericOwnerId::try_new(definition.into_any()).expect("named types own generic parameters");

    let declaration = binding_context.resolve_symbol_query(SymbolQueryRequest::<GenericDeclarationTemplateQuery>::new(owner))
        .map_err(super::binder::binding_query_error)?;

    values.intern_open_named_type_from_parameters(definition, declaration.value().parameters())
        .map_err(FactQueryError::SemanticValueStore)
}

pub(super) fn named_type(
    values: &SemanticValueStore,
    definition: NamedTypeSymbolId,
) -> Result<TypeId, FactQueryError> {
    let substitution = empty_substitution(values, definition.into_any())?;

    values
        .intern_type(TypeData::Named {
            definition,
            substitution,
        })
        .map_err(FactQueryError::SemanticValueStore)
}
