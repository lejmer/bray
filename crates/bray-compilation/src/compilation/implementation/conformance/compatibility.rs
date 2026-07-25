use std::collections::BTreeMap;

use bray_binder::SymbolFactProvider;
use bray_symbols::{
    CallableContractTemplate, CallableContractTemplateFact, CallableSignatureFact,
    CallableSymbolId, GenericConstParameterDeclaredTypeFact, GenericDeclarationTemplateFact,
    GenericOwnerId, GenericParameterSymbolId, PredicateDefinitionSymbolId,
    PredicateSignatureTemplateFact, RuntimeDefaultPresence, SymbolFactRequest, TraitApplicationId,
    TraitMemberFulfillmentId, TraitMemberRequirementId, TraitTypeFulfillmentValueFact,
    TypeExpressionTemplate,
};

use crate::compilation::binder::{CompilationBinderFacts, binder_fact_error};
use crate::fact::FactQueryError;

pub(super) fn fulfillment_is_compatible(
    symbols: &bray_symbols::SymbolGraph,
    values: &bray_symbols::SemanticValueStore,
    facts: &CompilationBinderFacts<'_>,
    trait_application: TraitApplicationId,
    requirement: TraitMemberRequirementId,
    fulfillment: TraitMemberFulfillmentId,
    type_bindings: &BTreeMap<bray_symbols::TraitTypeMemberSymbolId, TypeExpressionTemplate>,
) -> Result<bool, FactQueryError> {
    match (requirement, fulfillment) {
        (
            TraitMemberRequirementId::Callable(requirement),
            TraitMemberFulfillmentId::Callable(fulfillment),
        ) => callable_is_compatible(
            symbols,
            values,
            facts,
            trait_application,
            requirement.into(),
            fulfillment.into(),
            type_bindings,
        ),
        (
            TraitMemberRequirementId::Constant(requirement),
            TraitMemberFulfillmentId::Constant(fulfillment),
        ) => {
            let requirement = facts
                .symbol_fact(SymbolFactRequest::<
                    bray_symbols::TraitConstantMemberDeclaredTypeFact,
                >::new(requirement))
                .map_err(binder_fact_error)?;

            let fulfillment = facts
                .symbol_fact(SymbolFactRequest::<
                    bray_symbols::TraitConstantFulfillmentDeclaredTypeFact,
                >::new(fulfillment))
                .map_err(binder_fact_error)?;

            type_templates_are_compatible(
                values,
                trait_application,
                requirement.value(),
                fulfillment.value(),
                type_bindings,
            )
        }
        (TraitMemberRequirementId::Type(_), TraitMemberFulfillmentId::Type(fulfillment)) => {
            let value = facts
                .symbol_fact(SymbolFactRequest::<TraitTypeFulfillmentValueFact>::new(
                    fulfillment,
                ))
                .map_err(binder_fact_error)?;

            Ok(value.value().resolved_type().is_some() || !value.diagnostics().has_errors())
        }
        (
            TraitMemberRequirementId::Predicate(requirement),
            TraitMemberFulfillmentId::Predicate(fulfillment),
        ) => predicate_is_compatible(
            symbols,
            values,
            facts,
            trait_application,
            requirement.into(),
            fulfillment.into(),
            type_bindings,
        ),
        (
            TraitMemberRequirementId::ScopeEnter(requirement),
            TraitMemberFulfillmentId::ScopeEnter(fulfillment),
        ) => callable_is_compatible(
            symbols,
            values,
            facts,
            trait_application,
            requirement.into(),
            fulfillment.into(),
            type_bindings,
        ),
        (
            TraitMemberRequirementId::ScopeExit(requirement),
            TraitMemberFulfillmentId::ScopeExit(fulfillment),
        ) => callable_is_compatible(
            symbols,
            values,
            facts,
            trait_application,
            requirement.into(),
            fulfillment.into(),
            type_bindings,
        ),
        _ => Ok(false),
    }
}

