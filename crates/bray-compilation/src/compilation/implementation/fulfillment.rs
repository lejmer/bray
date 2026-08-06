use bray_binder::{BinderFactContext, SymbolFactProvider};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{
    AnySymbolId, CallableDefinitionId, CallableInstanceData, ExternalDeclarationIdentity,
    ExternalSymbolKeyData, GenericArgument, GenericOwnerId, GenericSubstitutionData,
    ImplementationSymbolId, SymbolFactRequest, SymbolKeyData, TraitApplicationData,
    TraitCallableFulfillmentSymbolId, TraitCallableMemberSymbolId, TraitSymbolId,
    TraitTypeFulfillmentSymbolId, TraitTypeFulfillmentValueFact, TraitTypeMemberSymbolId, TypeId,
};

use super::super::binder::{CompilationBinderFacts, binder_fact_error};
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
    let owner =
        GenericOwnerId::try_new(definition.into()).ok_or(FactQueryError::InfrastructureFailure)?;

    let substitution = GenericSubstitutionData::try_new(owner, parameters, arguments)
        .map_err(|_| FactQueryError::InfrastructureFailure)?;

    let substitution = values
        .intern_generic_substitution(substitution)
        .map_err(|_| FactQueryError::InfrastructureFailure)?;

    let application = values
        .intern_trait_application(TraitApplicationData::new(definition, substitution))
        .map_err(|_| FactQueryError::InfrastructureFailure)?;

    Ok(bray_symbols::ImplementationRequirementKey::new(
        subject,
        application,
    ))
}

pub(in crate::compilation) fn selected_type_valued_member(
    facts: &CompilationBinderFacts<'_>,
    substitution: bray_symbols::GenericSubstitutionId,
    fulfillments: &[TraitTypeFulfillmentSymbolId],
    member: TraitTypeMemberSymbolId,
    diagnostics: &mut DiagnosticBag,
) -> Result<TypeValuedMemberResolution, FactQueryError> {
    let expected_name =
        fulfillment_name(facts, member.into()).ok_or(FactQueryError::InfrastructureFailure)?;

    let mut matching = fulfillments.iter().copied().filter(|fulfillment| {
        fulfillment_name(facts, (*fulfillment).into()).is_some_and(|name| name == expected_name)
    });

    let Some(fulfillment) = matching.next() else {
        return Ok(TypeValuedMemberResolution::Invalid);
    };

    if matching.next().is_some() {
        return Ok(TypeValuedMemberResolution::Invalid);
    }

    let result = facts
        .symbol_fact(SymbolFactRequest::<TraitTypeFulfillmentValueFact>::new(
            fulfillment,
        ))
        .map_err(binder_fact_error)?;

    *diagnostics = diagnostics.merged(result.diagnostics());

    if result.diagnostics().has_errors() {
        return Ok(TypeValuedMemberResolution::Invalid);
    }

    let checked = facts.compilation().checked_constant_terms(result.value())?;

    *diagnostics = diagnostics.merged(checked.diagnostics());

    let Some(ty) = bray_checker::resolve_type_expression_template(
        facts.semantic_values(),
        result.value(),
        checked.value(),
    )
    .map_err(FactQueryError::CheckerInfrastructure)?
    else {
        return Ok(TypeValuedMemberResolution::Deferred);
    };

    facts
        .semantic_values()
        .substitute_type(ty, substitution)
        .map(TypeValuedMemberResolution::Resolved)
        .map_err(|_| FactQueryError::InfrastructureFailure)
}

pub(in crate::compilation) fn implementation_fulfillments<'facts>(
    facts: &'facts CompilationBinderFacts<'_>,
    implementation: ImplementationSymbolId,
) -> Result<ImplementationFulfillments<'facts>, FactQueryError> {
    let source =
        match implementation {
            ImplementationSymbolId::Inherent(id) => facts
                .symbols()
                .inherent_implementation(id)
                .map(|symbol| ImplementationFulfillments {
                    callables: symbol.callable_fulfillments(),
                    types: symbol.type_fulfillments(),
                }),
            ImplementationSymbolId::UnnamedTrait(id) => facts
                .symbols()
                .unnamed_trait_implementation(id)
                .map(|symbol| ImplementationFulfillments {
                    callables: symbol.callable_fulfillments(),
                    types: symbol.type_fulfillments(),
                }),
            ImplementationSymbolId::NamedTrait(id) => facts
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

    let imported = facts.imported_symbols().map_err(binder_fact_error)?;

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

    imported.ok_or(FactQueryError::InfrastructureFailure)
}

pub(in crate::compilation) fn selected_callable(
    facts: &CompilationBinderFacts<'_>,
    fulfillments: &[TraitCallableFulfillmentSymbolId],
    member: TraitCallableMemberSymbolId,
) -> Option<TraitCallableFulfillmentSymbolId> {
    let expected_name = fulfillment_name(facts, member.into())?;

    let mut matching = fulfillments.iter().copied().filter(|fulfillment| {
        fulfillment_name(facts, (*fulfillment).into()).is_some_and(|name| name == expected_name)
    });

    let fulfillment = matching.next()?;

    if matching.next().is_some() {
        return None;
    }

    Some(fulfillment)
}

pub(in crate::compilation) fn callable_instance(
    values: &bray_symbols::SemanticValueStore,
    callable: AnySymbolId,
    substitutions: impl IntoIterator<Item = bray_symbols::GenericSubstitutionId>,
) -> Result<CallableInstanceData, FactQueryError> {
    let definition =
        CallableDefinitionId::try_new(callable).ok_or(FactQueryError::InfrastructureFailure)?;

    let substitution =
        super::super::substitution::substitution_for_owner(values, callable, substitutions)?;

    Ok(CallableInstanceData::new(definition, substitution))
}

fn fulfillment_name<'facts>(
    facts: &'facts CompilationBinderFacts<'_>,
    fulfillment: AnySymbolId,
) -> Option<&'facts str> {
    if let Some(name) = facts.symbols().member_name(fulfillment) {
        return Some(name.as_str());
    }

    let key = facts.symbol_key(fulfillment).ok()??;

    let SymbolKeyData::External(key) = key.data() else {
        return None;
    };

    let ExternalSymbolKeyData::Declaration {
        identity: ExternalDeclarationIdentity::Name(name),
        ..
    } = key.data()
    else {
        return None;
    };

    Some(name.as_str())
}
