use bray_symbols::{
    CheckedConstraintKind, GenericConstraintSet, GenericSubstitutionId, SemanticValueStore,
    TraitApplicationId, TypeId,
};

use crate::fact::FactQueryError;

use super::mismatch::{ConstraintCategory, GenericConstraintMismatch};
use super::types::{dependency_contracts_are_compatible, substitute_requirement_type};

pub(super) fn constraints_are_compatible(
    values: &SemanticValueStore,
    subject: TypeId,
    trait_application: TraitApplicationId,
    generic_substitution: GenericSubstitutionId,
    requirement: &GenericConstraintSet,
    fulfillment: &GenericConstraintSet,
) -> Result<Option<GenericConstraintMismatch>, FactQueryError> {
    if requirement.constraints().len() != fulfillment.constraints().len() {
        return Ok(Some(GenericConstraintMismatch::Count {
            required: requirement.constraints().len(),
            provided: fulfillment.constraints().len(),
        }));
    }

    for (index, (requirement, fulfillment)) in requirement
        .constraints()
        .iter()
        .zip(fulfillment.constraints())
        .enumerate()
    {
        if requirement.ordinal() != fulfillment.ordinal() {
            return Ok(Some(GenericConstraintMismatch::Ordinal { index }));
        }

        let mismatch = match (requirement.kind(), fulfillment.kind()) {
            (
                CheckedConstraintKind::Predicate(requirement),
                CheckedConstraintKind::Predicate(fulfillment),
            ) => (!dependency_contracts_are_compatible(
                values,
                trait_application,
                Some(generic_substitution),
                requirement.dependency_contract(),
                fulfillment.dependency_contract(),
            )?)
            .then_some(GenericConstraintMismatch::PredicateDependencies { index }),
            (
                CheckedConstraintKind::TraitSatisfaction {
                    subject: requirement_subject,
                    application: requirement_application,
                },
                CheckedConstraintKind::TraitSatisfaction {
                    subject: fulfillment_subject,
                    application: fulfillment_application,
                },
            ) => (!(substitute_requirement_type(
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
                )? == fulfillment_application))
                .then_some(GenericConstraintMismatch::TraitSatisfaction { index }),
            (
                CheckedConstraintKind::TypeEquality {
                    left: required_left,
                    right: required_right,
                },
                CheckedConstraintKind::TypeEquality {
                    left: provided_left,
                    right: provided_right,
                },
            ) => (!(substitute_requirement_type(
                values,
                subject,
                trait_application,
                Some(generic_substitution),
                required_left,
            )? == provided_left
                && substitute_requirement_type(
                    values,
                    subject,
                    trait_application,
                    Some(generic_substitution),
                    required_right,
                )? == provided_right))
                .then_some(GenericConstraintMismatch::TypeEquality { index }),
            (requirement, fulfillment) => Some(GenericConstraintMismatch::Category {
                index,
                required: constraint_category(&requirement),
                provided: constraint_category(&fulfillment),
            }),
        };

        if mismatch.is_some() {
            return Ok(mismatch);
        }
    }

    Ok(None)
}

const fn constraint_category(kind: &CheckedConstraintKind) -> ConstraintCategory {
    match kind {
        CheckedConstraintKind::Predicate(_) => ConstraintCategory::Predicate,
        CheckedConstraintKind::TraitSatisfaction { .. } => ConstraintCategory::TraitSatisfaction,
        CheckedConstraintKind::TypeEquality { .. } => ConstraintCategory::TypeEquality,
    }
}

pub(super) fn substitute_requirement_trait_application(
    values: &SemanticValueStore,
    containing_trait: TraitApplicationId,
    generic_substitution: Option<GenericSubstitutionId>,
    requirement: TraitApplicationId,
) -> Result<TraitApplicationId, FactQueryError> {
    let trait_substitution = values
        .trait_application_data(containing_trait)
        .substitution();

    let requirement = values
        .substitute_trait_application(requirement, trait_substitution)
        .map_err(FactQueryError::SemanticValueStore)?;

    match generic_substitution {
        Some(substitution) => values
            .substitute_trait_application(requirement, substitution)
            .map_err(FactQueryError::SemanticValueStore),
        None => Ok(requirement),
    }
}
