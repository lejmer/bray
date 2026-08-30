use bray_binder::{BindingQueryContext, SymbolQueryProvider};
use bray_symbols::{
    ExactSymbolId, ImplementationCoherenceQuery, ImplementationSymbolId, SelfTypeContext,
    SemanticValueStore, SymbolQueryRequest, TraitSymbolId, TypeId,
};

use super::super::super::binder::{CompilationBindingContext, binding_query_error};
use super::super::super::implementation::implementation_instance_requirement;
use super::super::specialization::ConcreteCodegenInstance;
use crate::fact::FactQueryError;

pub(super) fn substitute_contextual_self(
    values: &SemanticValueStore,
    ty: TypeId,
    substitution: Option<(SelfTypeContext, TypeId)>,
) -> Result<TypeId, FactQueryError> {
    let Some((context, replacement)) = substitution else {
        return Ok(ty);
    };

    values
        .substitute_contextual_self(ty, context, replacement)
        .map_err(|_| FactQueryError::InfrastructureFailure)
}

pub(super) fn codegen_instance_contextual_self(
    binding_context: &CompilationBindingContext<'_>,
    instance: &ConcreteCodegenInstance,
) -> Result<Option<(SelfTypeContext, TypeId)>, FactQueryError> {
    if let Some(witness) = instance.contextual_self_witness() {
        let requirement = implementation_instance_requirement(binding_context, witness)?;

        let application = binding_context
            .semantic_values()
            .trait_application_data(requirement.trait_application())
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        if let Some(callable) = instance.callable_instance() {
            let container = binding_context
                .containing_symbol(callable.definition().symbol())
                .map_err(binding_query_error)?
                .ok_or(FactQueryError::InfrastructureFailure)?;

            let trait_definition = TraitSymbolId::try_from_any(container)
                .ok_or(FactQueryError::InfrastructureFailure)?;

            if trait_definition != application.definition() {
                return Err(FactQueryError::InfrastructureFailure);
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
        return Err(FactQueryError::InfrastructureFailure);
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

        let data = values
            .type_data(substituted)
            .unwrap_or_else(|error| panic!("test substituted type must resolve: {error:?}"));

        assert_eq!(data.as_ref(), &TypeData::Nullable(replacement));
    }
}
