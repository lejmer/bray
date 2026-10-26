use super::common::{decode_tag, validate_record_count};
use crate::semantic::codec::common::{
    REFERENCE_EXACT, REFERENCE_INPUT_ABSENT, REFERENCE_KIND_SHIFT, SemanticDecodeContext,
    map_wire_error, read_count, read_optional_u32, read_symbol_reference, read_symbol_references,
    read_u32,
};
use crate::semantic::codec::record::RecordTable;
use crate::semantic::model::{
    InterfaceCallableContract, InterfaceCallableContractClause, InterfaceCallablePhaseBehavior,
    InterfaceConstantTermId, InterfaceConstraint, InterfaceDependencyContract,
    InterfaceDependencyContractId, InterfaceDependencyGuard, InterfaceDependencyProjection,
    InterfaceDependencyRequirement, InterfaceDependencyRequirementKind, InterfaceDependencySubject,
    InterfaceDependencySubjectRoot, InterfaceImplementationInstanceId, InterfacePredicateSummary,
    InterfaceSemantics, InterfaceTrustedCapabilityRequirement,
};
use crate::wire::WireReader;
use crate::{
    InterfaceLimit, InterfaceValidationError, InterfaceValidationLimits, ValidatedInterfaceSection,
};
use bray_symbols::{CallableContractClauseKind, SymbolOrdinal};

pub(super) struct ContractRecordTables<'bytes> {
    pub(super) dependencies: RecordTable<'bytes>,
    pub(super) constraints: RecordTable<'bytes>,
    pub(super) callables: RecordTable<'bytes>,
}

pub(super) fn decode_contract_tables<'bytes>(
    section: ValidatedInterfaceSection<'bytes>,
    context: &mut SemanticDecodeContext,
) -> Result<ContractRecordTables<'bytes>, InterfaceValidationError> {
    let mut reader = WireReader::new(section.bytes());

    let dependencies = RecordTable::read_from(&mut reader, context, section.tag())?;
    let constraints = RecordTable::read_from(&mut reader, context, section.tag())?;
    let callables = RecordTable::read_from(&mut reader, context, section.tag())?;

    validate_record_count(
        section,
        [dependencies.len(), constraints.len(), callables.len()],
    )?;

    reader.finish().map_err(map_wire_error)?;

    Ok(ContractRecordTables {
        dependencies,
        constraints,
        callables,
    })
}

pub(super) fn decode_contracts(
    section: ValidatedInterfaceSection<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
    semantics: &mut InterfaceSemantics,
) -> Result<(), InterfaceValidationError> {
    let tables = decode_contract_tables(section, context)?;

    let dependencies = tables.dependencies.decode_all(context, |reader, context| {
        decode_dependency_contract(reader, limits, context)
    })?;

    let constraints = tables.constraints.decode_all(context, decode_constraint)?;

    let callables = tables.callables.decode_all(context, |reader, context| {
        decode_callable_contract(reader, limits, context)
    })?;

    semantics.dependency_contracts = dependencies.into();
    semantics.constraints = constraints.into();
    semantics.callable_contracts = callables.into();

    Ok(())
}