fn callable_is_compatible(
    symbols: &bray_symbols::SymbolGraph,
    values: &bray_symbols::SemanticValueStore,
    facts: &CompilationBinderFacts<'_>,
    trait_application: TraitApplicationId,
    requirement: CallableSymbolId,
    fulfillment: CallableSymbolId,
    type_bindings: &BTreeMap<bray_symbols::TraitTypeMemberSymbolId, TypeExpressionTemplate>,
) -> Result<bool, FactQueryError> {
    let requirement_signature = facts
        .symbol_fact(SymbolFactRequest::<CallableSignatureFact>::new(requirement))
        .map_err(binder_fact_error)?;

    let fulfillment_signature = facts
        .symbol_fact(SymbolFactRequest::<CallableSignatureFact>::new(fulfillment))
        .map_err(binder_fact_error)?;

    let requirement_type = requirement_signature.value().callable_type();
    let fulfillment_type = fulfillment_signature.value().callable_type();

    if requirement_signature
        .value()
        .receiver()
        .map(|receiver| receiver.mode())
        != fulfillment_signature
            .value()
            .receiver()
            .map(|receiver| receiver.mode())
    {
        return Ok(false);
    }

    match (requirement_type, fulfillment_type) {
        (
            TypeExpressionTemplate::Callable(requirement),
            TypeExpressionTemplate::Callable(fulfillment),
        ) => {
            if requirement.constness() != fulfillment.constness()
                || requirement.execution() != fulfillment.execution()
                || requirement.trust() != fulfillment.trust()
                || requirement.abi() != fulfillment.abi()
                || requirement.parameters().len() != fulfillment.parameters().len()
            {
                return Ok(false);
            }

            for (requirement, fulfillment) in requirement
                .parameters()
                .iter()
                .zip(fulfillment.parameters())
            {
                if requirement.name() != fulfillment.name()
                    || requirement.position() != fulfillment.position()
                    || requirement.mode() != fulfillment.mode()
                    || !type_templates_are_compatible(
                        values,
                        trait_application,
                        requirement.ty(),
                        fulfillment.ty(),
                        type_bindings,
                    )?
                {
                    return Ok(false);
                }
            }

            if !type_templates_are_compatible(
                values,
                trait_application,
                requirement.result(),
                fulfillment.result(),
                type_bindings,
            )? {
                return Ok(false);
            }
        }
        _ if !type_templates_are_compatible(
            values,
            trait_application,
            requirement_type,
            fulfillment_type,
            type_bindings,
        )? =>
        {
            return Ok(false);
        }
        _ => {}
    }

    if !generic_surfaces_are_compatible(
        values,
        facts,
        trait_application,
        requirement.into_any(),
        fulfillment.into_any(),
        type_bindings,
    )? {
        return Ok(false);
    }

    if !parameter_defaults_are_compatible(
        symbols,
        requirement_signature.value().parameters(),
        fulfillment_signature.value().parameters(),
    )? {
        return Ok(false);
    }

    callable_contracts_are_compatible(facts, requirement, fulfillment)
}

fn generic_surfaces_are_compatible(
    values: &bray_symbols::SemanticValueStore,
    facts: &CompilationBinderFacts<'_>,
    trait_application: TraitApplicationId,
    requirement: bray_symbols::AnySymbolId,
    fulfillment: bray_symbols::AnySymbolId,
    type_bindings: &BTreeMap<bray_symbols::TraitTypeMemberSymbolId, TypeExpressionTemplate>,
) -> Result<bool, FactQueryError> {
    let Some(requirement) = GenericOwnerId::try_new(requirement) else {
        return Ok(true);
    };

    let Some(fulfillment) = GenericOwnerId::try_new(fulfillment) else {
        return Ok(false);
    };

    let requirement = facts
        .symbol_fact(SymbolFactRequest::<GenericDeclarationTemplateFact>::new(
            requirement,
        ))
        .map_err(binder_fact_error)?;

    let fulfillment = facts
        .symbol_fact(SymbolFactRequest::<GenericDeclarationTemplateFact>::new(
            fulfillment,
        ))
        .map_err(binder_fact_error)?;

    if requirement.value().parameters().len() != fulfillment.value().parameters().len()
        || requirement.value().constraints().len() != fulfillment.value().constraints().len()
    {
        return Ok(false);
    }

    for (requirement, fulfillment) in requirement
        .value()
        .parameters()
        .iter()
        .zip(fulfillment.value().parameters())
    {
        match (requirement, fulfillment) {
            (GenericParameterSymbolId::Type(_), GenericParameterSymbolId::Type(_)) => {}
            (
                GenericParameterSymbolId::Const(requirement),
                GenericParameterSymbolId::Const(fulfillment),
            ) => {
                let requirement = facts
                    .symbol_fact(
                        SymbolFactRequest::<GenericConstParameterDeclaredTypeFact>::new(
                            *requirement,
                        ),
                    )
                    .map_err(binder_fact_error)?;

                let fulfillment = facts
                    .symbol_fact(
                        SymbolFactRequest::<GenericConstParameterDeclaredTypeFact>::new(
                            *fulfillment,
                        ),
                    )
                    .map_err(binder_fact_error)?;

                if !type_templates_are_compatible(
                    values,
                    trait_application,
                    requirement.value(),
                    fulfillment.value(),
                    type_bindings,
                )? {
                    return Ok(false);
                }
            }
            _ => return Ok(false),
        }
    }

    Ok(true)
}

