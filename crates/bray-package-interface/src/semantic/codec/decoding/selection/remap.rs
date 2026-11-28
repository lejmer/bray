use std::sync::Arc;

use super::mapping::RecordMaps;
use super::model::{RecordSet, SelectedRecords};
use crate::InterfaceValidationError;
use crate::semantic::model::{
    InterfaceConstantProjection, InterfaceConstantTerm, InterfaceConstantTermId,
    InterfaceConstantValue, InterfaceConstantValueId, InterfaceConstantValueKind,
    InterfaceDependencyGuard, InterfaceDependencyProjection, InterfaceDependencyRequirement,
    InterfaceDependencyRequirementValue, InterfaceDependencySubject,
    InterfaceDependencySubjectRoot, InterfaceGenericArgument, InterfaceSemantics, InterfaceType,
    InterfaceTypeId,
};

/// Remaps a template's selected values and keeps the slot maps its decoder reads through.
pub(super) fn remap_selected_template_values(
    records: SelectedRecords,
) -> Result<(InterfaceSemantics, crate::ExecutableTemplateSlots), InterfaceValidationError> {
    let slots = RecordMaps::new(&records)?.template_slots();

    Ok((remap_selected_records(records)?, slots))
}

pub(super) fn remap_selected_records(
    records: SelectedRecords,
) -> Result<InterfaceSemantics, InterfaceValidationError> {
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
        .map(|contract| remap_dependency_contract(contract, &maps))
        .collect::<Result<Vec<_>, InterfaceValidationError>>()?;

    let constraints = records
        .constraints
        .into_values()
        .map(|mut constraint| {
            match &mut constraint.kind {
                crate::InterfaceConstraintKind::Predicate(predicate) => {
                    predicate.dependency_contract =
                        maps.dependency_contract_id(predicate.dependency_contract)?;

                    predicate.condition = predicate
                        .condition
                        .map(|term| maps.constant_term_id(term))
                        .transpose()?;
                }
                crate::InterfaceConstraintKind::TraitSatisfaction {
                    subject,
                    application,
                } => {
                    *subject = maps.type_id(*subject)?;
                    *application = maps.trait_application_id(*application)?;
                }
                crate::InterfaceConstraintKind::TypeEquality { left, right } => {
                    *left = maps.type_id(*left)?;
                    *right = maps.type_id(*right)?;
                }
            }

            Ok(constraint)
        })
        .collect::<Result<Vec<_>, InterfaceValidationError>>()?;

    let callable_signatures = records
        .callable_signatures
        .into_values()
        .map(|mut signature| {
            signature.callable_type = maps.type_id(signature.callable_type)?;
            signature.result = maps.type_id(signature.result)?;

            signature.result_dependencies =
                maps.dependency_contract_id(signature.result_dependencies)?;

            if let Some(receiver) = &mut signature.receiver {
                receiver.ty = maps.type_id(receiver.ty)?;
            }

            Ok(signature)
        })
        .collect::<Result<Vec<_>, InterfaceValidationError>>()?;

    let callable_contracts = records
        .callable_contracts
        .into_values()
        .map(|mut contract| {
            remap_callable_contract(&mut contract, &maps)?;

            Ok(contract)
        })
        .collect::<Result<Vec<_>, InterfaceValidationError>>()?;

    let generic_declarations = records.generic_declarations.into_values();
    let callable_parameter_defaults = records.callable_parameter_defaults.into_values();

    let declared_types = records
        .declared_types
        .into_values()
        .map(|declared_type| {
            Ok(crate::InterfaceDeclaredType::new(
                declared_type.owner().clone(),
                maps.type_id(declared_type.ty())?,
            ))
        })
        .collect::<Result<Vec<_>, InterfaceValidationError>>()?;

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

    let runtime_requirements = records.runtime_requirements.into_values();

    let type_representations = records.type_representations.into_values()
        .map(|mut representation| {
            representation.try_map_types(|ty| maps.type_id(ty))?;

            Ok(representation)
        }).collect::<Result<Vec<_>, InterfaceValidationError>>()?;

    let semantics = remap_templates(
        records.checked_templates,
        records.declaration_templates,
        records.support_entities,
        &maps,
    )?;

    Ok(semantics
        .with_applications(
            substitutions,
            trait_applications,
            callable_instances,
            implementation_instances,
        )
        .with_values(dependency_contracts, types, constant_values, constant_terms)
        .with_contracts(constraints, callable_contracts)
        .with_declarations(
            callable_signatures,
            generic_declarations,
            callable_parameter_defaults,
            [],
        )
        .with_declared_types(declared_types)
        .with_type_representations(type_representations)
        .with_implementations(implementations, coherence)
        .with_target_dependencies(target_dependencies, [])
        .with_runtime_requirements(runtime_requirements))
}