pub(super) fn decode_dependency_contract(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
) -> Result<InterfaceDependencyContract, InterfaceValidationError> {
    let count = read_count(reader, limits, InterfaceLimit::RecordCount)?;
    let mut synchronous_cleanup = context.allocate_items(reader, count)?;

    for _ in 0..count {
        synchronous_cleanup.push(decode_dependency_requirement(reader, limits, context, 0)?);
    }

    let normal_completion =
        super::common::decode_bool(reader, crate::InterfaceValidationField::Dependency)?;

    let normal_storage_preserved =
        super::common::decode_bool(reader, crate::InterfaceValidationField::Dependency)?;

    let requirements = decode_dependency_requirements(reader, limits, context, 0)?;
    let mut reports = [None, None];

    for output in &mut reports {
        if super::common::decode_bool(reader, crate::InterfaceValidationField::Dependency)? {
            *output = Some(decode_dependency_requirements(reader, limits, context, 0)?);
        }
    }

    let [completion_reports, abnormal_reports] = reports;

    let count = read_count(reader, limits, InterfaceLimit::RecordCount)?;
    let mut borrows = context.allocate_items(reader, count)?;

    for _ in 0..count {
        let attributes = read_u32(reader)?;
        let independent = attributes & REFERENCE_INPUT_ABSENT != 0;
        let exact = attributes & REFERENCE_EXACT != 0;

        if independent && exact {
            return Err(crate::semantic::codec::invalid_discriminant(
                crate::InterfaceValidationField::Discriminant,
                attributes,
            ));
        }

        let kind = decode_tag(attributes >> REFERENCE_KIND_SHIFT)?;

        if independent && kind != bray_symbols::DependencyReferenceKind::Address {
            return Err(crate::semantic::codec::invalid_value(
                crate::InterfaceValidationField::Dependency,
            ));
        }

        let entry = if independent {
            None
        } else {
            Some(decode_dependency_subject(reader, limits, context)?)
        };

        let exit = decode_dependency_subject(reader, limits, context)?;

        let mut relation =
            bray_symbols::DependencyBorrowRelation::new(entry, exit, kind).with_exactness(exact);

        if !independent {
            relation = relation
                .with_dereferences(read_u32(reader)?)
                .with_target_projections(decode_dependency_projections(reader, limits, context)?);
        }

        borrows.push(relation);
    }

    let returned_input = read_optional_u32(reader)?.map(InterfaceConstantTermId::new);
    let observed_result = read_optional_u32(reader)?.map(InterfaceConstantTermId::new);

    let count = read_count(reader, limits, InterfaceLimit::RecordCount)?;
    let mut carriers = context.allocate_items(reader, count)?;

    for _ in 0..count {
        carriers.push(bray_symbols::DependencyWitnessCarrier::new(
            InterfaceConstantTermId::new(read_u32(reader)?),
            decode_dependency_projections(reader, limits, context)?,
        ));
    }

    let count = read_count(reader, limits, InterfaceLimit::RecordCount)?;
    let mut effects = context.allocate_items(reader, count)?;

    for _ in 0..count {
        use bray_symbols::DependencyStorageEffect;

        let raw = read_u32(reader)?;

        effects.push(match raw {
            0 => DependencyStorageEffect::Unknown,
            6 => DependencyStorageEffect::Ambient,
            5 => DependencyStorageEffect::Escape,
            4 => DependencyStorageEffect::Atomic,
            3 => DependencyStorageEffect::Cleanup(crate::InterfaceTypeId::new(read_u32(reader)?)),
            1 => DependencyStorageEffect::Direct {
                subject: decode_dependency_subject(reader, limits, context)?,
                kind: decode_mutation(reader)?,
            },
            2 => DependencyStorageEffect::Indirect {
                reference: decode_dependency_subject(reader, limits, context)?,
                projections: decode_dependency_projections(reader, limits, context)?.into(),
                kind: decode_mutation(reader)?,
            },
            _ => {
                return Err(crate::semantic::codec::invalid_discriminant(
                    crate::InterfaceValidationField::Dependency,
                    raw,
                ));
            }
        });
    }

    let mut contract = InterfaceDependencyContract::new(requirements);

    if let Some(requirements) = completion_reports {
        contract = contract.with_completion_reports(requirements);
    }

    if let Some(requirements) = abnormal_reports {
        contract = contract.with_abnormal_reports(requirements);
    }

    Ok(contract
        .with_borrows(borrows)
        .with_carriers(carriers)
        .with_returned_input(returned_input)
        .with_observed_result(observed_result)
        .with_effects(effects)
        .with_synchronous_cleanup(synchronous_cleanup)
        .with_normal_completion(normal_completion)
        .with_normal_storage_preservation(normal_storage_preserved))
}

pub(super) fn decode_constraint(
    reader: &mut WireReader<'_>,
    context: &mut SemanticDecodeContext,
) -> Result<InterfaceConstraint, InterfaceValidationError> {
    let owner = read_symbol_reference(reader, context)?;
    let ordinal = SymbolOrdinal::new(read_u32(reader)?);
    let raw = read_u32(reader)?;

    match raw {
        1 => Ok(InterfaceConstraint::new(
            owner,
            ordinal,
            decode_predicate_summary(reader)?,
        )),
        2 => Ok(InterfaceConstraint::trait_satisfaction(
            owner,
            ordinal,
            crate::InterfaceTypeId::new(read_u32(reader)?),
            crate::InterfaceTraitApplicationId::new(read_u32(reader)?),
        )),
        3 => Ok(InterfaceConstraint::type_equality(
            owner,
            ordinal,
            crate::InterfaceTypeId::new(read_u32(reader)?),
            crate::InterfaceTypeId::new(read_u32(reader)?),
        )),
        _ => Err(crate::semantic::codec::invalid_discriminant(
            crate::InterfaceValidationField::Dependency,
            raw,
        )),
    }
}

