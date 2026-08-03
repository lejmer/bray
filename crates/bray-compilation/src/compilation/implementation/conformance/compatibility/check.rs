use std::collections::BTreeMap;

use bray_binder::SymbolFactProvider;
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{
    CallableContractClause, CallableContractSet, CallableContractsFact,
    CallableParameterTypeTemplate, CallablePhaseBehavior, CallableSignatureFact,
    CallableSignatureTemplate, CallableSymbolId, CallableTypeTemplate, ConstantTermData,
    GenericArgument, GenericConstParameterDeclaredTypeFact, GenericConstraintsFact,
    GenericDeclarationTemplateFact, GenericOwnerId, GenericParameterSymbolId,
    GenericSubstitutionData, GenericSubstitutionId, PredicateDefinitionSymbolId,
    PredicateSignatureTemplateFact, RuntimeDefaultPresence, SymbolFactRequest, TraitApplicationId,
    TraitMemberFulfillmentId, TraitMemberRequirementId, TraitTypeFulfillmentValueFact, TypeData,
    TypeExpressionTemplate,
};

use crate::compilation::binder::{CompilationBinderFacts, binder_fact_error};
use crate::fact::FactQueryError;

use super::types::{
    dependency_contracts_are_compatible, substitute_requirement_type, type_templates_are_compatible,
};

macro_rules! demand_fact {
    ($facts:expr, $diagnostics:expr, $contract:ty, $owner:expr) => {{
        let result = $facts
            .symbol_fact(SymbolFactRequest::<$contract>::new($owner))
            .map_err(binder_fact_error)?;

        *$diagnostics = $diagnostics.merged(result.diagnostics());

        result
    }};
}
pub(in crate::compilation::implementation::conformance) struct CompatibilityContext<
    'facts,
    'compilation,
> {
    symbols: &'facts bray_symbols::SymbolGraph,
    imported: Option<&'facts bray_symbols::ImportedSymbolSkeleton>,
    values: &'facts bray_symbols::SemanticValueStore,
    facts: &'facts CompilationBinderFacts<'compilation>,
    trait_application: TraitApplicationId,
    type_bindings: &'facts BTreeMap<bray_symbols::TraitTypeMemberSymbolId, TypeExpressionTemplate>,
}

impl<'facts, 'compilation> CompatibilityContext<'facts, 'compilation> {
    pub(in crate::compilation::implementation::conformance) fn new(
        symbols: &'facts bray_symbols::SymbolGraph,
        imported: Option<&'facts bray_symbols::ImportedSymbolSkeleton>,
        values: &'facts bray_symbols::SemanticValueStore,
        facts: &'facts CompilationBinderFacts<'compilation>,
        trait_application: TraitApplicationId,
        type_bindings: &'facts BTreeMap<
            bray_symbols::TraitTypeMemberSymbolId,
            TypeExpressionTemplate,
        >,
    ) -> Self {
        Self {
            symbols,
            imported,
            values,
            facts,
            trait_application,
            type_bindings,
        }
    }
}