fn remap_templates(
    checked_templates: RecordSet<crate::InterfaceCheckedTemplate>,
    declaration_templates: RecordSet<crate::InterfaceDeclarationTemplate>,
    support_entities: RecordSet<crate::semantic::model::InterfaceSupportEntity>,
    maps: &RecordMaps,
) -> Result<InterfaceSemantics, InterfaceValidationError> {
    let checked_templates = checked_templates
        .into_values()
        .map(|template| template.try_map_references(&mut &*maps))
        .collect::<Result<Vec<_>, InterfaceValidationError>>()?;

    let declaration_templates = declaration_templates
        .into_values()
        .map(|declaration| {
            let entity = maps.support_entity_id(declaration.entity())?;

            Ok(declaration.with_entity(entity))
        })
        .collect::<Result<Vec<_>, InterfaceValidationError>>()?;

    let support_entities = support_entities
        .into_values()
        .map(|entity| remap_support_entity(entity, maps))
        .collect::<Result<Vec<_>, InterfaceValidationError>>()?;

    Ok(InterfaceSemantics::new().with_templates(checked_templates, declaration_templates, support_entities))
}

fn remap_support_entity(
    entity: crate::semantic::model::InterfaceSupportEntity,
    maps: &RecordMaps,
) -> Result<crate::semantic::model::InterfaceSupportEntity, InterfaceValidationError> {
    use crate::semantic::model::{InterfaceSupportEntity, InterfaceSupportImplementation};

    Ok(match entity {
        InterfaceSupportEntity::CheckedTemplate(template) => {
            InterfaceSupportEntity::CheckedTemplate(maps.checked_template_id(template)?)
        }
        InterfaceSupportEntity::Implementation(implementation) => {
            InterfaceSupportEntity::Implementation(InterfaceSupportImplementation::new(
                implementation.declaration().clone(),
                maps.type_id(implementation.subject())?,
                implementation.trait_application().map(|application| maps.trait_application_id(application)).transpose()?,
            ))
        }
        InterfaceSupportEntity::Declaration(declaration) => InterfaceSupportEntity::Declaration(declaration),
    })
}

fn remap_dependency_storage(
    borrows: &[bray_symbols::DependencyBorrowRelation<crate::InterfaceDependencySubject, crate::InterfaceDependencyProjection>],
    stored_values: &[bray_symbols::DependencyStoredValue<crate::InterfaceDependencySubject>],
    maps: &RecordMaps,
) -> Result<(
    Vec<bray_symbols::DependencyBorrowRelation<crate::InterfaceDependencySubject, crate::InterfaceDependencyProjection>>,
    Vec<bray_symbols::DependencyStoredValue<crate::InterfaceDependencySubject>>,
), InterfaceValidationError> {
    let stored_values = stored_values.iter().map(|stored| {
        let mut subject = stored.destination().subject().clone();

        remap_dependency_subject(&mut subject, maps)?;

        let destination = bray_symbols::DependencyStoredValuePath::new(subject, stored.destination().dereferences());

        let sources = stored.sources().iter().map(|source| {
            let mut subject = source.subject().clone();

            remap_dependency_subject(&mut subject, maps)?;

            Ok(bray_symbols::DependencyStoredValuePath::new(subject, source.dereferences()))
        }).collect::<Result<Vec<_>, InterfaceValidationError>>()?;

        Ok(bray_symbols::DependencyStoredValue::new(destination, sources, stored.is_complete()))
    }).collect::<Result<Vec<_>, InterfaceValidationError>>()?;

    let borrows = borrows
        .iter()
        .map(|relation| {
            // The selected record owns remapped paths independently of the decoded table.
            let mut entry = relation.source_subject().cloned();
            let mut exit = relation.exit().clone();

            if let Some(entry) = &mut entry {
                remap_dependency_subject(entry, maps)?;
            }

            remap_dependency_subject(&mut exit, maps)?;

            let mut projections = relation.target_projections().to_vec();

            remap_dependency_projections(&mut projections, maps)?;

            Ok(
                relation.remap(entry, exit, projections),
            )
        })
        .collect::<Result<Vec<_>, InterfaceValidationError>>()?;

    Ok((borrows, stored_values))
}

