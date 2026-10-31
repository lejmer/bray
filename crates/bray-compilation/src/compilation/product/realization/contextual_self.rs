use std::collections::BTreeMap;

use bray_binder::{BindingQueryContext, SymbolQueryProvider};
use bray_symbols::{
    ExactSymbolId, GenericSubstitutionId, ImplementationCoherenceQuery, ImplementationSymbolId,
    SelfTypeContext, SemanticValueStore, SymbolQueryRequest, TraitApplicationId, TraitSymbolId,
    TypeId,
};

use super::super::super::binder::{CompilationBindingContext, binding_query_error};
use super::super::super::implementation::implementation_instance_requirement;
use super::super::specialization::ConcreteCodegenInstance;
use crate::compilation::{
    CodegenPreparationError, Compilation, ProductDataKind, ProductQueryContext, ProductQueryFailure,
};
use crate::fact::{CancellationToken, FactQueryError};

impl Compilation {
    pub(in crate::compilation::product) fn concrete_mir_callables(
        &self,
        instance: &ConcreteCodegenInstance,
        unit: &bray_ir::MirUnit,
        cancellation: &CancellationToken,
    ) -> Result<
        BTreeMap<bray_ir::MirCallableReference, bray_ir::MirCallableReference>,
        CodegenPreparationError,
    > {
        let values = self.semantic_value_store()?;
        let binding = self.binding_context(cancellation)?;
        let contextual_self = codegen_instance_contextual_self(&binding, instance)?;
        let mut callables = BTreeMap::new();

        for demand in bray_codegen::demanded_callable_instances_for_mir(unit) {
            let reference = demand.reference();

            let callable = substitute_callable_instance(
                values,
                reference.instance(),
                instance.substitution(),
                contextual_self,
            )?;

            let concrete = bray_ir::MirCallableReference::new(callable, reference.abi());

            if concrete != reference {
                callables.insert(reference, concrete);
            }
        }

        Ok(callables)
    }
}

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

            let trait_definition = TraitSymbolId::try_from_any(container).unwrap_or_else(|| {
                panic!("product UnexpectedSymbolKind construction failed: symbol={:?}, expected={:?}, actual={:?}", container, bray_symbols::SymbolKind::Trait, container.kind())
            });

            if trait_definition != application.definition() {
                panic!(
                    "product TraitDefinitionMismatch construction failed: context={:?}, expected={:?}, actual={:?}",
                    instance.key(),
                    application.definition(),
                    trait_definition
                );
            }
        }

        return Ok(Some((
            SelfTypeContext::Trait(application.definition()),
            requirement.subject(),
        )));
    }

    let values = binding_context.semantic_values();

    let declaration = match instance.callable_instance() {
        Some(callable) => callable.definition().symbol(),
        None => {
            let Some(substitution) = instance.substitution() else {
                return Ok(None);
            };

            values
                .generic_substitution_data(substitution)
                .owner()
                .symbol()
        }
    };

    let context = SelfTypeContext::try_new(declaration).or_else(|| {
        binding_context
            .symbols()
            .enclosing_self_type_context(declaration)
    });

    match context {
        Some(SelfTypeContext::Implementation(implementation)) => {
            let subject = implementation_subject(binding_context, implementation)?;

            let subject = match instance.substitution() {
                Some(substitution) => values
                    .substitute_type(subject, substitution)
                    .map_err(FactQueryError::SemanticValueStore)?,
                None => subject,
            };

            return Ok(Some((
                SelfTypeContext::Implementation(implementation),
                subject,
            )));
        }
        Some(SelfTypeContext::Trait(_)) => {
            return Err(ProductQueryFailure::missing(
                ProductQueryContext::Instance(instance.key().clone()),
                ProductDataKind::ContextualSelfWitness,
            )
            .into());
        }
        Some(SelfTypeContext::NamedType(_)) | None => {}
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