pub(in crate::compilation::implementation::conformance) fn fulfillment_is_compatible(
    context: &CompatibilityContext<'_, '_>,
    requirement: TraitMemberRequirementId,
    fulfillment: TraitMemberFulfillmentId,
    diagnostics: &mut DiagnosticBag,
) -> Result<bool, FactQueryError> {
    let values = context.values;
    let facts = context.facts;
    let trait_application = context.trait_application;
    let type_bindings = context.type_bindings;

    match (requirement, fulfillment) {
        (
            TraitMemberRequirementId::Callable(requirement),
            TraitMemberFulfillmentId::Callable(fulfillment),
        ) => callable_is_compatible(context, requirement.into(), fulfillment.into(), diagnostics),
        (
            TraitMemberRequirementId::Constant(requirement),
            TraitMemberFulfillmentId::Constant(fulfillment),
        ) => {
            let requirement = demand_fact!(
                facts,
                diagnostics,
                bray_symbols::TraitConstantMemberDeclaredTypeFact,
                requirement
            );

            let fulfillment = demand_fact!(
                facts,
                diagnostics,
                bray_symbols::TraitConstantFulfillmentDeclaredTypeFact,
                fulfillment
            );

            type_templates_are_compatible(
                values,
                trait_application,
                None,
                requirement.value(),
                fulfillment.value(),
                type_bindings,
            )
        }
        (TraitMemberRequirementId::Type(_), TraitMemberFulfillmentId::Type(fulfillment)) => {
            let value = demand_fact!(
                facts,
                diagnostics,
                TraitTypeFulfillmentValueFact,
                fulfillment
            );

            Ok(value.value().resolved_type().is_some() || !value.diagnostics().has_errors())
        }
        (
            TraitMemberRequirementId::Predicate(requirement),
            TraitMemberFulfillmentId::Predicate(fulfillment),
        ) => predicate_is_compatible(context, requirement.into(), fulfillment.into(), diagnostics),
        (
            TraitMemberRequirementId::Finalizer(requirement),
            TraitMemberFulfillmentId::Finalizer(fulfillment),
        ) => callable_is_compatible(context, requirement.into(), fulfillment.into(), diagnostics),
        (
            TraitMemberRequirementId::Destructor(requirement),
            TraitMemberFulfillmentId::Destructor(fulfillment),
        ) => callable_is_compatible(context, requirement.into(), fulfillment.into(), diagnostics),
        (
            TraitMemberRequirementId::ScopeEnter(requirement),
            TraitMemberFulfillmentId::ScopeEnter(fulfillment),
        ) => callable_is_compatible(context, requirement.into(), fulfillment.into(), diagnostics),
        (
            TraitMemberRequirementId::ScopeExit(requirement),
            TraitMemberFulfillmentId::ScopeExit(fulfillment),
        ) => callable_is_compatible(context, requirement.into(), fulfillment.into(), diagnostics),
        _ => Ok(false),
    }
}

pub(in crate::compilation::implementation::conformance) fn subject_lifecycle_is_compatible(
    context: &CompatibilityContext<'_, '_>,
    requirement: TraitMemberRequirementId,
    fulfillment: CallableSymbolId,
    diagnostics: &mut DiagnosticBag,
) -> Result<bool, FactQueryError> {
    let requirement = match requirement {
        TraitMemberRequirementId::Finalizer(requirement) => requirement.into(),
        TraitMemberRequirementId::Destructor(requirement) => requirement.into(),
        _ => return Ok(false),
    };

    callable_is_compatible(context, requirement, fulfillment, diagnostics)
}

