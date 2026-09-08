use bray_binder::BindingQueryContext;
use bray_symbols::{
    AnySymbolId, GenericArgument, GenericOwnerId, GenericParameterSymbolId,
    GenericSubstitutionData, GenericSubstitutionId, NamedTypeSymbolId, SelfTypeContext,
    SemanticValueStore, TypeData, TypeId,
};

use super::binder::CompilationBindingContext;
use super::{
    SemanticDataKind, SemanticQueryContext, SemanticQueryFailure, SemanticQueryViolation,
    SemanticSymbolCategory,
};
use crate::fact::FactQueryError;

pub(super) fn generic_owner(symbol: AnySymbolId) -> Result<GenericOwnerId, SemanticQueryFailure> {
    GenericOwnerId::try_new(symbol).ok_or_else(|| {
        SemanticQueryFailure::contract(
            SemanticQueryContext::Symbol(symbol),
            SemanticQueryViolation::UnexpectedSymbolKind {
                expected: SemanticSymbolCategory::GenericOwner,
                actual: symbol.kind(),
            },
        )
    })
}

#[cfg(test)]
mod tests {
    use super::{
        GenericOwnerId, SemanticQueryContext, SemanticQueryFailure, SemanticQueryViolation,
        SemanticSymbolCategory, generic_owner,
    };

    #[test]
    fn generic_owners_preserve_the_rejected_symbol_and_category() {
        let compilation = crate::test_support::compilation("module app; func invoke() {}");
        let symbols = compilation.symbol_graph().unwrap();

        let function = symbols
            .functions()
            .iter()
            .find(|symbol| symbol.origin() == bray_symbols::SymbolOrigin::Source)
            .unwrap();

        let callable = function.id().into();

        assert_eq!(
            generic_owner(callable),
            Ok(GenericOwnerId::try_new(callable).unwrap())
        );

        let unsupported = symbols.compiler_known_environment().id().into();

        assert_eq!(
            generic_owner(unsupported),
            Err(SemanticQueryFailure::contract(
                SemanticQueryContext::Symbol(unsupported),
                SemanticQueryViolation::UnexpectedSymbolKind {
                    expected: SemanticSymbolCategory::GenericOwner,
                    actual: unsupported.kind(),
                },
            ))
        );
    }
}

pub(super) fn empty_substitution(
    values: &bray_symbols::SemanticValueStore,
    definition: AnySymbolId,
) -> Result<GenericSubstitutionId, FactQueryError> {
    let owner = generic_owner(definition)?;

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
    let owner = generic_owner(owner)?;

    let mut parameters = Vec::new();
    let mut arguments = Vec::new();

    for substitution in substitutions {
        let substitution = values
            .generic_substitution_data(substitution)
            .map_err(FactQueryError::SemanticValueStore)?;

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

    binding_context
        .semantic_values()
        .intern_open_named_type(binding_context.symbols(), definition)
        .map_err(FactQueryError::SemanticValueStore)
        .and_then(|ty| {
            ty.ok_or_else(|| {
                SemanticQueryFailure::contract(
                    SemanticQueryContext::Symbol(symbol),
                    SemanticQueryViolation::Missing(SemanticDataKind::Type),
                )
                .into()
            })
        })
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