fn remap_dependency_contract(
    mut contract: crate::InterfaceDependencyContract,
    maps: &RecordMaps,
) -> Result<crate::InterfaceDependencyContract, InterfaceValidationError> {
    for requirements in std::iter::once(&mut contract.requirements)
        .chain(std::iter::once(&mut contract.synchronous_cleanup))
    {
        remap_dependency_requirements(requirements, maps)?;
    }

    let remap_effect = |effect: &crate::InterfaceStorageEffect| effect.try_map(
        &mut (),
        |_, subject| {
            // The selected artifact owns its remapped subject.
            let mut subject = subject.clone();

            remap_dependency_subject(&mut subject, maps)?;

            Ok(subject)
        },
        |_, projections| {
            let mut projections = projections.to_vec();

            remap_dependency_projections(&mut projections, maps)?;

            Ok(projections.into())
        },
        |_, ty| maps.type_id(ty),
    );

    let remap_binding = |(destination, value): &crate::InterfaceCapturedBinding| {
        let mut destination = destination.clone();

        remap_dependency_subject(&mut destination, maps)?;

        Ok((destination, value.try_map(&mut |input| maps.constant_term_id(*input), &mut |projection| {
                let mut projection = projection.clone();

                remap_dependency_projections(std::slice::from_mut(&mut projection), maps)?;

                Ok::<_, InterfaceValidationError>(projection)
            }, &mut |root| {
                let mut root = root.clone();

                remap_static_reference(&mut root, maps)?;

                Ok(root)
            }, &mut |call| Ok(crate::InterfaceCapturedCallable {
                instance: maps.callable_instance_id(call.instance)?, signature: maps.type_id(call.signature)?,
                receiver: call.receiver.map(|ty| maps.type_id(ty)).transpose()?,
                requirement: call.requirement.map(|(ty, app)| Ok((maps.type_id(ty)?, maps.trait_application_id(app)?))).transpose()?,
                dispatch: call.dispatch.map(|w| maps.implementation_instance_id(w)).transpose()?,
                witnesses: call.witnesses.iter().map(|w| maps.implementation_instance_id(*w)).collect::<Result<_, _>>()?,
            }), &mut |variant| Ok(variant.clone()))?))
    };

    if let Some(rows) = &contract.failure_outcomes {
        let rows = rows.iter().cloned().map(|mut row| {
            row.guard = row.guard.map(|term| maps.constant_term_id(term)).transpose()?;

            row.effects = row.effects.as_deref().map(|effects| effects.iter().map(&remap_effect)
                .collect::<Result<Vec<_>, _>>().map(bray_base::sorted_unique_shared_slice)).transpose()?;

            row.cell_changes = row.cell_changes.as_deref().map(|cells| cells.iter().map(&remap_effect)
                .collect::<Result<Vec<_>, _>>().map(bray_base::sorted_unique_shared_slice)).transpose()?;

            let (references, stored_values) = remap_dependency_storage(&row.references, &row.stored_values, maps)?;

            row.references = references.into();
            row.stored_values = stored_values.into();

            row.writes = bray_base::sorted_unique_shared_slice(row.writes.iter().map(&remap_binding)
                .collect::<Result<Vec<_>, _>>()?);

            if let Some(report) = &mut row.report {
                remap_dependency_requirements(report, maps)?;
            }

            Ok(row)
        }).collect::<Result<Vec<_>, InterfaceValidationError>>()?;

        contract.failure_outcomes = Some(bray_base::sorted_unique_shared_slice(rows));
    }

    contract.captured_values = contract.captured_values.iter().map(|outcome| {
        let guard = outcome.guard.map(|term| maps.constant_term_id(term)).transpose()?;
        let mut report = outcome.report.clone();

        if let Some(requirements) = &mut report {
            remap_dependency_requirements(requirements, maps)?;
        }

        let report_payloads = outcome.report_payloads.iter().cloned().map(|mut payload| {
            remap_dependency_subject(&mut payload.destination, maps)?;
            remap_dependency_requirements(&mut payload.requirements, maps)?;

            Ok(payload)
        }).collect::<Result<Vec<_>, InterfaceValidationError>>()?;

        let bindings = outcome.bindings.iter().map(&remap_binding).collect::<Result<Vec<_>, _>>()?;

        let owned_result = outcome.ownership.try_map(
        &mut |transfer| {
            Ok::<_, InterfaceValidationError>(bray_symbols::DependencyOwnedResult::Transfer(
                bray_symbols::DependencyOwnedTransfer::new(
                    maps.constant_term_id(transfer.input())?,
                    transfer.output().iter().cloned(),
                )
                .with_preservation(transfer.preservation().clone())
                .with_condition(
                    transfer
                        .condition()
                        .map(|condition| maps.constant_term_id(condition))
                        .transpose()?,
                ),
            ))
        },
        &mut |condition| maps.constant_term_id(*condition).map(Some),
    )?;

    let carriers = outcome.carriers
        .iter()
        .map(|carrier| {
            // Selected records own their remapped paths independently of the decoded table.
            let mut path = carrier.path().to_vec();

            remap_dependency_projections(&mut path, maps)?;

            let support = carrier.support().map(|support| support.try_map(
                &mut (),
                |_, owner| {
                    // A selected artifact owns its remapped support routes.
                    let mut owner = owner.clone();

                    remap_dependency_subject(&mut owner, maps)?;

                    Ok(owner)
                },
                |_, address| maps.constant_term_id(*address),
            )).transpose()?;

            Ok(carrier.remap(maps.constant_term_id(carrier.guarantee())?, path).with_support(support))
        })
        .collect::<Result<Vec<_>, InterfaceValidationError>>()?;

        let (references, stored_values) = remap_dependency_storage(&outcome.references, &outcome.stored_values, maps)?;

        let mut inactive_payloads = outcome.inactive_payloads.to_vec();

        for payload in &mut inactive_payloads {
            remap_dependency_subject(payload, maps)?;
        }

        let effects = outcome.effects.iter().map(remap_effect).collect::<Result<Vec<_>, _>>()?;

        let cell_changes = outcome.cell_changes.as_deref().map(|cells| cells.iter().map(&remap_effect)
            .collect::<Result<Vec<_>, _>>().map(bray_base::sorted_unique_shared_slice)).transpose()?;

        Ok(bray_symbols::CapturedValueOutcome::new(guard, bindings).with_effects(effects).with_cell_changes(cell_changes).with_carriers(carriers).with_report(report)
            .with_report_payloads(report_payloads)
            .with_ownership(owned_result).with_storage(references, stored_values)
            .with_inactive_payloads(inactive_payloads))
    }).collect::<Result<_, InterfaceValidationError>>()?;

    contract.effects = bray_base::sorted_unique_shared_slice(
        contract.effects.iter().map(remap_effect).collect::<Result<Vec<_>, _>>()?);

    Ok(contract
)
}

