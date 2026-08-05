use bray_binder::BinderFactContext;
use bray_symbols::{
    AnySymbolId, GenericArgument, GenericOwnerId, GenericParameterSymbolId,
    GenericSubstitutionData, GenericSubstitutionId, NamedTypeSymbolId, SelfTypeContext,
    SemanticValueStore, TypeData, TypeId,
};

use super::binder::CompilationBinderFacts;
use crate::fact::FactQueryError;

pub(super) fn empty_substitution(
    values: &bray_symbols::SemanticValueStore,
    definition: AnySymbolId,
) -> Result<GenericSubstitutionId, FactQueryError> {
    let owner = GenericOwnerId::try_new(definition).ok_or(FactQueryError::InfrastructureFailure)?;

    let substitution = GenericSubstitutionData::try_new(owner, [], [])
        .map_err(|_| FactQueryError::InfrastructureFailure)?;

    values
        .intern_generic_substitution(substitution)
        .map_err(|_| FactQueryError::InfrastructureFailure)
}

pub(super) fn substitution_for_owner(
    values: &SemanticValueStore,
    owner: AnySymbolId,
    substitutions: impl IntoIterator<Item = GenericSubstitutionId>,
) -> Result<GenericSubstitutionId, FactQueryError> {
    let owner = GenericOwnerId::try_new(owner).ok_or(FactQueryError::InfrastructureFailure)?;
    let mut parameters = Vec::new();
    let mut arguments = Vec::new();

    for substitution in substitutions {
        let substitution = values
            .generic_substitution_data(substitution)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        for binding in substitution.bindings() {
            parameters.push(binding.parameter());
            arguments.push(binding.argument());
        }
    }

    let substitution = GenericSubstitutionData::try_new(owner, parameters, arguments)
        .map_err(|_| FactQueryError::InfrastructureFailure)?;

    values
        .intern_generic_substitution(substitution)
        .map_err(|_| FactQueryError::InfrastructureFailure)
}

pub(super) fn generic_parameter_argument(
    values: &SemanticValueStore,
    parameter: GenericParameterSymbolId,
) -> Result<GenericArgument, FactQueryError> {
    values
        .intern_generic_parameter_argument(parameter)
        .map_err(|_| FactQueryError::InfrastructureFailure)
}

pub(super) fn contextual_self_type(
    facts: &CompilationBinderFacts<'_>,
    context: SelfTypeContext,
) -> Result<TypeId, FactQueryError> {
    let definition = NamedTypeSymbolId::try_from_any(context.symbol())
        .ok_or(FactQueryError::InfrastructureFailure)?;

    facts
        .semantic_values()
        .intern_open_named_type(facts.symbols(), definition)
        .map_err(|_| FactQueryError::InfrastructureFailure)
        .and_then(|ty| ty.ok_or(FactQueryError::InfrastructureFailure))
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
        .map_err(|_| FactQueryError::InfrastructureFailure)
}
