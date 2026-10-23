use bray_binder::{BindingQueryContext, SymbolQueryProvider};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{
    AnySymbolId, CallableDefinitionId, CallableInstanceData, CallableSignatureQuery,
    ExternalDeclarationIdentity, ExternalSymbolKeyData, GenericArgument, GenericOwnerId,
    GenericSubstitutionData, ImplementationCoherenceQuery, ImplementationInstanceId,
    ImplementationRequirementKey, ImplementationSymbolId, SymbolKeyData, SymbolQueryRequest,
    TraitApplicationData, TraitCallableFulfillmentSymbolId, TraitCallableMemberSymbolId,
    TraitSymbolId, TraitTypeFulfillmentSymbolId, TraitTypeFulfillmentValueQuery,
    TraitTypeMemberSymbolId, TypeId,
};

use super::super::binder::{CompilationBindingContext, binding_query_error};
use crate::fact::FactQueryError;

#[derive(Clone, Copy)]
pub(in crate::compilation) struct ImplementationFulfillments<'symbols> {
    pub(in crate::compilation) callables: &'symbols [TraitCallableFulfillmentSymbolId],
    pub(in crate::compilation) types: &'symbols [TraitTypeFulfillmentSymbolId],
}

pub(in crate::compilation) enum TypeValuedMemberResolution {
    Resolved(TypeId),
    Invalid,
    Deferred,
}

pub(in crate::compilation) fn implementation_requirement(
    values: &bray_symbols::SemanticValueStore,
    subject: TypeId,
    definition: TraitSymbolId,
    parameters: impl IntoIterator<Item = bray_symbols::GenericParameterSymbolId>,
    arguments: impl IntoIterator<Item = GenericArgument>,
) -> Result<bray_symbols::ImplementationRequirementKey, FactQueryError> {
    let symbol = definition.into();

    let owner = GenericOwnerId::try_new(symbol).ok_or_else(|| {
        crate::compilation::SemanticQueryFailure::contract(
            crate::compilation::SemanticQueryContext::Symbol(symbol),
            crate::compilation::SemanticQueryViolation::UnexpectedSymbolKind {
                expected: crate::compilation::SemanticSymbolCategory::GenericOwner,
                actual: symbol.kind(),
            },
        )
    })?;

    let substitution =
        GenericSubstitutionData::try_new(owner, parameters, arguments).map_err(|cause| {
            crate::compilation::SemanticQueryFailure::GenericSubstitution {
                owner: Some(owner),
                cause,
            }
        })?;

    let substitution = values
        .intern_generic_substitution(substitution)
        .map_err(FactQueryError::SemanticValueStore)?;

    let application = values
        .intern_trait_application(TraitApplicationData::new(definition, substitution))
        .map_err(FactQueryError::SemanticValueStore)?;

    Ok(bray_symbols::ImplementationRequirementKey::new(
        subject,
        application,
    ))
}

pub(in crate::compilation) fn implementation_instance_requirement(
    binding_context: &CompilationBindingContext<'_>,
    instance: ImplementationInstanceId,
) -> Result<ImplementationRequirementKey, FactQueryError> {
    let values = binding_context.semantic_values();

    let instance = values.implementation_instance_data(instance);

    let coherence = binding_context
        .resolve_symbol_query(SymbolQueryRequest::<ImplementationCoherenceQuery>::new(
            instance.definition(),
        ))
        .map_err(binding_query_error)?;

    let application = coherence.value().trait_application().ok_or_else(|| {
        crate::compilation::SemanticQueryFailure::contract(
            crate::compilation::SemanticQueryContext::Symbol(instance.definition().into_any()),
            crate::compilation::SemanticQueryViolation::Missing(
                crate::compilation::SemanticDataKind::TraitApplication,
            ),
        )
    })?;

    let subject = values
        .substitute_type(coherence.value().subject(), instance.substitution())
        .map_err(FactQueryError::SemanticValueStore)?;

    let application = values
        .substitute_trait_application(application, instance.substitution())
        .map_err(FactQueryError::SemanticValueStore)?;

    Ok(ImplementationRequirementKey::new(subject, application))
}