fn remap_callable_contract(
    contract: &mut crate::InterfaceCallableContract,
    maps: &RecordMaps,
) -> Result<(), InterfaceValidationError> {
    for clauses in [
        &mut contract.invocation_preconditions,
        &mut contract.static_constraints,
        &mut contract.normal_completion_postconditions,
    ] {
        for clause in Arc::make_mut(clauses) {
            match &mut clause.value {
                crate::InterfaceCallableContractClauseValue::Predicate(predicate) => {
                    predicate.dependency_contract =
                        maps.dependency_contract_id(predicate.dependency_contract)?;

                    predicate.condition = predicate
                        .condition
                        .map(|term| maps.constant_term_id(term))
                        .transpose()?;
                }
                crate::InterfaceCallableContractClauseValue::TraitSatisfaction {
                    subject,
                    application,
                } => {
                    *subject = maps.type_id(*subject)?;
                    *application = maps.trait_application_id(*application)?;
                }
            }
        }
    }

    for behavior in std::iter::once(&mut contract.invocation_behavior)
        .chain(contract.deferred_execution_behavior.iter_mut())
    {
        remap_behavior(behavior, maps)?;
    }

    contract.execution_contract = contract.execution_contract.try_map(
        &mut (),
        |(), term| maps.constant_term_id(term),
        |(), target| {
            use bray_symbols::CallableExecutionTarget;

            Ok(match target {
                CallableExecutionTarget::Callable(callable) => {
                    CallableExecutionTarget::Callable(maps.callable_instance_id(callable)?)
                }
                CallableExecutionTarget::Requirement { callable, requirement: (subject, application) } => {
                    CallableExecutionTarget::Requirement {
                        callable: maps.callable_instance_id(callable)?,
                        requirement: (maps.type_id(subject)?, maps.trait_application_id(application)?),
                    }
                }
                CallableExecutionTarget::Indirect(ty) => CallableExecutionTarget::Indirect(maps.type_id(ty)?),
            })
        },
    )?;

    Ok(())
}