pub(super) fn decode_callable_contract(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
) -> Result<InterfaceCallableContract, InterfaceValidationError> {
    let owner = read_symbol_reference(reader, context)?;

    let mut decoded_clauses = decode_callable_clauses(
        reader,
        limits,
        context,
        CallableContractClauseKind::Requires,
    )?;

    decoded_clauses.extend(decode_callable_clauses(
        reader,
        limits,
        context,
        CallableContractClauseKind::Static,
    )?);

    decoded_clauses.extend(decode_callable_clauses(
        reader,
        limits,
        context,
        CallableContractClauseKind::Ensures,
    )?);

    let execution_contract = super::execution::decode_execution_contract(reader, limits, context)?;
    let invocation_behavior = decode_callable_behavior(reader, limits, context)?;
    let deferred_execution_behavior_raw = read_u32(reader)?;

    let deferred_execution_behavior = match deferred_execution_behavior_raw {
        0 => None,
        1 => Some(decode_callable_behavior(reader, limits, context)?),
        _ => {
            return Err(crate::semantic::codec::invalid_discriminant(
                crate::InterfaceValidationField::Dependency,
                deferred_execution_behavior_raw,
            ));
        }
    };

    Ok(InterfaceCallableContract::new(
        owner,
        decoded_clauses,
        invocation_behavior,
        deferred_execution_behavior,
    )
    .with_execution_contract(execution_contract))
}

fn decode_callable_clauses(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
    kind: CallableContractClauseKind,
) -> Result<Vec<InterfaceCallableContractClause>, InterfaceValidationError> {
    let count = read_count(reader, limits, InterfaceLimit::RecordCount)?;
    let mut clauses = context.allocate_items(reader, count)?;

    for _ in 0..count {
        let ordinal = SymbolOrdinal::new(read_u32(reader)?);
        let raw = read_u32(reader)?;

        let clause = match raw {
            1 => InterfaceCallableContractClause::new(
                ordinal,
                kind,
                decode_predicate_summary(reader)?,
            ),
            2 if kind == CallableContractClauseKind::Static => {
                InterfaceCallableContractClause::trait_satisfaction(
                    ordinal,
                    crate::InterfaceTypeId::new(read_u32(reader)?),
                    crate::InterfaceTraitApplicationId::new(read_u32(reader)?),
                )
            }
            _ => {
                return Err(crate::semantic::codec::invalid_discriminant(
                    crate::InterfaceValidationField::Dependency,
                    raw,
                ));
            }
        };

        clauses.push(clause);
    }

    Ok(clauses)
}

