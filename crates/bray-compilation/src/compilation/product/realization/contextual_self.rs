use bray_binder::{BindingQueryContext, SymbolQueryProvider};
use bray_symbols::{
    ExactSymbolId, ImplementationCoherenceQuery, ImplementationSymbolId, SelfTypeContext,
    SymbolQueryRequest, TraitSymbolId, TypeId,
};

use super::super::super::binder::{CompilationBindingContext, binding_query_error};
use super::super::super::implementation::implementation_instance_requirement;
use super::super::specialization::ConcreteCodegenInstance;
use crate::compilation::{ProductDataKind, ProductQueryContext, ProductQueryFailure};
use crate::fact::FactQueryError;

pub(in crate::compilation::product) fn codegen_instance_contextual_self(
    binding_context: &CompilationBindingContext<'_>,
    instance: &ConcreteCodegenInstance,
) -> Result<Option<(SelfTypeContext, TypeId)>, FactQueryError> {
    if let Some(witness) = instance.contextual_self_witness() {
        let requirement = implementation_instance_requirement(binding_context, witness)?;

        let application = binding_context
            .semantic_values()
            .trait_application_data(requirement.trait_application())
            .map_err(FactQueryError::SemanticValueStore)?;

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