fn remap_type(ty: &mut InterfaceType, maps: &RecordMaps) -> Result<(), InterfaceValidationError> {
    match ty {
        InterfaceType::Named { substitution, .. } => {
            *substitution = maps.substitution_id(*substitution)?;
        }
        InterfaceType::TypeValuedMemberProjection {
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
        InterfaceType::FlexibleArray(target)
        | InterfaceType::Slice(target)
        | InterfaceType::Generator(target)
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
            invocation_behavior,
            deferred_execution_behavior,
            ..
        } => {
            for parameter in Arc::make_mut(parameters) {
                parameter.ty = maps.type_id(parameter.ty)?;
            }

            *result = maps.type_id(*result)?;

            remap_behavior(Arc::make_mut(invocation_behavior), maps)?;

            if let Some(behavior) = deferred_execution_behavior {
                remap_behavior(Arc::make_mut(behavior), maps)?;
            }
        }
        InterfaceType::TypeParameter(_) | InterfaceType::ContextualSelf(_) => {}
    }

    Ok(())
}

fn remap_behavior(
    behavior: &mut crate::InterfaceCallablePhaseBehavior,
    maps: &RecordMaps,
) -> Result<(), InterfaceValidationError> {
    behavior.receiver_cleanup = bray_base::sorted_unique_shared_slice(
        behavior
            .receiver_cleanup
            .iter()
            .map(|ty| maps.type_id(*ty))
            .collect::<Result<Vec<_>, _>>()?,
    );

    for predicates in [
        &mut behavior.predicate_requirements,
        &mut behavior.predicate_guarantees,
    ] {
        for predicate in Arc::make_mut(predicates) {
            predicate.dependency_contract =
                maps.dependency_contract_id(predicate.dependency_contract)?;

            predicate.condition = predicate
                .condition
                .map(|term| maps.constant_term_id(term))
                .transpose()?;
        }
    }

    behavior.execution_domains = behavior
        .execution_domains
        .iter()
        .map(|domain| {
            domain.try_map(|mut predicate| {
                predicate.dependency_contract =
                    maps.dependency_contract_id(predicate.dependency_contract)?;

                predicate.condition = predicate
                    .condition
                    .map(|term| maps.constant_term_id(term))
                    .transpose()?;

                Ok::<_, InterfaceValidationError>(predicate)
            })
        })
        .collect::<Result<Vec<_>, _>>()?
        .into();

    behavior.dependency_contract = maps.dependency_contract_id(behavior.dependency_contract)?;

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
        InterfaceConstantValueKind::Tuple(values) | InterfaceConstantValueKind::Array(values) => {
            remap_constant_value_ids(Arc::make_mut(values), maps)?;
        }
        InterfaceConstantValueKind::Product(fields) => {
            remap_constant_value_fields(Arc::make_mut(fields), maps)?;
        }
        InterfaceConstantValueKind::Union { fields, .. } => {
            remap_constant_value_fields(Arc::make_mut(fields), maps)?;
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
        InterfaceConstantTerm::Typed { term, ty } => {
            *term = maps.constant_term_id(*term)?;
            *ty = maps.type_id(*ty)?;
        }
        InterfaceConstantTerm::Value(value) => {
            *value = maps.constant_value_id(*value)?;
        }
        InterfaceConstantTerm::Unary { operand, .. } => {
            *operand = maps.constant_term_id(*operand)?;
        }
        InterfaceConstantTerm::RepeatedArray {
            value: left,
            count: right,
        }
        | InterfaceConstantTerm::Binary { left, right, .. } => {
            *left = maps.constant_term_id(*left)?;
            *right = maps.constant_term_id(*right)?;
        }
        InterfaceConstantTerm::Conversion { operand, target } => {
            *operand = maps.constant_term_id(*operand)?;
            *target = maps.type_id(*target)?;
        }
        InterfaceConstantTerm::VariantTest { subject: value, .. }
        | InterfaceConstantTerm::NullablePresent(value) => {
            *value = maps.constant_term_id(*value)?;
        }
        InterfaceConstantTerm::Tuple(values) | InterfaceConstantTerm::Array(values) => {
            remap_constant_term_ids(Arc::make_mut(values), maps)?;
        }
        InterfaceConstantTerm::Product(fields) => {
            remap_constant_term_fields(Arc::make_mut(fields), maps)?;
        }
        InterfaceConstantTerm::Union { fields, .. } => {
            remap_constant_term_fields(Arc::make_mut(fields), maps)?;
        }
        InterfaceConstantTerm::TraitConstant {
            subject,
            application,
            ..
        } => {
            *subject = maps.type_id(*subject)?;
            *application = maps.trait_application_id(*application)?;
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
            selected_implementation,
            arguments,
            implementation_witnesses,
        } => {
            *callable = maps.callable_instance_id(*callable)?;

            *selected_implementation = selected_implementation
                .map(|implementation| maps.implementation_instance_id(implementation))
                .transpose()?;

            for witness in Arc::make_mut(implementation_witnesses) {
                *witness = maps.implementation_instance_id(*witness)?;
            }

            remap_constant_term_ids(Arc::make_mut(arguments), maps)?;
        }
        InterfaceConstantTerm::PredicateCall {
            substitution,
            arguments,
            ..
        } => {
            *substitution = maps.substitution_id(*substitution)?;
            remap_constant_term_ids(Arc::make_mut(arguments), maps)?;
        }
        InterfaceConstantTerm::Projection { subject, kind } => {
            *subject = maps.constant_term_id(*subject)?;

            if let InterfaceConstantProjection::ArrayElement(index) = kind {
                *index = maps.constant_term_id(*index)?;
            }

            if let InterfaceConstantProjection::ArraySlice { lower, upper } = kind {
                for bound in [lower, upper].into_iter().flatten() {
                    *bound = maps.constant_term_id(*bound)?;
                }
            }
        }
        InterfaceConstantTerm::IntegerLiteral { .. }
        | InterfaceConstantTerm::CallableArgument(_)
        | InterfaceConstantTerm::Parameter(_)
        | InterfaceConstantTerm::TargetProperty(_) => {}
    }

    Ok(())
}