pub(super) fn decode_callable_behavior(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
) -> Result<InterfaceCallablePhaseBehavior, InterfaceValidationError> {
    let mut predicate_sets = Vec::with_capacity(2);

    for _ in 0..2 {
        let count = read_count(reader, limits, InterfaceLimit::RecordCount)?;
        let mut predicates = context.allocate_items(reader, count)?;

        for _ in 0..count {
            predicates.push(decode_predicate_summary(reader)?);
        }

        predicate_sets.push(predicates);
    }

    let effects = read_symbol_references(reader, limits, context)?;
    let capabilities = read_symbol_references(reader, limits, context)?;
    let execution_requirements = read_symbol_references(reader, limits, context)?;

    let trusted_count = read_count(reader, limits, InterfaceLimit::RecordCount)?;

    let mut trusted_capabilities = context.allocate_items(reader, trusted_count)?;

    for _ in 0..trusted_count {
        trusted_capabilities.push(InterfaceTrustedCapabilityRequirement::new(
            SymbolOrdinal::new(read_u32(reader)?),
            read_symbol_reference(reader, context)?,
        ));
    }

    let lifecycle_count = read_count(reader, limits, InterfaceLimit::RecordCount)?;
    let mut lifecycle_obligations = context.allocate_items(reader, lifecycle_count)?;

    for _ in 0..lifecycle_count {
        lifecycle_obligations.push(decode_tag(read_u32(reader)?)?);
    }

    let dependency_contract = InterfaceDependencyContractId::new(read_u32(reader)?);
    let current_run_cancellation = decode_tag(read_u32(reader)?)?;

    let may_unwind =
        super::common::decode_bool(reader, crate::InterfaceValidationField::Dependency)?;

    let count = read_count(reader, limits, InterfaceLimit::RecordCount)?;
    let mut receiver_cleanup = context.allocate_items(reader, count)?;

    for _ in 0..count {
        receiver_cleanup.push(crate::InterfaceTypeId::new(read_u32(reader)?));
    }

    let count = read_count(reader, limits, InterfaceLimit::RecordCount)?;
    let mut execution_properties = context.allocate_items(reader, count)?;

    for _ in 0..count {
        execution_properties.push(decode_tag(read_u32(reader)?)?);
    }

    let mut behavior = InterfaceCallablePhaseBehavior::new(
        effects,
        capabilities,
        trusted_capabilities,
        execution_requirements,
        lifecycle_obligations,
        dependency_contract,
        current_run_cancellation,
    )
    .with_may_unwind(may_unwind)
    .with_receiver_cleanup(receiver_cleanup);

    behavior.predicate_guarantees = predicate_sets
        .pop()
        .expect("two phase predicate sets")
        .into();

    behavior.predicate_requirements = predicate_sets
        .pop()
        .expect("two phase predicate sets")
        .into();

    // Retain wire ordering so validation can reject duplicate or unordered promises.
    behavior.execution_properties = execution_properties.into();

    Ok(behavior)
}

pub(super) fn decode_dependency_requirement(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
    depth: u64,
) -> Result<InterfaceDependencyRequirement, InterfaceValidationError> {
    limits.check(InterfaceLimit::SemanticTypeDepth, depth)?;

    let raw = read_u32(reader)?;

    match raw {
        10 => {
            let ty = crate::InterfaceTypeId::new(read_u32(reader)?);
            let requirements = decode_dependency_requirements(reader, limits, context, depth.saturating_add(1))?;

            Ok(InterfaceDependencyRequirement::cleanup_dependencies(ty, requirements))
        }
        7 => Ok(InterfaceDependencyRequirement::synchronous_cleanup(
            crate::InterfaceTypeId::new(read_u32(reader)?),
        )),
        8 => Ok(InterfaceDependencyRequirement::finalizer_body_cleanup(
            crate::InterfaceTypeId::new(read_u32(reader)?),
        )),
        9 => {
            let provider = read_symbol_reference(reader, context)?;
            let substitution = crate::InterfaceGenericSubstitutionId::new(read_u32(reader)?);

            let contextual_self = read_optional_u32(reader)?
                .map(|subject| {
                    read_symbol_reference(reader, context)
                        .map(|context| (context, crate::InterfaceTypeId::new(subject)))
                })
                .transpose()?;

            Ok(InterfaceDependencyRequirement::default_cleanup(
                provider,
                substitution,
                contextual_self,
            ))
        }
        1 => Ok(InterfaceDependencyRequirement::new(
            decode_dependency_subject(reader, limits, context)?,
            decode_dependency_requirement_kind(reader)?,
        )),
        2 => {
            let guard = decode_dependency_guard(reader, limits, context)?;

            let requirements =
                decode_dependency_requirements(reader, limits, context, depth.saturating_add(1))?;

            Ok(InterfaceDependencyRequirement::guarded(guard, requirements))
        }
        5 => Ok(InterfaceDependencyRequirement::variable(
            read_u32(reader)?,
            SymbolOrdinal::new(read_u32(reader)?),
        )),
        6 => {
            let count = read_count(reader, limits, InterfaceLimit::RecordCount)?;
            let mut definitions = context.allocate_items(reader, count)?;

            for _ in 0..count {
                definitions.push(decode_dependency_requirements(
                    reader,
                    limits,
                    context,
                    depth.saturating_add(1),
                )?);
            }

            let result =
                decode_dependency_requirements(reader, limits, context, depth.saturating_add(1))?;

            Ok(InterfaceDependencyRequirement::fixed_point(
                definitions,
                result,
            ))
        }
        3 | 4 => {
            let output = decode_tag(read_u32(reader)?)?;
            let callable = crate::InterfaceCallableInstanceId::new(read_u32(reader)?);

            let requirement = if raw == 3 {
                Some((
                    crate::InterfaceTypeId::new(read_u32(reader)?),
                    crate::InterfaceTraitApplicationId::new(read_u32(reader)?),
                ))
            } else {
                None
            };

            let inputs = decode_dependency_call_inputs(reader, limits, context, depth)?;

            Ok(InterfaceDependencyRequirement::result_call(
                callable,
                requirement,
                inputs,
                output,
            ))
        }
        _ => Err(crate::semantic::codec::invalid_discriminant(
            crate::InterfaceValidationField::Dependency,
            raw,
        )),
    }
}

