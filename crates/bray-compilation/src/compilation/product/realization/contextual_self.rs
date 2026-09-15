use bray_binder::{BindingQueryContext, SymbolQueryProvider};
use bray_symbols::{
    ExactSymbolId, GenericSubstitutionId, ImplementationCoherenceQuery, ImplementationSymbolId,
    SelfTypeContext, SemanticValueStore, SymbolQueryRequest, TraitApplicationId, TraitSymbolId,
    TypeId,
};

use super::super::super::binder::{CompilationBindingContext, binding_query_error};
use super::super::super::implementation::implementation_instance_requirement;
use super::super::specialization::ConcreteCodegenInstance;
use crate::compilation::{ProductDataKind, ProductQueryContext, ProductQueryFailure};
use crate::fact::FactQueryError;

pub(in crate::compilation::product) fn substitute_contextual_self(
    values: &SemanticValueStore,
    ty: TypeId,
    substitution: Option<(SelfTypeContext, TypeId)>,
) -> Result<TypeId, FactQueryError> {
    let Some((context, replacement)) = substitution else {
        return Ok(ty);
    };

    values
        .substitute_contextual_self(ty, context, replacement)
        .map_err(FactQueryError::SemanticValueStore)
}

pub(in crate::compilation::product) fn substitute_contextual_self_in_application(
    values: &SemanticValueStore,
    application: TraitApplicationId,
    substitution: Option<(SelfTypeContext, TypeId)>,
) -> Result<TraitApplicationId, FactQueryError> {
    let Some((context, replacement)) = substitution else {
        return Ok(application);
    };

    values
        .substitute_contextual_self_in_application(application, context, replacement)
        .map_err(FactQueryError::SemanticValueStore)
}

pub(in crate::compilation::product) fn substitute_contextual_self_in_substitution(
    values: &SemanticValueStore,
    substitution: GenericSubstitutionId,
    contextual_self: Option<(SelfTypeContext, TypeId)>,
) -> Result<GenericSubstitutionId, FactQueryError> {
    let Some((context, replacement)) = contextual_self else {
        return Ok(substitution);
    };

    values
        .substitute_contextual_self_in_substitution(substitution, context, replacement)
        .map_err(FactQueryError::SemanticValueStore)
}

pub(in crate::compilation::product) fn substitute_callable_instance(
    values: &SemanticValueStore,
    callable: bray_symbols::CallableInstanceData,
    owner_substitution: Option<GenericSubstitutionId>,
    contextual_self: Option<(SelfTypeContext, TypeId)>,
) -> Result<bray_symbols::CallableInstanceData, FactQueryError> {
    let substitution = match owner_substitution {
        Some(owner) => values
            .substitute_generic_substitution(callable.substitution(), owner)
            .map_err(FactQueryError::SemanticValueStore)?,
        None => callable.substitution(),
    };

    let substitution =
        substitute_contextual_self_in_substitution(values, substitution, contextual_self)?;

    Ok(bray_symbols::CallableInstanceData::new(
        callable.definition(),
        substitution,
    ))
}

pub(in crate::compilation::product) fn codegen_instance_contextual_self(
    binding_context: &CompilationBindingContext<'_>,
    instance: &ConcreteCodegenInstance,
) -> Result<Option<(SelfTypeContext, TypeId)>, FactQueryError> {
    if let Some(witness) = instance.contextual_self_witness() {
        let requirement = implementation_instance_requirement(binding_context, witness)?;

        let application = binding_context
            .semantic_values()
            .trait_application_data(requirement.trait_application());

        if let Some(callable) = instance.callable_instance() {
            let container = binding_context
                .containing_symbol(callable.definition().symbol())
                .map_err(binding_query_error)?
                .ok_or_else(|| {
                    ProductQueryFailure::missing(
                        ProductQueryContext::Instance(instance.key().clone()),
                        ProductDataKind::ContainingSymbol,
                    )
                })?;

            let trait_definition = TraitSymbolId::try_from_any(container).ok_or_else(|| {
                ProductQueryFailure::UnexpectedSymbolKind {
                    symbol: container,
                    expected: bray_symbols::SymbolKind::Trait,
                    actual: container.kind(),
                }
            })?;

            if trait_definition != application.definition() {
                return Err(ProductQueryFailure::TraitDefinitionMismatch {
                    context: ProductQueryContext::Instance(instance.key().clone()),
                    expected: application.definition(),
                    actual: trait_definition,
                }
                .into());
            }
        }

        return Ok(Some((
            SelfTypeContext::Trait(application.definition()),
            requirement.subject(),
        )));
    }

    let Some(callable) = instance.callable_instance() else {
        return Ok(None);
    };

    let Some(container) = binding_context
        .containing_symbol(callable.definition().symbol())
        .map_err(binding_query_error)?
    else {
        return Ok(None);
    };

    if let Some(implementation) = ImplementationSymbolId::try_from_any(container) {
        let subject = implementation_subject(binding_context, implementation)?;

        return Ok(Some((
            SelfTypeContext::Implementation(implementation),
            subject,
        )));
    }

    if TraitSymbolId::try_from_any(container).is_some() {
        return Err(ProductQueryFailure::missing(
            ProductQueryContext::Instance(instance.key().clone()),
            ProductDataKind::ContextualSelfWitness,
        )
        .into());
    }

    Ok(None)
}

pub(super) fn implementation_subject(
    binding_context: &CompilationBindingContext<'_>,
    implementation: ImplementationSymbolId,
) -> Result<TypeId, FactQueryError> {
    binding_context
        .resolve_symbol_query(SymbolQueryRequest::<ImplementationCoherenceQuery>::new(
            implementation,
        ))
        .map(|coherence| coherence.value().subject())
        .map_err(binding_query_error)
}

#[cfg(test)]
mod tests {
    use bray_symbols::{
        ImplementationSymbolId, InherentImplementationSymbolId, SelfTypeContext,
        SemanticValueStore, SymbolId, TypeData,
    };

    use super::substitute_contextual_self;

    #[test]
    fn codegen_contextual_self_substitution_reaches_nested_type_forms() {
        let values = SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("test semantic values must initialize: {error:?}"));

        let implementation = ImplementationSymbolId::Inherent(
            InherentImplementationSymbolId::from_symbol_id(SymbolId::new(1)),
        );

        let context = SelfTypeContext::Implementation(implementation);

        let contextual = values
            .intern_type(TypeData::ContextualSelf(context))
            .unwrap_or_else(|error| panic!("test contextual type must intern: {error:?}"));

        let nested = values
            .intern_type(TypeData::Nullable(contextual))
            .unwrap_or_else(|error| panic!("test nested type must intern: {error:?}"));

        let replacement = values
            .intern_type(TypeData::tuple([]))
            .unwrap_or_else(|error| panic!("test replacement type must intern: {error:?}"));

        let substituted = substitute_contextual_self(&values, nested, Some((context, replacement)))
            .unwrap_or_else(|error| panic!("test contextual type must substitute: {error:?}"));

        let data = values.type_data(substituted);

        assert_eq!(data.as_ref(), &TypeData::Nullable(replacement));
    }
}
