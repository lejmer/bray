use std::collections::BTreeMap;

use bray_symbols::{
    DependencyContractTemplateId, GenericSubstitutionId, SemanticValueStore, TraitApplicationId,
    TraitTypeMemberSymbolId, TypeData, TypeExpressionTemplate, TypeId,
};

use crate::fact::FactQueryError;

pub(super) fn type_templates_are_compatible(
    values: &SemanticValueStore,
    trait_application: TraitApplicationId,
    generic_substitution: Option<GenericSubstitutionId>,
    requirement: &TypeExpressionTemplate,
    fulfillment: &TypeExpressionTemplate,
    type_bindings: &BTreeMap<TraitTypeMemberSymbolId, TypeExpressionTemplate>,
) -> Result<bool, FactQueryError> {
    match requirement {
        TypeExpressionTemplate::Resolved(requirement) => {
            let requirement = substitute_requirement_type(
                values,
                trait_application,
                generic_substitution,
                *requirement,
            )?;

            let requirement_data = values
                .type_data(requirement)
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

            if let TypeData::Nullable(requirement) = requirement_data.as_ref() {
                let Some(fulfillment) = fulfillment.resolved_type() else {
                    return Ok(false);
                };

                let fulfillment_data = values
                    .type_data(fulfillment)
                    .map_err(|_| FactQueryError::InfrastructureFailure)?;

                let TypeData::Nullable(fulfillment) = fulfillment_data.as_ref() else {
                    return Ok(false);
                };

                return type_templates_are_compatible(
                    values,
                    trait_application,
                    generic_substitution,
                    &TypeExpressionTemplate::Resolved(*requirement),
                    &TypeExpressionTemplate::Resolved(*fulfillment),
                    type_bindings,
                );
            }

            if let TypeData::TypeValuedMemberProjection { member, .. } = requirement_data.as_ref() {
                let Some(requirement) = type_bindings.get(member) else {
                    return Ok(false);
                };

                return type_templates_are_compatible(
                    values,
                    trait_application,
                    generic_substitution,
                    requirement,
                    fulfillment,
                    type_bindings,
                );
            }

            Ok(fulfillment.resolved_type() == Some(requirement))
        }
        TypeExpressionTemplate::TypeValuedMemberProjection { member, .. } => {
            let Some(requirement) = type_bindings.get(member) else {
                return Ok(false);
            };

            type_templates_are_compatible(
                values,
                trait_application,
                generic_substitution,
                requirement,
                fulfillment,
                type_bindings,
            )
        }
        TypeExpressionTemplate::Nullable(requirement) => nullable_templates_are_compatible(
            values,
            trait_application,
            generic_substitution,
            requirement,
            fulfillment,
            type_bindings,
        ),
        _ => Ok(requirement == fulfillment),
    }
}

fn nullable_templates_are_compatible(
    values: &SemanticValueStore,
    trait_application: TraitApplicationId,
    generic_substitution: Option<GenericSubstitutionId>,
    requirement: &TypeExpressionTemplate,
    fulfillment: &TypeExpressionTemplate,
    type_bindings: &BTreeMap<TraitTypeMemberSymbolId, TypeExpressionTemplate>,
) -> Result<bool, FactQueryError> {
    match fulfillment {
        TypeExpressionTemplate::Nullable(fulfillment) => type_templates_are_compatible(
            values,
            trait_application,
            generic_substitution,
            requirement,
            fulfillment,
            type_bindings,
        ),
        TypeExpressionTemplate::Resolved(fulfillment) => {
            let data = values
                .type_data(*fulfillment)
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

            let TypeData::Nullable(fulfillment) = data.as_ref() else {
                return Ok(false);
            };

            type_templates_are_compatible(
                values,
                trait_application,
                generic_substitution,
                requirement,
                &TypeExpressionTemplate::Resolved(*fulfillment),
                type_bindings,
            )
        }
        _ => Ok(false),
    }
}

pub(super) fn substitute_requirement_type(
    values: &SemanticValueStore,
    trait_application: TraitApplicationId,
    generic_substitution: Option<GenericSubstitutionId>,
    requirement: TypeId,
) -> Result<TypeId, FactQueryError> {
    let trait_substitution = values
        .trait_application_data(trait_application)
        .map_err(|_| FactQueryError::InfrastructureFailure)?
        .substitution();

    let requirement = values
        .substitute_type(requirement, trait_substitution)
        .map_err(|_| FactQueryError::InfrastructureFailure)?;

    match generic_substitution {
        Some(substitution) => values
            .substitute_type(requirement, substitution)
            .map_err(|_| FactQueryError::InfrastructureFailure),
        None => Ok(requirement),
    }
}

pub(super) fn dependency_contracts_are_compatible(
    values: &SemanticValueStore,
    trait_application: TraitApplicationId,
    generic_substitution: Option<GenericSubstitutionId>,
    requirement: DependencyContractTemplateId,
    fulfillment: DependencyContractTemplateId,
) -> Result<bool, FactQueryError> {
    let trait_substitution = values
        .trait_application_data(trait_application)
        .map_err(|_| FactQueryError::InfrastructureFailure)?
        .substitution();

    let requirement = values
        .substitute_dependency_contract(requirement, trait_substitution)
        .map_err(|_| FactQueryError::InfrastructureFailure)?;

    let requirement = match generic_substitution {
        Some(substitution) => values
            .substitute_dependency_contract(requirement, substitution)
            .map_err(|_| FactQueryError::InfrastructureFailure)?,
        None => requirement,
    };

    Ok(requirement == fulfillment)
}
