use std::sync::Arc;

use super::mapping::RecordMaps;
use super::model::SelectedRecords;
use crate::InterfaceValidationError;
use crate::semantic::model::{
    InterfaceConstantProjection, InterfaceConstantTerm, InterfaceConstantTermId,
    InterfaceConstantValue, InterfaceConstantValueId, InterfaceConstantValueKind,
    InterfaceDependencyGuard, InterfaceDependencyProjection, InterfaceDependencyRequirement,
    InterfaceDependencyRequirementValue, InterfaceDependencySubject,
    InterfaceDependencySubjectRoot, InterfaceGenericArgument, InterfaceSemanticFacts,
    InterfaceType, InterfaceTypeId,
};

pub(super) fn remap_selected_records(
    records: SelectedRecords,
) -> Result<InterfaceSemanticFacts, InterfaceValidationError> {
    let maps = RecordMaps::new(&records)?;

    let substitutions = records
        .substitutions
        .into_values()
        .map(|mut substitution| {
            for binding in Arc::make_mut(&mut substitution.bindings) {
                binding.argument = match binding.argument {
                    InterfaceGenericArgument::Type(ty) => {
                        InterfaceGenericArgument::Type(maps.type_id(ty)?)
                    }
                    InterfaceGenericArgument::Constant(term) => {
                        InterfaceGenericArgument::Constant(maps.constant_term_id(term)?)
                    }
                };
            }

            Ok(substitution)
        })
        .collect::<Result<Vec<_>, InterfaceValidationError>>()?;

    let trait_applications = records
        .trait_applications
        .into_values()
        .map(|mut application| {
            application.substitution = maps.substitution_id(application.substitution)?;

            Ok(application)
        })
        .collect::<Result<Vec<_>, InterfaceValidationError>>()?;

    let callable_instances = records
        .callable_instances
        .into_values()
        .map(|mut instance| {
            instance.substitution = maps.substitution_id(instance.substitution)?;

            Ok(instance)
        })
        .collect::<Result<Vec<_>, InterfaceValidationError>>()?;

    let implementation_instances = records
        .implementation_instances
        .into_values()
        .map(|mut instance| {
            instance.substitution = maps.substitution_id(instance.substitution)?;

            Ok(instance)
        })
        .collect::<Result<Vec<_>, InterfaceValidationError>>()?;

    let types = records
        .types
        .into_values()
        .map(|mut ty| {
            remap_type(&mut ty, &maps)?;

            Ok(ty)
        })
        .collect::<Result<Vec<_>, InterfaceValidationError>>()?;

    let constant_values = records
        .constant_values
        .into_values()
        .map(|mut value| {
            remap_constant_value(&mut value, &maps)?;

            Ok(value)
        })
        .collect::<Result<Vec<_>, InterfaceValidationError>>()?;

    let constant_terms = records
        .constant_terms
        .into_values()
        .map(|mut term| {
            remap_constant_term(&mut term, &maps)?;

            Ok(term)
        })
        .collect::<Result<Vec<_>, InterfaceValidationError>>()?;

    let dependency_contracts = records
        .dependency_contracts
        .into_values()
        .map(|mut contract| {
            for requirement in Arc::make_mut(&mut contract.requirements) {
                remap_dependency_requirement(requirement, &maps)?;
            }

            Ok(contract)
        })
        .collect::<Result<Vec<_>, InterfaceValidationError>>()?;

    let constraints = records
        .constraints
        .into_values()
        .map(|mut constraint| {
            constraint.predicate.dependency_contract =
                maps.dependency_contract_id(constraint.predicate.dependency_contract)?;

            Ok(constraint)
        })
        .collect::<Result<Vec<_>, InterfaceValidationError>>()?;

    let callable_signatures = records
        .callable_signatures
        .into_values()
        .map(|mut signature| {
            signature.callable_type = maps.type_id(signature.callable_type)?;
            signature.result = maps.type_id(signature.result)?;

            if let Some(receiver) = &mut signature.receiver {
                receiver.ty = maps.type_id(receiver.ty)?;
            }

            Ok(signature)
        })
        .collect::<Result<Vec<_>, InterfaceValidationError>>()?;

    let generic_declarations = records.generic_declarations.into_values();
    let callable_parameter_defaults = records.callable_parameter_defaults.into_values();

    let implementations = records
        .implementations
        .into_values()
        .map(|mut implementation| {
            implementation.subject = maps.type_id(implementation.subject)?;
            implementation.trait_application = implementation
                .trait_application
                .map(|application| maps.trait_application_id(application))
                .transpose()?;

            Ok(implementation)
        })
        .collect::<Result<Vec<_>, InterfaceValidationError>>()?;

    let coherence = records
        .coherence
        .into_values()
        .map(|mut coherence| {
            coherence.subject = maps.type_id(coherence.subject)?;
            coherence.trait_application = maps.trait_application_id(coherence.trait_application)?;

            Ok(coherence)
        })
        .collect::<Result<Vec<_>, InterfaceValidationError>>()?;

    let target_dependencies = records
        .target_dependencies
        .into_values()
        .map(|mut target| {
            target.value = maps.constant_value_id(target.value)?;

            Ok(target)
        })
        .collect::<Result<Vec<_>, InterfaceValidationError>>()?;

    Ok(InterfaceSemanticFacts::new()
        .with_applications(
            substitutions,
            trait_applications,
            callable_instances,
            implementation_instances,
        )
        .with_values(dependency_contracts, types, constant_values, constant_terms)
        .with_contracts(constraints, [])
        .with_declarations(
            callable_signatures,
            generic_declarations,
            callable_parameter_defaults,
            [],
        )
        .with_implementations(implementations, coherence)
        .with_target_dependencies(target_dependencies, []))
}