pub(in crate::compilation) fn selected_type_valued_member(
    binding_context: &CompilationBindingContext<'_>,
    substitution: bray_symbols::GenericSubstitutionId,
    fulfillments: &[TraitTypeFulfillmentSymbolId],
    member: TraitTypeMemberSymbolId,
    diagnostics: &mut DiagnosticBag,
) -> Result<TypeValuedMemberResolution, FactQueryError> {
    let expected_name = fulfillment_name(binding_context, member.into())?.ok_or_else(|| {
        crate::compilation::SemanticQueryFailure::contract(
            crate::compilation::SemanticQueryContext::Symbol(member.into()),
            crate::compilation::SemanticQueryViolation::Missing(
                crate::compilation::SemanticDataKind::MemberName,
            ),
        )
    })?;

    let Some(fulfillment) = matching_fulfillment(binding_context, fulfillments, expected_name)?
    else {
        return Ok(TypeValuedMemberResolution::Invalid);
    };

    let result = binding_context
        .resolve_symbol_query(SymbolQueryRequest::<TraitTypeFulfillmentValueQuery>::new(
            fulfillment,
        ))
        .map_err(binding_query_error)?;

    *diagnostics = diagnostics.merged(result.diagnostics());

    if result.diagnostics().has_errors() {
        return Ok(TypeValuedMemberResolution::Invalid);
    }

    let checked = binding_context
        .compilation()
        .checked_constant_terms(result.value())?;

    *diagnostics = diagnostics.merged(checked.diagnostics());

    let Some(ty) = bray_checker::resolve_type_expression_template(
        binding_context.semantic_values(),
        result.value(),
        checked.value(),
    ) else {
        return Ok(TypeValuedMemberResolution::Deferred);
    };

    binding_context
        .semantic_values()
        .substitute_type(ty, substitution)
        .map(TypeValuedMemberResolution::Resolved)
        .map_err(FactQueryError::SemanticValueStore)
}

pub(in crate::compilation) fn implementation_fulfillments<'binding_context>(
    binding_context: &'binding_context CompilationBindingContext<'_>,
    implementation: ImplementationSymbolId,
) -> Result<ImplementationFulfillments<'binding_context>, FactQueryError> {
    let source = match implementation {
        ImplementationSymbolId::Inherent(id) => binding_context
            .symbols()
            .inherent_implementation(id)
            .map(|symbol| ImplementationFulfillments {
                callables: symbol.callable_fulfillments(),
                types: symbol.type_fulfillments(),
            }),
        ImplementationSymbolId::UnnamedTrait(id) => binding_context
            .symbols()
            .unnamed_trait_implementation(id)
            .map(|symbol| ImplementationFulfillments {
                callables: symbol.callable_fulfillments(),
                types: symbol.type_fulfillments(),
            }),
        ImplementationSymbolId::NamedTrait(id) => binding_context
            .symbols()
            .named_trait_implementation(id)
            .map(|symbol| ImplementationFulfillments {
                callables: symbol.callable_fulfillments(),
                types: symbol.type_fulfillments(),
            }),
    };

    if let Some(fulfillments) = source {
        return Ok(fulfillments);
    }

    let imported = binding_context
        .imported_symbols()
        .map_err(binding_query_error)?;

    let imported = match implementation {
        ImplementationSymbolId::Inherent(id) => imported
            .and_then(|symbols| symbols.inherent_implementation(id))
            .map(|symbol| ImplementationFulfillments {
                callables: symbol.callable_fulfillments(),
                types: symbol.type_fulfillments(),
            }),
        ImplementationSymbolId::UnnamedTrait(id) => imported
            .and_then(|symbols| symbols.unnamed_trait_implementation(id))
            .map(|symbol| ImplementationFulfillments {
                callables: symbol.callable_fulfillments(),
                types: symbol.type_fulfillments(),
            }),
        ImplementationSymbolId::NamedTrait(id) => imported
            .and_then(|symbols| symbols.named_trait_implementation(id))
            .map(|symbol| ImplementationFulfillments {
                callables: symbol.callable_fulfillments(),
                types: symbol.type_fulfillments(),
            }),
    };

    imported.ok_or_else(|| {
        crate::compilation::SemanticQueryFailure::contract(
            crate::compilation::SemanticQueryContext::Symbol(implementation.into_any()),
            crate::compilation::SemanticQueryViolation::Missing(
                crate::compilation::SemanticDataKind::Implementation,
            ),
        )
        .into()
    })
}

pub(in crate::compilation) fn selected_callable(
    binding_context: &CompilationBindingContext<'_>,
    fulfillments: &[TraitCallableFulfillmentSymbolId],
    member: TraitCallableMemberSymbolId,
) -> Result<Option<TraitCallableFulfillmentSymbolId>, FactQueryError> {
    let name = fulfillment_name(binding_context, member.into())?
        .expect("checked trait callable member retains its name");

    matching_fulfillment(binding_context, fulfillments, name)
}

fn matching_fulfillment<T: Copy + Into<AnySymbolId>>(
    binding_context: &CompilationBindingContext<'_>,
    fulfillments: &[T],
    expected_name: &str,
) -> Result<Option<T>, FactQueryError> {
    let mut selected = None;

    for fulfillment in fulfillments.iter().copied() {
        if fulfillment_name(binding_context, fulfillment.into())? != Some(expected_name) {
            continue;
        }

        if selected.is_some() {
            return Ok(None);
        }

        selected = Some(fulfillment);
    }

    Ok(selected)
}

#[derive(Clone, Copy)]
pub(in crate::compilation) struct ImplementationCallableInstance {
    instance: CallableInstanceData,
    uses_trait_default: bool,
}

impl ImplementationCallableInstance {
    pub(in crate::compilation) const fn new(
        instance: CallableInstanceData,
        uses_trait_default: bool,
    ) -> Self {
        Self {
            instance,
            uses_trait_default,
        }
    }