fn callable_is_compatible(
    context: &CompatibilityContext<'_, '_>,
    requirement: CallableSymbolId,
    fulfillment: CallableSymbolId,
    diagnostics: &mut DiagnosticBag,
) -> Result<bool, FactQueryError> {
    let symbols = context.symbols;
    let imported = context.imported;
    let values = context.values;
    let facts = context.facts;
    let trait_application = context.trait_application;
    let type_bindings = context.type_bindings;

    let requirement_signature =
        demand_fact!(facts, diagnostics, CallableSignatureFact, requirement);

    let fulfillment_signature =
        demand_fact!(facts, diagnostics, CallableSignatureFact, fulfillment);

    let (generics_are_compatible, generic_substitution) = generic_surfaces_are_compatible(
        values,
        facts,
        trait_application,
        requirement.into_any(),
        fulfillment.into_any(),
        type_bindings,
        diagnostics,
    )?;

    if !generics_are_compatible {
        return Ok(false);
    }

    let requirement_type = callable_type_template(values, requirement_signature.value())?;
    let fulfillment_type = callable_type_template(values, fulfillment_signature.value())?;

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

    if requirement_type.constness() != fulfillment_type.constness()
        || requirement_type.execution() != fulfillment_type.execution()
        || requirement_type.trust() != fulfillment_type.trust()
        || requirement_type.abi() != fulfillment_type.abi()
        || requirement_type.parameters().len() != fulfillment_type.parameters().len()
    {
        return Ok(false);
    }

    for (requirement, fulfillment) in requirement_type
        .parameters()
        .iter()
        .zip(fulfillment_type.parameters())
    {
        if requirement.name() != fulfillment.name()
            || requirement.position() != fulfillment.position()
            || requirement.mode() != fulfillment.mode()
            || !type_templates_are_compatible(
                values,
                trait_application,
                generic_substitution,
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
        generic_substitution,
        requirement_type.result(),
        fulfillment_type.result(),
        type_bindings,
    )? {
        return Ok(false);
    }

    if !parameter_defaults_are_compatible(
        symbols,
        imported,
        requirement_signature.value().parameters(),
        fulfillment_signature.value().parameters(),
    )? {
        return Ok(false);
    }

    let compatible = callable_contracts_are_compatible(
        values,
        facts,
        trait_application,
        generic_substitution,
        requirement,
        fulfillment,
        diagnostics,
    )?;

    Ok(compatible)
}

fn callable_type_template(
    values: &bray_symbols::SemanticValueStore,
    signature: &CallableSignatureTemplate,
) -> Result<CallableTypeTemplate, FactQueryError> {
    match signature.callable_type() {
        TypeExpressionTemplate::Callable(callable) => Ok(callable.clone()),
        TypeExpressionTemplate::Resolved(ty) => {
            let data = values
                .type_data(*ty)
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

            let TypeData::Callable(callable) = data.as_ref() else {
                return Err(FactQueryError::InfrastructureFailure);
            };

            let parameter_types = signature
                .parameter_type_templates(values)
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

            let parameters =
                callable
                    .parameters()
                    .iter()
                    .zip(parameter_types)
                    .map(|(parameter, ty)| {
                        CallableParameterTypeTemplate::new(
                            parameter.name().clone(),
                            parameter.position(),
                            parameter.mode(),
                            ty,
                        )
                    });

            Ok(CallableTypeTemplate::new(
                parameters,
                signature.result().clone(),
                callable.constness(),
                callable.trust(),
                callable.abi(),
                callable.dependency_contracts(),
            )
            .with_phase_behaviors(callable.phase_behaviors().clone()))
        }
        _ => Err(FactQueryError::InfrastructureFailure),
    }
}

fn generic_surfaces_are_compatible(
    values: &bray_symbols::SemanticValueStore,
    facts: &CompilationBinderFacts<'_>,
    trait_application: TraitApplicationId,
    requirement: bray_symbols::AnySymbolId,
    fulfillment: bray_symbols::AnySymbolId,
    type_bindings: &BTreeMap<bray_symbols::TraitTypeMemberSymbolId, TypeExpressionTemplate>,
    diagnostics: &mut DiagnosticBag,
) -> Result<(bool, Option<GenericSubstitutionId>), FactQueryError> {
    let Some(requirement_owner) = GenericOwnerId::try_new(requirement) else {
        return Ok((true, None));
    };

    let Some(fulfillment_owner) = GenericOwnerId::try_new(fulfillment) else {
        return Ok((false, None));
    };

    let requirement = demand_fact!(
        facts,
        diagnostics,
        GenericDeclarationTemplateFact,
        requirement_owner
    );

    let fulfillment = demand_fact!(
        facts,
        diagnostics,
        GenericDeclarationTemplateFact,
        fulfillment_owner
    );

    if requirement.value().parameters().len() != fulfillment.value().parameters().len() {
        return Ok((false, None));
    }

    let mut arguments = Vec::with_capacity(fulfillment.value().parameters().len());

    for (requirement_parameter, fulfillment_parameter) in requirement
        .value()
        .parameters()
        .iter()
        .zip(fulfillment.value().parameters())
    {
        match (requirement_parameter, fulfillment_parameter) {
            (GenericParameterSymbolId::Type(_), GenericParameterSymbolId::Type(fulfillment)) => {
                let ty = values
                    .intern_type(TypeData::TypeParameter(*fulfillment))
                    .map_err(|_| FactQueryError::InfrastructureFailure)?;

                arguments.push(GenericArgument::Type(ty));
            }
            (GenericParameterSymbolId::Const(_), GenericParameterSymbolId::Const(fulfillment)) => {
                let term = values
                    .intern_constant_term(ConstantTermData::Parameter(*fulfillment))
                    .map_err(|_| FactQueryError::InfrastructureFailure)?;

                arguments.push(GenericArgument::Constant(term));
            }
            _ => return Ok((false, None)),
        }
    }

    let substitution = GenericSubstitutionData::try_new(
        requirement_owner,
        requirement.value().parameters().iter().copied(),
        arguments,
    )
    .map_err(|_| FactQueryError::InfrastructureFailure)?;

    let substitution = values
        .intern_generic_substitution(substitution)
        .map_err(|_| FactQueryError::InfrastructureFailure)?;

    for (requirement, fulfillment) in requirement
        .value()
        .parameters()
        .iter()
        .zip(fulfillment.value().parameters())
    {
        let (
            GenericParameterSymbolId::Const(requirement),
            GenericParameterSymbolId::Const(fulfillment),
        ) = (requirement, fulfillment)
        else {
            continue;
        };

        let requirement_type = demand_fact!(
            facts,
            diagnostics,
            GenericConstParameterDeclaredTypeFact,
            *requirement
        );

        let fulfillment_type = demand_fact!(
            facts,
            diagnostics,
            GenericConstParameterDeclaredTypeFact,
            *fulfillment
        );

        if !type_templates_are_compatible(
            values,
            trait_application,
            Some(substitution),
            requirement_type.value(),
            fulfillment_type.value(),
            type_bindings,
        )? {
            return Ok((false, None));
        }
    }

    let requirement_constraints = demand_fact!(
        facts,
        diagnostics,
        GenericConstraintsFact,
        requirement_owner
    );

    let fulfillment_constraints = demand_fact!(
        facts,
        diagnostics,
        GenericConstraintsFact,
        fulfillment_owner
    );

    if !constraints_are_compatible(
        values,
        trait_application,
        substitution,
        requirement_constraints.value(),
        fulfillment_constraints.value(),
    )? {
        return Ok((false, None));
    }

    Ok((true, Some(substitution)))
}

fn constraints_are_compatible(
    values: &bray_symbols::SemanticValueStore,
    trait_application: TraitApplicationId,
    generic_substitution: GenericSubstitutionId,
    requirement: &bray_symbols::GenericConstraintSet,
    fulfillment: &bray_symbols::GenericConstraintSet,
) -> Result<bool, FactQueryError> {
    if requirement.constraints().len() != fulfillment.constraints().len() {
        return Ok(false);
    }

    for (requirement, fulfillment) in requirement
        .constraints()
        .iter()
        .zip(fulfillment.constraints())
    {
        if requirement.ordinal() != fulfillment.ordinal() {
            return Ok(false);
        }

        let compatible = match (requirement.kind(), fulfillment.kind()) {
            (
                bray_symbols::CheckedConstraintKind::Predicate(requirement),
                bray_symbols::CheckedConstraintKind::Predicate(fulfillment),
            ) => dependency_contracts_are_compatible(
                values,
                trait_application,
                Some(generic_substitution),
                requirement.dependency_contract(),
                fulfillment.dependency_contract(),
            )?,
            (
                bray_symbols::CheckedConstraintKind::TraitSatisfaction {
                    subject: requirement_subject,
                    application: requirement_application,
                },
                bray_symbols::CheckedConstraintKind::TraitSatisfaction {
                    subject: fulfillment_subject,
                    application: fulfillment_application,
                },
            ) => {
                substitute_requirement_type(
                    values,
                    trait_application,
                    Some(generic_substitution),
                    requirement_subject,
                )? == fulfillment_subject
                    && substitute_requirement_trait_application(
                        values,
                        trait_application,
                        Some(generic_substitution),
                        requirement_application,
                    )? == fulfillment_application
            }
            _ => false,
        };

        if !compatible {
            return Ok(false);
        }
    }

    Ok(true)
}

fn substitute_requirement_trait_application(
    values: &bray_symbols::SemanticValueStore,
    containing_trait: TraitApplicationId,
    generic_substitution: Option<GenericSubstitutionId>,
    requirement: TraitApplicationId,
) -> Result<TraitApplicationId, FactQueryError> {
    let trait_substitution = values
        .trait_application_data(containing_trait)
        .map_err(|_| FactQueryError::InfrastructureFailure)?
        .substitution();

    let requirement = values
        .substitute_trait_application(requirement, trait_substitution)
        .map_err(|_| FactQueryError::InfrastructureFailure)?;

    match generic_substitution {
        Some(substitution) => values
            .substitute_trait_application(requirement, substitution)
            .map_err(|_| FactQueryError::InfrastructureFailure),
        None => Ok(requirement),
    }
}

fn parameter_defaults_are_compatible(
    symbols: &bray_symbols::SymbolGraph,
    imported: Option<&bray_symbols::ImportedSymbolSkeleton>,
    requirement: &[bray_symbols::CallableParameterSymbolId],
    fulfillment: &[bray_symbols::CallableParameterSymbolId],
) -> Result<bool, FactQueryError> {
    if requirement.len() != fulfillment.len() {
        return Ok(false);
    }

    for (requirement, fulfillment) in requirement.iter().zip(fulfillment) {
        let requirement = symbols
            .callable_parameter(*requirement)
            .or_else(|| imported.and_then(|symbols| symbols.callable_parameter(*requirement)))
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let fulfillment = symbols
            .callable_parameter(*fulfillment)
            .or_else(|| imported.and_then(|symbols| symbols.callable_parameter(*fulfillment)))
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
    values: &bray_symbols::SemanticValueStore,
    facts: &CompilationBinderFacts<'_>,
    trait_application: TraitApplicationId,
    generic_substitution: Option<GenericSubstitutionId>,
    requirement: CallableSymbolId,
    fulfillment: CallableSymbolId,
    diagnostics: &mut DiagnosticBag,
) -> Result<bool, FactQueryError> {
    let requirement = demand_fact!(facts, diagnostics, CallableContractsFact, requirement);
    let fulfillment = demand_fact!(facts, diagnostics, CallableContractsFact, fulfillment);

    contract_sets_are_compatible(
        values,
        trait_application,
        generic_substitution,
        requirement.value(),
        fulfillment.value(),
    )
}

fn contract_sets_are_compatible(
    values: &bray_symbols::SemanticValueStore,
    trait_application: TraitApplicationId,
    generic_substitution: Option<GenericSubstitutionId>,
    requirement: &CallableContractSet,
    fulfillment: &CallableContractSet,
) -> Result<bool, FactQueryError> {
    if !contract_clauses_are_compatible(
        values,
        trait_application,
        generic_substitution,
        requirement.invocation_preconditions(),
        fulfillment.invocation_preconditions(),
    )? || !contract_clauses_are_compatible(
        values,
        trait_application,
        generic_substitution,
        requirement.static_constraints(),
        fulfillment.static_constraints(),
    )? || !contract_clauses_are_compatible(
        values,
        trait_application,
        generic_substitution,
        requirement.normal_completion_postconditions(),
        fulfillment.normal_completion_postconditions(),
    )? || !phase_behaviors_are_compatible(
        values,
        trait_application,
        generic_substitution,
        requirement.invocation_behavior(),
        fulfillment.invocation_behavior(),
    )? {
        return Ok(false);
    }

    match (
        requirement.deferred_execution_behavior(),
        fulfillment.deferred_execution_behavior(),
    ) {
        (Some(requirement), Some(fulfillment)) => phase_behaviors_are_compatible(
            values,
            trait_application,
            generic_substitution,
            requirement,
            fulfillment,
        ),
        (None, None) => Ok(true),
        _ => Ok(false),
    }
}

fn contract_clauses_are_compatible(
    values: &bray_symbols::SemanticValueStore,
    trait_application: TraitApplicationId,
    generic_substitution: Option<GenericSubstitutionId>,
    requirement: &[CallableContractClause],
    fulfillment: &[CallableContractClause],
) -> Result<bool, FactQueryError> {
    if requirement.len() != fulfillment.len() {
        return Ok(false);
    }

    for (requirement, fulfillment) in requirement.iter().zip(fulfillment) {
        if requirement.ordinal() != fulfillment.ordinal()
            || requirement.kind() != fulfillment.kind()
        {
            return Ok(false);
        }

        let compatible = match (requirement.value(), fulfillment.value()) {
            (
                bray_symbols::CallableContractClauseValue::Predicate(requirement),
                bray_symbols::CallableContractClauseValue::Predicate(fulfillment),
            ) => dependency_contracts_are_compatible(
                values,
                trait_application,
                generic_substitution,
                requirement.dependency_contract(),
                fulfillment.dependency_contract(),
            )?,
            (
                bray_symbols::CallableContractClauseValue::TraitSatisfaction {
                    subject: requirement_subject,
                    application: requirement_application,
                },
                bray_symbols::CallableContractClauseValue::TraitSatisfaction {
                    subject: fulfillment_subject,
                    application: fulfillment_application,
                },
            ) => {
                substitute_requirement_type(
                    values,
                    trait_application,
                    generic_substitution,
                    requirement_subject,
                )? == fulfillment_subject
                    && substitute_requirement_trait_application(
                        values,
                        trait_application,
                        generic_substitution,
                        requirement_application,
                    )? == fulfillment_application
            }
            _ => false,
        };

        if !compatible {
            return Ok(false);
        }
    }

    Ok(true)
}

fn phase_behaviors_are_compatible(
    values: &bray_symbols::SemanticValueStore,
    trait_application: TraitApplicationId,
    generic_substitution: Option<GenericSubstitutionId>,
    requirement: &CallablePhaseBehavior,
    fulfillment: &CallablePhaseBehavior,
) -> Result<bool, FactQueryError> {
    if requirement.effects() != fulfillment.effects()
        || requirement.capabilities() != fulfillment.capabilities()
        || requirement.trusted_capabilities() != fulfillment.trusted_capabilities()
        || requirement.execution_requirements() != fulfillment.execution_requirements()
        || requirement.lifecycle_obligations() != fulfillment.lifecycle_obligations()
    {
        return Ok(false);
    }

    dependency_contracts_are_compatible(
        values,
        trait_application,
        generic_substitution,
        requirement.dependency_contract(),
        fulfillment.dependency_contract(),
    )
}

fn predicate_is_compatible(
    context: &CompatibilityContext<'_, '_>,
    requirement: PredicateDefinitionSymbolId,
    fulfillment: PredicateDefinitionSymbolId,
    diagnostics: &mut DiagnosticBag,
) -> Result<bool, FactQueryError> {
    let symbols = context.symbols;
    let imported = context.imported;
    let values = context.values;
    let facts = context.facts;
    let trait_application = context.trait_application;
    let type_bindings = context.type_bindings;

    let requirement_signature = demand_fact!(
        facts,
        diagnostics,
        PredicateSignatureTemplateFact,
        requirement
    );

    let fulfillment_signature = demand_fact!(
        facts,
        diagnostics,
        PredicateSignatureTemplateFact,
        fulfillment
    );

    let (generics_are_compatible, generic_substitution) = generic_surfaces_are_compatible(
        values,
        facts,
        trait_application,
        requirement.into_any(),
        fulfillment.into_any(),
        type_bindings,
        diagnostics,
    )?;

    if !generics_are_compatible
        || requirement_signature.value().is_trusted() != fulfillment_signature.value().is_trusted()
        || requirement_signature.value().parameters().len()
            != fulfillment_signature.value().parameters().len()
    {
        return Ok(false);
    }

    for (requirement, fulfillment) in requirement_signature
        .value()
        .parameters()
        .iter()
        .zip(fulfillment_signature.value().parameters())
    {
        let requirement_name = symbols
            .member_name(requirement.parameter().into())
            .or_else(|| {
                imported.and_then(|symbols| symbols.member_name(requirement.parameter().into()))
            })
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let fulfillment_name = symbols
            .member_name(fulfillment.parameter().into())
            .or_else(|| {
                imported.and_then(|symbols| symbols.member_name(fulfillment.parameter().into()))
            })
            .ok_or(FactQueryError::InfrastructureFailure)?;

        if requirement_name != fulfillment_name
            || !type_templates_are_compatible(
                values,
                trait_application,
                generic_substitution,
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
