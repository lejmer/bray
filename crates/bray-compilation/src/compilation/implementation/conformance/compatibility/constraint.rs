use bray_symbols::{
    CheckedConstraintKind, GenericConstraintSet, GenericSubstitutionId, SemanticValueStore,
    TraitApplicationId, TypeId,
};

use crate::fact::FactQueryError;

use super::types::{dependency_contracts_are_compatible, substitute_requirement_type};

pub(super) fn constraints_are_compatible(
    values: &SemanticValueStore,
    subject: TypeId,
    trait_application: TraitApplicationId,
    generic_substitution: GenericSubstitutionId,
    requirement: &GenericConstraintSet,
    fulfillment: &GenericConstraintSet,
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
                CheckedConstraintKind::Predicate(requirement),
                CheckedConstraintKind::Predicate(fulfillment),
            ) => dependency_contracts_are_compatible(
                values,
                trait_application,
                Some(generic_substitution),
                requirement.dependency_contract(),
                fulfillment.dependency_contract(),
            )?,
            (
                CheckedConstraintKind::TraitSatisfaction {
                    subject: requirement_subject,
                    application: requirement_application,
                },
                CheckedConstraintKind::TraitSatisfaction {
                    subject: fulfillment_subject,
                    application: fulfillment_application,
                },
            ) => {
                substitute_requirement_type(
                    values,
                    subject,
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

pub(super) fn substitute_requirement_trait_application(
    values: &SemanticValueStore,
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