fn decode_dependency_call_inputs(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
    depth: u64,
) -> Result<Vec<crate::InterfaceDependencyCallInput>, InterfaceValidationError> {
    let count = read_count(reader, limits, InterfaceLimit::RecordCount)?;
    let mut inputs = context.allocate_items(reader, count)?;

    for _ in 0..count {
        let input = decode_dependency_subject(reader, limits, context)?;

        if !input.projections.is_empty() {
            return Err(crate::semantic::codec::invalid_value(
                crate::InterfaceValidationField::Dependency,
            ));
        }

        let values =
            decode_dependency_requirements(reader, limits, context, depth.saturating_add(1))?;

        let storage =
            decode_dependency_requirements(reader, limits, context, depth.saturating_add(1))?;

        inputs.push(crate::InterfaceDependencyCallInput::new(
            input.root, values, storage,
        ));
    }

    Ok(inputs)
}

fn decode_dependency_requirements(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
    depth: u64,
) -> Result<Vec<InterfaceDependencyRequirement>, InterfaceValidationError> {
    let count = read_count(reader, limits, InterfaceLimit::RecordCount)?;
    let mut requirements = context.allocate_items(reader, count)?;

    for _ in 0..count {
        requirements.push(decode_dependency_requirement(
            reader, limits, context, depth,
        )?);
    }

    Ok(requirements)
}

pub(super) fn decode_dependency_subject(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
) -> Result<InterfaceDependencySubject, InterfaceValidationError> {
    let root_raw = read_u32(reader)?;

    let root = match root_raw {
        1 => InterfaceDependencySubjectRoot::Receiver,
        2 => InterfaceDependencySubjectRoot::Parameter(SymbolOrdinal::new(read_u32(reader)?)),
        3 => InterfaceDependencySubjectRoot::Result,
        8 => InterfaceDependencySubjectRoot::EvaluationStorage,
        4 => {
            InterfaceDependencySubjectRoot::ScopedCapability(SymbolOrdinal::new(read_u32(reader)?))
        }
        5 => InterfaceDependencySubjectRoot::ImplementationWitness(
            InterfaceImplementationInstanceId::new(read_u32(reader)?),
        ),
        9 => InterfaceDependencySubjectRoot::ImplementationRequirement(
            crate::InterfaceTypeId::new(read_u32(reader)?),
            crate::InterfaceTraitApplicationId::new(read_u32(reader)?),
        ),
        6 => InterfaceDependencySubjectRoot::ProductStatic(read_symbol_reference(reader, context)?),
        7 => InterfaceDependencySubjectRoot::ExactThreadStatic(read_symbol_reference(
            reader, context,
        )?),
        _ => {
            return Err(crate::semantic::codec::invalid_discriminant(
                crate::InterfaceValidationField::Dependency,
                root_raw,
            ));
        }
    };

    let projections = decode_dependency_projections(reader, limits, context)?;

    Ok(InterfaceDependencySubject::new(root, projections))
}

fn decode_dependency_projections(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
) -> Result<Vec<InterfaceDependencyProjection>, InterfaceValidationError> {
    let count = read_count(reader, limits, InterfaceLimit::RecordCount)?;

    let mut projections = context.allocate_items(reader, count)?;

    for _ in 0..count {
        let raw = read_u32(reader)?;

        projections.push(match raw {
            1 => {
                InterfaceDependencyProjection::ProductField(read_symbol_reference(reader, context)?)
            }
            2 => InterfaceDependencyProjection::TupleElement(SymbolOrdinal::new(read_u32(reader)?)),
            3 => InterfaceDependencyProjection::Element(InterfaceConstantTermId::new(read_u32(
                reader,
            )?)),
            4 => InterfaceDependencyProjection::NullableValue,
            5 => InterfaceDependencyProjection::UnionPayloadField(read_symbol_reference(
                reader, context,
            )?),
            6 => InterfaceDependencyProjection::OwnedTarget,
            7 => InterfaceDependencyProjection::UninitPayload,
            _ => {
                return Err(crate::semantic::codec::invalid_discriminant(
                    crate::InterfaceValidationField::Dependency,
                    raw,
                ));
            }
        });
    }

    Ok(projections)
}