fn remap_type(ty: &mut InterfaceType, maps: &RecordMaps) -> Result<(), InterfaceValidationError> {
    match ty {
        InterfaceType::Named { substitution, .. } => {
            *substitution = maps.substitution_id(*substitution)?;
        }
        InterfaceType::AssociatedTypeProjection {
            subject,
            application,
            ..
        } => {
            *subject = maps.type_id(*subject)?;
            *application = maps.trait_application_id(*application)?;
        }
        InterfaceType::Tuple(elements) => {
            remap_type_ids(Arc::make_mut(elements), maps)?;
        }
        InterfaceType::Array { element, length } => {
            *element = maps.type_id(*element)?;
            *length = maps.constant_term_id(*length)?;
        }
        InterfaceType::Slice(target)
        | InterfaceType::Nullable(target)
        | InterfaceType::Borrow { target, .. } => {
            *target = maps.type_id(*target)?;
        }
        InterfaceType::TraitView(application) => {
            *application = maps.trait_application_id(*application)?;
        }
        InterfaceType::OwnedIndirection { storage, target } => {
            *storage = maps.type_id(*storage)?;
            *target = maps.type_id(*target)?;
        }
        InterfaceType::Callable {
            parameters,
            result,
            invocation_dependency_contract,
            deferred_dependency_contract,
            ..
        } => {
            for parameter in Arc::make_mut(parameters) {
                parameter.ty = maps.type_id(parameter.ty)?;
            }

            *result = maps.type_id(*result)?;
            *invocation_dependency_contract =
                maps.dependency_contract_id(*invocation_dependency_contract)?;
            *deferred_dependency_contract = deferred_dependency_contract
                .map(|contract| maps.dependency_contract_id(contract))
                .transpose()?;
        }
        InterfaceType::TypeParameter(_) | InterfaceType::ContextualSelf(_) => {}
    }

    Ok(())
}

fn remap_constant_value(
    value: &mut InterfaceConstantValue,
    maps: &RecordMaps,
) -> Result<(), InterfaceValidationError> {
    value.ty = maps.type_id(value.ty)?;

    match &mut value.kind {
        InterfaceConstantValueKind::NullablePresent(value) => {
            *value = maps.constant_value_id(*value)?;
        }
        InterfaceConstantValueKind::Tuple(values)
        | InterfaceConstantValueKind::Array(values)
        | InterfaceConstantValueKind::Product(values) => {
            remap_constant_value_ids(Arc::make_mut(values), maps)?;
        }
        InterfaceConstantValueKind::Union { fields, .. } => {
            remap_constant_value_ids(Arc::make_mut(fields), maps)?;
        }
        InterfaceConstantValueKind::Boolean(_)
        | InterfaceConstantValueKind::Character(_)
        | InterfaceConstantValueKind::Integer(_)
        | InterfaceConstantValueKind::Real(_)
        | InterfaceConstantValueKind::Complex { .. }
        | InterfaceConstantValueKind::String(_)
        | InterfaceConstantValueKind::Unit
        | InterfaceConstantValueKind::NullableAbsent => {}
    }

    Ok(())
}