    pub(in crate::compilation) const fn instance(self) -> CallableInstanceData {
        self.instance
    }

    pub(in crate::compilation) const fn uses_trait_default(self) -> bool {
        self.uses_trait_default
    }
}

pub(in crate::compilation) fn implementation_callable_instance(
    binding_context: &CompilationBindingContext<'_>,
    fulfillments: &[TraitCallableFulfillmentSymbolId],
    member: TraitCallableMemberSymbolId,
    trait_substitution: bray_symbols::GenericSubstitutionId,
    implementation_substitution: bray_symbols::GenericSubstitutionId,
) -> Result<Option<ImplementationCallableInstance>, FactQueryError> {
    let values = binding_context.semantic_values();

    match selected_callable(binding_context, fulfillments, member)? {
        Some(fulfillment) => callable_instance(
            values,
            fulfillment.into(),
            [trait_substitution, implementation_substitution],
        )
        .map(|instance| Some(ImplementationCallableInstance::new(instance, false))),
        None => {
            let signature = binding_context
                .resolve_symbol_query(SymbolQueryRequest::<CallableSignatureQuery>::new(
                    member.into(),
                ))
                .map_err(binding_query_error)?;

            if !signature.value().has_body() {
                return Ok(None);
            }

            callable_instance(values, member.into(), [trait_substitution])
                .map(|instance| Some(ImplementationCallableInstance::new(instance, true)))
        }
    }
}

/// Transfers a trait member's inferred arguments to the selected implementation's parameters.
pub(in crate::compilation) fn instantiate_implementation_member(
    binding_context: &CompilationBindingContext<'_>,
    selected: CallableInstanceData,
    member: CallableInstanceData,
) -> Result<CallableInstanceData, FactQueryError> {
    let values = binding_context.semantic_values();

    let member_substitution = values.generic_substitution_data(member.substitution());

    let selected_substitution = values.generic_substitution_data(selected.substitution());

    let member_parameters = binding_context
        .resolve_symbol_query(SymbolQueryRequest::<
            bray_symbols::GenericDeclarationTemplateQuery,
        >::new(member_substitution.owner()))
        .map_err(binding_query_error)?;

    let selected_parameters = binding_context
        .resolve_symbol_query(SymbolQueryRequest::<
            bray_symbols::GenericDeclarationTemplateQuery,
        >::new(selected_substitution.owner()))
        .map_err(binding_query_error)?;

    let arguments = member_parameters
        .value()
        .parameters()
        .iter()
        .map(
            |parameter| match member_substitution.argument_for(*parameter) {
                Some(argument) => Ok(argument),
                None => values
                    .intern_generic_parameter_argument(*parameter)
                    .map_err(FactQueryError::SemanticValueStore),
            },
        )
        .collect::<Result<Vec<_>, _>>()?;

    let substitution = GenericSubstitutionData::try_new(
        selected_substitution.owner(),
        selected_parameters.value().parameters().iter().copied(),
        arguments,
    )
    .map_err(
        |cause| crate::compilation::SemanticQueryFailure::GenericSubstitution {
            owner: Some(selected_substitution.owner()),
            cause,
        },
    )?;

    let substitution = values
        .intern_generic_substitution(substitution)
        .map_err(FactQueryError::SemanticValueStore)?;

    callable_instance(
        values,
        selected.definition().symbol(),
        [selected.substitution(), substitution],
    )
}

pub(in crate::compilation) fn callable_instance(
    values: &bray_symbols::SemanticValueStore,
    callable: AnySymbolId,
    substitutions: impl IntoIterator<Item = bray_symbols::GenericSubstitutionId>,
) -> Result<CallableInstanceData, FactQueryError> {
    let definition = CallableDefinitionId::try_new(callable).ok_or_else(|| {
        crate::compilation::SemanticQueryFailure::contract(
            crate::compilation::SemanticQueryContext::Symbol(callable),
            crate::compilation::SemanticQueryViolation::UnexpectedSymbolKind {
                expected: crate::compilation::SemanticSymbolCategory::Callable,
                actual: callable.kind(),
            },
        )
    })?;

    let substitution =
        super::super::substitution::substitution_for_owner(values, callable, substitutions)?;

    Ok(CallableInstanceData::new(definition, substitution))
}

fn fulfillment_name<'binding_context>(
    binding_context: &'binding_context CompilationBindingContext<'_>,
    fulfillment: AnySymbolId,
) -> Result<Option<&'binding_context str>, FactQueryError> {
    if let Some(name) = binding_context.symbols().member_name(fulfillment) {
        return Ok(Some(name.as_str()));
    }

    let Some(key) = binding_context
        .symbol_key(fulfillment)
        .map_err(binding_query_error)?
    else {
        return Ok(None);
    };

    let SymbolKeyData::External(key) = key.data() else {
        return Ok(None);
    };

    let ExternalSymbolKeyData::Declaration {
        identity: ExternalDeclarationIdentity::Name(name),
        ..
    } = key.data()
    else {
        return Ok(None);
    };

    Ok(Some(name.as_str()))
}