pub(super) fn decode_dependency_guard(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
) -> Result<InterfaceDependencyGuard, InterfaceValidationError> {
    let raw = read_u32(reader)?;

    match raw {
        1 => Ok(InterfaceDependencyGuard::NullablePresent(
            decode_dependency_subject(reader, limits, context)?,
        )),
        2 => Ok(InterfaceDependencyGuard::ActiveUnionVariant {
            subject: decode_dependency_subject(reader, limits, context)?,
            variant: read_symbol_reference(reader, context)?,
        }),
        _ => Err(crate::semantic::codec::invalid_discriminant(
            crate::InterfaceValidationField::Dependency,
            raw,
        )),
    }
}

pub(super) fn decode_dependency_requirement_kind(
    reader: &mut WireReader<'_>,
) -> Result<InterfaceDependencyRequirementKind, InterfaceValidationError> {
    let raw = read_u32(reader)?;

    match raw {
        1 => Ok(InterfaceDependencyRequirementKind::StorageAlive),
        7 => Ok(InterfaceDependencyRequirementKind::ValueDependencies),
        2 => Ok(InterfaceDependencyRequirementKind::StorageInitialized),
        3 => Ok(InterfaceDependencyRequirementKind::ExclusiveMutationAuthority),
        4 => Ok(InterfaceDependencyRequirementKind::BorrowCapabilityActive(
            decode_tag(read_u32(reader)?)?,
        )),
        5 => Ok(InterfaceDependencyRequirementKind::ScopedCapabilityLive),
        6 => Ok(InterfaceDependencyRequirementKind::LifecycleObligation(
            decode_tag(read_u32(reader)?)?,
        )),
        _ => Err(crate::semantic::codec::invalid_discriminant(
            crate::InterfaceValidationField::Dependency,
            raw,
        )),
    }
}

fn decode_predicate_summary(
    reader: &mut WireReader<'_>,
) -> Result<InterfacePredicateSummary, InterfaceValidationError> {
    let dependency = InterfaceDependencyContractId::new(read_u32(reader)?);
    let condition = read_optional_u32(reader)?.map(InterfaceConstantTermId::new);

    let domain = match read_u32(reader)? {
        0 => bray_symbols::PredicateDomain::Value,
        1 => bray_symbols::PredicateDomain::Trusted,
        2 => bray_symbols::PredicateDomain::Execution,
        raw => {
            return Err(crate::semantic::codec::invalid_discriminant(
                crate::InterfaceValidationField::Reference,
                raw,
            ));
        }
    };

    Ok(InterfacePredicateSummary::new(dependency).with_condition(condition, domain))
}