fn parameter_defaults_are_compatible(
    symbols: &bray_symbols::SymbolGraph,
    requirement: &[bray_symbols::CallableParameterSymbolId],
    fulfillment: &[bray_symbols::CallableParameterSymbolId],
) -> Result<bool, FactQueryError> {
    if requirement.len() != fulfillment.len() {
        return Ok(false);
    }

    for (requirement, fulfillment) in requirement.iter().zip(fulfillment) {
        let requirement = symbols
            .callable_parameter(*requirement)
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let fulfillment = symbols
            .callable_parameter(*fulfillment)
            .ok_or(FactQueryError::InfrastructureFailure)?;

        if matches!(
            requirement.default_presence(),
            RuntimeDefaultPresence::Present
        ) != matches!(
            fulfillment.default_presence(),
            RuntimeDefaultPresence::Present
        ) {
            return Ok(false);
        }
    }

    Ok(true)
}

fn callable_contracts_are_compatible(
    facts: &CompilationBinderFacts<'_>,
    requirement: CallableSymbolId,
    fulfillment: CallableSymbolId,
) -> Result<bool, FactQueryError> {
    let requirement = facts
        .symbol_fact(SymbolFactRequest::<CallableContractTemplateFact>::new(
            requirement,
        ))
        .map_err(binder_fact_error)?;

    let fulfillment = facts
        .symbol_fact(SymbolFactRequest::<CallableContractTemplateFact>::new(
            fulfillment,
        ))
        .map_err(binder_fact_error)?;

    Ok(match (requirement.value(), fulfillment.value()) {
        (
            CallableContractTemplate::Source(requirement),
            CallableContractTemplate::Source(fulfillment),
        ) => {
            requirement
                .expressions()
                .iter()
                .map(|clause| clause.kind())
                .eq(fulfillment.expressions().iter().map(|clause| clause.kind()))
                && requirement.capabilities().len() == fulfillment.capabilities().len()
        }
        (
            CallableContractTemplate::Resolved(requirement),
            CallableContractTemplate::Resolved(fulfillment),
        ) => requirement == fulfillment,
        _ => true,
    })
}

fn predicate_is_compatible(
    symbols: &bray_symbols::SymbolGraph,
    values: &bray_symbols::SemanticValueStore,
    facts: &CompilationBinderFacts<'_>,
    trait_application: TraitApplicationId,
    requirement: PredicateDefinitionSymbolId,
    fulfillment: PredicateDefinitionSymbolId,
    type_bindings: &BTreeMap<bray_symbols::TraitTypeMemberSymbolId, TypeExpressionTemplate>,
) -> Result<bool, FactQueryError> {
    let requirement = facts
        .symbol_fact(SymbolFactRequest::<PredicateSignatureTemplateFact>::new(
            requirement,
        ))
        .map_err(binder_fact_error)?;

    let fulfillment = facts
        .symbol_fact(SymbolFactRequest::<PredicateSignatureTemplateFact>::new(
            fulfillment,
        ))
        .map_err(binder_fact_error)?;

    if requirement.value().is_trusted() != fulfillment.value().is_trusted()
        || requirement.value().parameters().len() != fulfillment.value().parameters().len()
    {
        return Ok(false);
    }

    for (requirement, fulfillment) in requirement
        .value()
        .parameters()
        .iter()
        .zip(fulfillment.value().parameters())
    {
        let requirement_name = symbols
            .member_name(requirement.parameter().into())
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let fulfillment_name = symbols
            .member_name(fulfillment.parameter().into())
            .ok_or(FactQueryError::InfrastructureFailure)?;

        if requirement_name != fulfillment_name
            || !type_templates_are_compatible(
                values,
                trait_application,
                requirement.ty(),
                fulfillment.ty(),
                type_bindings,
            )?
        {
            return Ok(false);
        }
    }

    Ok(true)
}

fn type_templates_are_compatible(
    values: &bray_symbols::SemanticValueStore,
    trait_application: TraitApplicationId,
    requirement: &TypeExpressionTemplate,
    fulfillment: &TypeExpressionTemplate,
    type_bindings: &BTreeMap<bray_symbols::TraitTypeMemberSymbolId, TypeExpressionTemplate>,
) -> Result<bool, FactQueryError> {
    let substitution = values
        .trait_application_data(trait_application)
        .map_err(|_| FactQueryError::InfrastructureFailure)?
        .substitution();

    match requirement {
        TypeExpressionTemplate::Resolved(requirement) => {
            let requirement = values
                .substitute_type(*requirement, substitution)
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

            Ok(fulfillment.resolved_type() == Some(requirement))
        }
        TypeExpressionTemplate::TypeValuedMemberProjection { member, .. } => {
            let Some(requirement) = type_bindings.get(member) else {
                return Ok(false);
            };

            type_templates_are_compatible(
                values,
                trait_application,
                requirement,
                fulfillment,
                type_bindings,
            )
        }
        _ => Ok(requirement == fulfillment),
    }
}
