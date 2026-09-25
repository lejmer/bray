use std::collections::BTreeMap;

use bray_symbols::{
    DependencyContractTemplateId, DependencyRequirement, DependencyRequirementKind,
    DependencySubjectRoot, GenericArgument, GenericSubstitutionId, SemanticValueStore, TraitApplicationId,
    TraitTypeMemberSymbolId, TypeData, TypeExpressionTemplate, TypeId,
};

use crate::fact::FactQueryError;

pub(super) fn type_templates_are_compatible(
    values: &SemanticValueStore,
    subject: TypeId,
    trait_application: TraitApplicationId,
    fulfillment_context: Option<bray_symbols::SelfTypeContext>,
    generic_substitution: Option<GenericSubstitutionId>,
    requirement: &TypeExpressionTemplate,
    fulfillment: &TypeExpressionTemplate,
    type_bindings: &BTreeMap<TraitTypeMemberSymbolId, TypeExpressionTemplate>,
) -> Result<bool, FactQueryError> {
    let normalized_fulfillment;

    let fulfillment = match (fulfillment, fulfillment_context) {
        (TypeExpressionTemplate::Resolved(ty), Some(context)) => {
            normalized_fulfillment = TypeExpressionTemplate::Resolved(
                values
                    .substitute_contextual_self(*ty, context, subject)
                    .map_err(FactQueryError::SemanticValueStore)?,
            );

            &normalized_fulfillment
        }
        _ => fulfillment,
    };

    match requirement {
        TypeExpressionTemplate::Resolved(requirement) => {
            let requirement = substitute_requirement_type(
                values,
                subject,
                trait_application,
                generic_substitution,
                *requirement,
            )?;

            let requirement_data = values.type_data(requirement);

            if let TypeData::Named {
                definition: required_definition,
                substitution: required_substitution,
            } = requirement_data.as_ref()
            {
                let Some(fulfillment) = fulfillment.resolved_type() else {
                    return Ok(false);
                };

                let fulfillment_data = values.type_data(fulfillment);

                let TypeData::Named {
                    definition: provided_definition,
                    substitution: provided_substitution,
                } = fulfillment_data.as_ref()
                else {
                    return Ok(false);
                };

                if required_definition != provided_definition {
                    return Ok(false);
                }

                let required = values.generic_substitution_data(*required_substitution);
                let provided = values.generic_substitution_data(*provided_substitution);

                if required.bindings().len() != provided.bindings().len() {
                    return Ok(false);
                }

                for (required, provided) in required.bindings().iter().zip(provided.bindings()) {
                    match (required.argument(), provided.argument()) {
                        (GenericArgument::Type(required), GenericArgument::Type(provided)) => {
                            if !type_templates_are_compatible(
                                values,
                                subject,
                                trait_application,
                                fulfillment_context,
                                generic_substitution,
                                &TypeExpressionTemplate::Resolved(required),
                                &TypeExpressionTemplate::Resolved(provided),
                                type_bindings,
                            )? {
                                return Ok(false);
                            }
                        }
                        (required, provided) if required == provided => {}
                        _ => return Ok(false),
                    }
                }

                return Ok(true);
            }

            if let TypeData::Nullable(requirement) = requirement_data.as_ref() {
                let Some(fulfillment) = fulfillment.resolved_type() else {
                    return Ok(false);
                };

                let fulfillment_data = values.type_data(fulfillment);

                let TypeData::Nullable(fulfillment) = fulfillment_data.as_ref() else {
                    return Ok(false);
                };

                return type_templates_are_compatible(
                    values,
                    subject,
                    trait_application,
                    fulfillment_context,
                    generic_substitution,
                    &TypeExpressionTemplate::Resolved(*requirement),
                    &TypeExpressionTemplate::Resolved(*fulfillment),
                    type_bindings,
                );
            }

            if let TypeData::Borrow {
                kind: requirement_kind,
                target: requirement,
            } = requirement_data.as_ref()
            {
                let Some(fulfillment) = fulfillment.resolved_type() else {
                    return Ok(false);
                };

                let fulfillment_data = values.type_data(fulfillment);

                let TypeData::Borrow {
                    kind: fulfillment_kind,
                    target: fulfillment,
                } = fulfillment_data.as_ref()
                else {
                    return Ok(false);
                };

                if requirement_kind != fulfillment_kind {
                    return Ok(false);
                }

                return type_templates_are_compatible(
                    values,
                    subject,
                    trait_application,
                    fulfillment_context,
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
                    subject,
                    trait_application,
                    fulfillment_context,
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
                subject,
                trait_application,
                fulfillment_context,
                generic_substitution,
                requirement,
                fulfillment,
                type_bindings,
            )
        }
        TypeExpressionTemplate::Nullable(requirement) => nullable_templates_are_compatible(
            values,
            subject,
            trait_application,
            fulfillment_context,
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
    subject: TypeId,
    trait_application: TraitApplicationId,
    fulfillment_context: Option<bray_symbols::SelfTypeContext>,
    generic_substitution: Option<GenericSubstitutionId>,
    requirement: &TypeExpressionTemplate,
    fulfillment: &TypeExpressionTemplate,
    type_bindings: &BTreeMap<TraitTypeMemberSymbolId, TypeExpressionTemplate>,
) -> Result<bool, FactQueryError> {
    match fulfillment {
        TypeExpressionTemplate::Nullable(fulfillment) => type_templates_are_compatible(
            values,
            subject,
            trait_application,
            fulfillment_context,
            generic_substitution,
            requirement,
            fulfillment,
            type_bindings,
        ),
        TypeExpressionTemplate::Resolved(fulfillment) => {
            let data = values.type_data(*fulfillment);

            let TypeData::Nullable(fulfillment) = data.as_ref() else {
                return Ok(false);
            };

            type_templates_are_compatible(
                values,
                subject,
                trait_application,
                fulfillment_context,
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
    subject: TypeId,
    trait_application: TraitApplicationId,
    generic_substitution: Option<GenericSubstitutionId>,
    requirement: TypeId,
) -> Result<TypeId, FactQueryError> {
    let application = values.trait_application_data(trait_application);

    let requirement = values
        .substitute_type(requirement, application.substitution())
        .map_err(FactQueryError::SemanticValueStore)?;

    let requirement = values
        .substitute_contextual_self(
            requirement,
            bray_symbols::SelfTypeContext::Trait(application.definition()),
            subject,
        )
        .map_err(FactQueryError::SemanticValueStore)?;

    match generic_substitution {
        Some(substitution) => values
            .substitute_type(requirement, substitution)
            .map_err(FactQueryError::SemanticValueStore),
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
    let requirement = substituted_dependency_requirement(
        values, trait_application, generic_substitution, requirement,
    )?;

    Ok(requirement == fulfillment)
}

/// Static storage reached by a concrete body is an environmental requirement, not a trait condition.
pub(super) fn phase_dependencies_are_compatible(
    values: &SemanticValueStore,
    trait_application: TraitApplicationId,
    generic_substitution: Option<GenericSubstitutionId>,
    requirement: DependencyContractTemplateId,
    fulfillment: DependencyContractTemplateId,
) -> Result<bool, FactQueryError> {
    let requirement = substituted_dependency_requirement(
        values, trait_application, generic_substitution, requirement,
    )?;

    let required = values.dependency_contract_template_data(requirement);
    let provided = values.dependency_contract_template_data(fulfillment);

    Ok(required.requirements().iter().filter(|item| !is_static_access(item)).eq(
        provided.requirements().iter().filter(|item| !is_static_access(item)),
    ))
}

fn is_static_access(requirement: &DependencyRequirement) -> bool {
    matches!(
        requirement,
        DependencyRequirement::Direct { subject, kind: DependencyRequirementKind::StorageAlive }
            if matches!(
                subject.subject_root(),
                DependencySubjectRoot::ProductStatic(_) | DependencySubjectRoot::ExactThreadStatic(_)
            )
    )
}

fn substituted_dependency_requirement(
    values: &SemanticValueStore,
    trait_application: TraitApplicationId,
    generic_substitution: Option<GenericSubstitutionId>,
    requirement: DependencyContractTemplateId,
) -> Result<DependencyContractTemplateId, FactQueryError> {
    let trait_substitution = values
        .trait_application_data(trait_application)
        .substitution();

    let requirement = values
        .substitute_dependency_contract(requirement, trait_substitution)
        .map_err(FactQueryError::SemanticValueStore)?;

    match generic_substitution {
        Some(substitution) => values
            .substitute_dependency_contract(requirement, substitution)
            .map_err(FactQueryError::SemanticValueStore),
        None => Ok(requirement),
    }
}