fn decode_mutation(
    reader: &mut WireReader<'_>,
) -> Result<bray_symbols::DependencyMutationKind<crate::InterfaceTypeId>, InterfaceValidationError>
{
    use bray_symbols::DependencyMutationKind;

    Ok(match read_u32(reader)? {
        1 => DependencyMutationKind::Write,
        2 => DependencyMutationKind::Consume,
        3 => DependencyMutationKind::Retire,
        4 => DependencyMutationKind::Atomic,
        5 => DependencyMutationKind::Replace(crate::InterfaceTypeId::new(read_u32(reader)?)),
        6 => DependencyMutationKind::Clear,
        7 => DependencyMutationKind::Escape,
        raw => {
            return Err(crate::semantic::codec::invalid_discriminant(
                crate::InterfaceValidationField::Dependency,
                raw,
            ));
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{SemanticDecodeContext, WireReader, decode_dependency_contract, map_wire_error};
    use crate::{
        InterfaceDependencyContract, InterfaceDependencyRequirement,
        InterfaceDependencyRequirementKind, InterfaceDependencySubject,
        InterfaceDependencySubjectRoot, InterfaceLimit, InterfaceValidationError,
        InterfaceValidationLimits,
    };
    use bray_symbols::SymbolOrdinal;

    #[test]
    fn independent_reference_origins_cannot_claim_exact_input_identity() {
        use crate::semantic::codec::common::{
            REFERENCE_EXACT, REFERENCE_INPUT_ABSENT, REFERENCE_KIND_SHIFT,
        };

        let attributes = (2 << REFERENCE_KIND_SHIFT) | REFERENCE_INPUT_ABSENT;
        let mut words = vec![0, 1, 0, 0, 0, 0, 1, attributes, 3, 0, 0, 0, 0, 0];
        let limits = InterfaceValidationLimits::default();
        let contract = decode_words(&words, limits).expect("fixture encoded words must decode");

        assert_eq!(contract.borrows.len(), 1);
        assert!(contract.borrows[0].entry().is_none());
        assert!(!contract.borrows[0].is_exact());
        words[7] |= REFERENCE_EXACT;
        assert!(decode_words(&words, limits).is_err());

        // A raw-address origin bound cannot supply an ordinary borrow lifetime.
        words[7] = (1 << REFERENCE_KIND_SHIFT) | REFERENCE_INPUT_ABSENT;
        assert!(decode_words(&words, limits).is_err());
    }

    #[test]
    fn predicate_checking_domains_decode_and_reject_unknown_tags() {
        for (tag, domain) in [
            (0u32, bray_symbols::PredicateDomain::Value),
            (1, bray_symbols::PredicateDomain::Trusted),
            (2, bray_symbols::PredicateDomain::Execution),
        ] {
            let predicate =
                crate::InterfacePredicateSummary::new(crate::InterfaceDependencyContractId::new(3))
                    .with_condition(Some(crate::InterfaceConstantTermId::new(7)), domain);

            let mut bytes = [3, 1, 7, tag]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>();

            let mut reader = WireReader::new(&bytes);

            assert_eq!(
                super::decode_predicate_summary(&mut reader).expect("fixture `super::decode_predicate_summary` call must succeed"),
                predicate
            );

            reader.finish().expect("fixture operation must finish successfully");

            let end = bytes.len();

            bytes[end - 4..].copy_from_slice(&3u32.to_le_bytes());
            assert!(super::decode_predicate_summary(&mut WireReader::new(&bytes)).is_err());
        }
    }

    fn decode_words(
        words: &[u32],
        limits: InterfaceValidationLimits,
    ) -> Result<InterfaceDependencyContract, InterfaceValidationError> {
        let bytes = words
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .collect::<Vec<_>>();

        let mut reader = WireReader::new(&bytes);
        let mut context = SemanticDecodeContext::new(limits);
        let contract = decode_dependency_contract(&mut reader, limits, &mut context)?;

        reader.finish().map_err(map_wire_error)?;

        Ok(contract)
    }

    #[test]
    fn synchronous_cleanup_types_decode_with_bounded_counts() {
        let limits = InterfaceValidationLimits::default();
        let decoded = decode_words(&[1, 7, 3, 1, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 5], limits).expect("fixture encoded words must decode");

        assert_eq!(
            decoded,
            InterfaceDependencyContract::new([]).with_synchronous_cleanup([
                crate::InterfaceDependencyRequirement::synchronous_cleanup(
                    crate::InterfaceTypeId::new(3)
                )
            ])
        );

        assert!(matches!(
            decode_words(&[u32::MAX], limits),
            Err(InterfaceValidationError::ResourceLimitExceeded {
                limit: InterfaceLimit::RecordCount,
                ..
            })
        ));
    }

    #[test]
    fn report_outputs_preserve_presence_and_separate_requirements() {
        let decoded = decode_words(
            &[0, 1, 0, 0, 1, 0, 1, 1, 1, 2, 3, 0, 7, 0, 0, 0, 0, 2, 0, 5],
            InterfaceValidationLimits::default(),
        )
        .expect("fixture encoded words must decode");

        assert_eq!(
            decoded,
            InterfaceDependencyContract::new([])
                .with_completion_reports([])
                .with_abnormal_reports([InterfaceDependencyRequirement::new(
                    InterfaceDependencySubject::new(
                        InterfaceDependencySubjectRoot::Parameter(SymbolOrdinal::new(3)),
                        []
                    ),
                    InterfaceDependencyRequirementKind::ValueDependencies,
                )])
        );
    }

    #[test]
    fn witness_dependencies_decode_separate_value_and_storage_inputs() {
        let words = [
            0, // No synchronous cleanup obligations.
            1, // Normal completion is possible.
            0, // No normal-return preservation proof.
            1, // One result requirement.
            3, 1, 2, 3, 4, 1, // Value-output witness call and one input.
            1, 0, // Receiver input with no projection.
            1, 1, 2, 5, 0, 7, // Values carried by parameter five.
            1, 1, 8, 0, 1, // Evaluation-owned storage.
            0, 0, // No normal or abnormal report output.
            0, 0, 0, 0, 2, 0,
            5, // No reference relations, returned input, result observation or carriers. Unknown effects and input escapes.
        ];

        let decoded = decode_words(&words, InterfaceValidationLimits::default()).expect("fixture encoded words must decode");

        let expected =
            InterfaceDependencyContract::new([InterfaceDependencyRequirement::result_call(
                crate::InterfaceCallableInstanceId::new(2),
                Some((
                    crate::InterfaceTypeId::new(3),
                    crate::InterfaceTraitApplicationId::new(4),
                )),
                [crate::InterfaceDependencyCallInput::new(
                    InterfaceDependencySubjectRoot::Receiver,
                    [InterfaceDependencyRequirement::new(
                        InterfaceDependencySubject::new(
                            InterfaceDependencySubjectRoot::Parameter(SymbolOrdinal::new(5)),
                            [],
                        ),
                        InterfaceDependencyRequirementKind::ValueDependencies,
                    )],
                    [InterfaceDependencyRequirement::new(
                        InterfaceDependencySubject::new(
                            InterfaceDependencySubjectRoot::EvaluationStorage,
                            [],
                        ),
                        InterfaceDependencyRequirementKind::StorageAlive,
                    )],
                )],
                bray_symbols::DependencyCallOutput::Value,
            )]);

        assert_eq!(decoded, expected);

        assert!(matches!(
            decode_words(
                &words,
                InterfaceValidationLimits::default().with_semantic_type_depth(0)
            ),
            Err(InterfaceValidationError::ResourceLimitExceeded {
                limit: InterfaceLimit::SemanticTypeDepth,
                ..
            })
        ));
    }

    #[test]
    fn witness_dependencies_reject_projected_inputs_and_unbounded_counts() {
        assert_eq!(
            decode_words(
                &[0, 1, 0, 1, 3, 1, 2, 3, 4, 1, 1, 1, 4, 0, 0],
                InterfaceValidationLimits::default()
            ),
            Err(crate::semantic::codec::invalid_value(
                crate::InterfaceValidationField::Dependency
            )),
        );

        assert!(matches!(
            decode_words(
                &[0, 1, 0, 1, 3, 1, 2, 3, 4, u32::MAX],
                InterfaceValidationLimits::default()
            ),
            Err(InterfaceValidationError::ResourceLimitExceeded {
                limit: InterfaceLimit::RecordCount,
                ..
            }),
        ));
    }
    #[test]
    fn dependency_equations_decode_with_bounded_nesting() {
        let words = [
            0, 1, 0, 1, 6, 1, 1, 5, 0, 0, 1, 5, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 5,
        ];

        let variable = InterfaceDependencyRequirement::variable(0, SymbolOrdinal::new(0));

        let expected =
            InterfaceDependencyContract::new([InterfaceDependencyRequirement::fixed_point(
                [vec![variable.clone()]],
                [variable],
            )]);

        assert_eq!(
            decode_words(&words, InterfaceValidationLimits::default()),
            Ok(expected)
        );

        assert!(matches!(
            decode_words(
                &words,
                InterfaceValidationLimits::default().with_semantic_type_depth(0)
            ),
            Err(InterfaceValidationError::ResourceLimitExceeded {
                limit: InterfaceLimit::SemanticTypeDepth,
                ..
            })
        ));

        assert!(matches!(
            decode_words(
                &[0, 1, 0, 1, 6, u32::MAX],
                InterfaceValidationLimits::default()
            ),
            Err(InterfaceValidationError::ResourceLimitExceeded {
                limit: InterfaceLimit::RecordCount,
                ..
            })
        ));
    }
}