fn remap_dependency_requirements(
    requirements: &mut Arc<[InterfaceDependencyRequirement]>,
    maps: &RecordMaps,
) -> Result<(), InterfaceValidationError> {
    let requirements = Arc::make_mut(requirements);

    for requirement in &mut *requirements {
        remap_dependency_requirement(requirement, maps)?;
    }

    requirements.sort_unstable();

    Ok(())
}

fn remap_dependency_requirement(
    requirement: &mut InterfaceDependencyRequirement,
    maps: &RecordMaps,
) -> Result<(), InterfaceValidationError> {
    match &mut requirement.value {
        InterfaceDependencyRequirementValue::CleanupDependencies { ty, requirements } => {
            *ty = maps.type_id(*ty)?;

            remap_dependency_requirements(requirements, maps)?;
        }
        InterfaceDependencyRequirementValue::SynchronousCleanup(ty)
        | InterfaceDependencyRequirementValue::FinalizerBodyCleanup(ty) => {
            *ty = maps.type_id(*ty)?;
        }
        InterfaceDependencyRequirementValue::DefaultCleanup {
            substitution,
            contextual_self,
            ..
        } => {
            *substitution = maps.substitution_id(*substitution)?;

            if let Some((_, subject)) = contextual_self {
                *subject = maps.type_id(*subject)?;
            }
        }
        InterfaceDependencyRequirementValue::Variable { .. } => {}
        InterfaceDependencyRequirementValue::FixedPoint {
            definitions,
            result,
        } => {
            for definition in Arc::make_mut(definitions)
                .iter_mut()
                .chain(std::iter::once(result))
            {
                for nested in Arc::make_mut(definition) {
                    remap_dependency_requirement(nested, maps)?;
                }
            }
        }
        InterfaceDependencyRequirementValue::ResultCall {
            output: _,
            callable,
            requirement,
            inputs,
        } => {
            *callable = maps.callable_instance_id(*callable)?;

            if let Some((subject, application)) = requirement {
                *subject = maps.type_id(*subject)?;
                *application = maps.trait_application_id(*application)?;
            }

            remap_dependency_call_inputs(inputs, maps)?;
        }
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

fn remap_dependency_call_inputs(
    inputs: &mut Arc<[crate::InterfaceDependencyCallInput]>,
    maps: &RecordMaps,
) -> Result<(), InterfaceValidationError> {
    for input in Arc::make_mut(inputs) {
        for nested in Arc::make_mut(&mut input.values)
            .iter_mut()
            .chain(Arc::make_mut(&mut input.storage))
        {
            remap_dependency_requirement(nested, maps)?;
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

    if let InterfaceDependencySubjectRoot::ImplementationRequirement(ty, application) =
        &mut subject.root
    {
        *ty = maps.type_id(*ty)?;
        *application = maps.trait_application_id(*application)?;
    }

    if let InterfaceDependencySubjectRoot::StaticInstance(reference, _) = &mut subject.root {
        remap_static_reference(reference, maps)?;
    }

    remap_dependency_projections(Arc::make_mut(&mut subject.projections), maps)
}

fn remap_static_reference(
    reference: &mut crate::InterfaceStaticReference, maps: &RecordMaps,
) -> Result<(), InterfaceValidationError> {
    reference.substitution = maps.substitution_id(reference.substitution)?;

    reference.selected_witnesses = reference.selected_witnesses.iter().map(|witness|
        maps.implementation_instance_id(*witness)).collect::<Result<_, _>>()?;

    Ok(())
}

fn remap_dependency_projections(
    projections: &mut [InterfaceDependencyProjection],
    maps: &RecordMaps,
) -> Result<(), InterfaceValidationError> {
    for projection in projections {
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

fn remap_constant_value_fields(
    fields: &mut [bray_symbols::ConstantField<
        crate::InterfaceSymbolReference,
        InterfaceConstantValueId,
    >],
    maps: &RecordMaps,
) -> Result<(), InterfaceValidationError> {
    for field in fields {
        // Field identity remains unchanged while the selected record ID is remapped.
        let identity = field.field().clone();

        *field =
            bray_symbols::ConstantField::new(identity, maps.constant_value_id(*field.value())?);
    }

    Ok(())
}

fn remap_constant_term_fields(
    fields: &mut [bray_symbols::ConstantField<
        crate::InterfaceSymbolReference,
        InterfaceConstantTermId,
    >],
    maps: &RecordMaps,
) -> Result<(), InterfaceValidationError> {
    for field in fields {
        // Field identity remains unchanged while the selected record ID is remapped.
        let identity = field.field().clone();

        *field = bray_symbols::ConstantField::new(identity, maps.constant_term_id(*field.value())?);
    }

    Ok(())
}