fn remap_constant_term(
    term: &mut InterfaceConstantTerm,
    maps: &RecordMaps,
) -> Result<(), InterfaceValidationError> {
    match term {
        InterfaceConstantTerm::Value(value) => {
            *value = maps.constant_value_id(*value)?;
        }
        InterfaceConstantTerm::Unary { operand, .. } => {
            *operand = maps.constant_term_id(*operand)?;
        }
        InterfaceConstantTerm::Binary { left, right, .. } => {
            *left = maps.constant_term_id(*left)?;
            *right = maps.constant_term_id(*right)?;
        }
        InterfaceConstantTerm::Conversion { operand, target } => {
            *operand = maps.constant_term_id(*operand)?;
            *target = maps.type_id(*target)?;
        }
        InterfaceConstantTerm::DefinitionApplication {
            substitution,
            selected_implementation,
            ..
        } => {
            *substitution = maps.substitution_id(*substitution)?;
            *selected_implementation = selected_implementation
                .map(|implementation| maps.implementation_instance_id(implementation))
                .transpose()?;
        }
        InterfaceConstantTerm::Call {
            callable,
            arguments,
        } => {
            *callable = maps.callable_instance_id(*callable)?;
            remap_constant_term_ids(Arc::make_mut(arguments), maps)?;
        }
        InterfaceConstantTerm::Projection { subject, kind } => {
            *subject = maps.constant_term_id(*subject)?;

            if let InterfaceConstantProjection::ArrayElement(index) = kind {
                *index = maps.constant_term_id(*index)?;
            }
        }
        InterfaceConstantTerm::IntegerLiteral { .. }
        | InterfaceConstantTerm::Parameter(_)
        | InterfaceConstantTerm::TargetFact(_) => {}
    }

    Ok(())
}

fn remap_dependency_requirement(
    requirement: &mut InterfaceDependencyRequirement,
    maps: &RecordMaps,
) -> Result<(), InterfaceValidationError> {
    match &mut requirement.value {
        InterfaceDependencyRequirementValue::Direct { subject, .. } => {
            remap_dependency_subject(subject, maps)?;
        }
        InterfaceDependencyRequirementValue::Guarded {
            guard,
            requirements,
        } => {
            match guard {
                InterfaceDependencyGuard::NullablePresent(subject)
                | InterfaceDependencyGuard::ActiveUnionVariant { subject, .. } => {
                    remap_dependency_subject(subject, maps)?;
                }
            }

            for requirement in Arc::make_mut(requirements) {
                remap_dependency_requirement(requirement, maps)?;
            }
        }
    }

    Ok(())
}

fn remap_dependency_subject(
    subject: &mut InterfaceDependencySubject,
    maps: &RecordMaps,
) -> Result<(), InterfaceValidationError> {
    if let InterfaceDependencySubjectRoot::ImplementationWitness(implementation) = &mut subject.root
    {
        *implementation = maps.implementation_instance_id(*implementation)?;
    }

    for projection in Arc::make_mut(&mut subject.projections) {
        if let InterfaceDependencyProjection::Element(term) = projection {
            *term = maps.constant_term_id(*term)?;
        }
    }

    Ok(())
}

fn remap_type_ids(
    ids: &mut [InterfaceTypeId],
    maps: &RecordMaps,
) -> Result<(), InterfaceValidationError> {
    for id in ids {
        *id = maps.type_id(*id)?;
    }

    Ok(())
}

fn remap_constant_value_ids(
    ids: &mut [InterfaceConstantValueId],
    maps: &RecordMaps,
) -> Result<(), InterfaceValidationError> {
    for id in ids {
        *id = maps.constant_value_id(*id)?;
    }

    Ok(())
}

fn remap_constant_term_ids(
    ids: &mut [InterfaceConstantTermId],
    maps: &RecordMaps,
) -> Result<(), InterfaceValidationError> {
    for id in ids {
        *id = maps.constant_term_id(*id)?;
    }

    Ok(())
}
