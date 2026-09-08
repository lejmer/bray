use bray_symbols::CallableConditions;

use super::common::{decode_tag, validate_record_count};
use crate::semantic::codec::common::{
    SemanticDecodeContext, map_wire_error, read_count, read_optional_u32, read_symbol_reference,
    read_symbol_references, read_u32,
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

#[cfg(test)]
mod tests {
    use super::decode_contract_evidence;
    use crate::InterfaceValidationLimits;
    use crate::semantic::codec::common::{SemanticDecodeContext, write_count};
    use crate::wire::{WireEncoder, WireReader};

    fn evidence_bytes(promises: &[u32], targets: &[u32]) -> Vec<u8> {
        let mut encoder = WireEncoder::new();
        write_count(&mut encoder, promises.len());

        for ordinal in promises {
            encoder.write_u32(0); // Checked body evidence.
            encoder.write_u32(1); // Postcondition obligation.
            encoder.write_u32(*ordinal);
            write_count(&mut encoder, targets.len());

            for target in targets {
                encoder.write_u32(1); // Local symbol reference.
                encoder.write_u32(*target);
                encoder.write_u32(1); // Postcondition obligation.
                encoder.write_u32(0);
            }
        }

        encoder.into_bytes()
    }

    #[test]
    fn proof_decoding_rejects_duplicates_reordering_and_every_truncation() {
        let limits = InterfaceValidationLimits::default();
        let valid = evidence_bytes(&[1, 2], &[1, 2]);

        assert!(
            decode_contract_evidence(
                &mut WireReader::new(&valid),
                limits,
                &mut SemanticDecodeContext::new(limits)
            )
            .is_ok()
        );

        for length in 0..valid.len() {
            assert!(
                decode_contract_evidence(
                    &mut WireReader::new(&valid[..length]),
                    limits,
                    &mut SemanticDecodeContext::new(limits)
                )
                .is_err(),
                "length {length}"
            );
        }

        for invalid in [
            evidence_bytes(&[1, 1], &[1]),
            evidence_bytes(&[2, 1], &[1]),
            evidence_bytes(&[1], &[1, 1]),
            evidence_bytes(&[1], &[2, 1]),
        ] {
            assert!(
                decode_contract_evidence(
                    &mut WireReader::new(&invalid),
                    limits,
                    &mut SemanticDecodeContext::new(limits)
                )
                .is_err()
            );
        }

        for origin in [1_u32, u32::MAX] {
            let mut invalid = evidence_bytes(&[1], &[1]);
            invalid[4..8].copy_from_slice(&origin.to_le_bytes());

            assert!(
                decode_contract_evidence(
                    &mut WireReader::new(&invalid),
                    limits,
                    &mut SemanticDecodeContext::new(limits)
                )
                .is_err()
            );
        }

        let mut foreign = evidence_bytes(&[1], &[]);
        foreign[4..8].copy_from_slice(&1_u32.to_le_bytes());

        let decoded = decode_contract_evidence(
            &mut WireReader::new(&foreign),
            limits,
            &mut SemanticDecodeContext::new(limits),
        )
        .unwrap();

        assert_eq!(
            decoded[0].origin(),
            bray_symbols::CallableEvidenceOrigin::ForeignAssertion
        );
    }
}

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
    let requirement_count = read_count(reader, limits, InterfaceLimit::RecordCount)?;
    let mut requirements = context.allocate_items(reader, requirement_count)?;

    for _ in 0..requirement_count {
        requirements.push(decode_dependency_requirement(reader, limits, context, 0)?);
    }

    Ok(InterfaceDependencyContract::new(requirements))
}

fn decode_predicate(
    reader: &mut WireReader<'_>,
) -> Result<InterfacePredicateSummary, InterfaceValidationError> {
    let dependency = InterfaceDependencyContractId::new(read_u32(reader)?);
    let condition = read_optional_u32(reader)?.map(crate::InterfaceConstantTermId::new);

    Ok(InterfacePredicateSummary::new(dependency).with_condition(condition))
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
            decode_predicate(reader)?,
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

    let conditions = decode_callable_conditions(reader, limits, context)?;
    let evidence = decode_contract_evidence(reader, limits, context)?;

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

    Ok(
        InterfaceCallableContract::new(owner, [], invocation_behavior, deferred_execution_behavior)
            .with_conditions(conditions)
            .with_evidence(evidence),
    )
}

fn decode_contract_evidence(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
) -> Result<
    Vec<bray_symbols::CallableContractEvidence<crate::InterfaceSymbolReference>>,
    InterfaceValidationError,
> {
    let count = read_count(reader, limits, InterfaceLimit::RecordCount)?;
    let mut proofs = context.allocate_items(reader, count)?;

    for _ in 0..count {
        let origin = match read_u32(reader)? {
            0 => bray_symbols::CallableEvidenceOrigin::CheckedBody,
            1 => bray_symbols::CallableEvidenceOrigin::ForeignAssertion,
            value => {
                return Err(crate::semantic::codec::invalid_discriminant(
                    crate::InterfaceValidationField::Dependency,
                    value,
                ));
            }
        };

        let obligation = decode_contract_obligation(reader)?;
        let count = read_count(reader, limits, InterfaceLimit::RecordCount)?;
        let mut dependencies = context.allocate_items(reader, count)?;

        for _ in 0..count {
            dependencies.push((
                read_symbol_reference(reader, context)?,
                decode_contract_obligation(reader)?,
            ));
        }

        if !crate::validation::is_strictly_sorted(&dependencies) {
            return Err(crate::semantic::codec::invalid_value(
                crate::InterfaceValidationField::Reference,
            ));
        }

        proofs.push(match origin {
            bray_symbols::CallableEvidenceOrigin::CheckedBody => {
                bray_symbols::CallableContractEvidence::new(obligation, dependencies)
            }
            bray_symbols::CallableEvidenceOrigin::ForeignAssertion if dependencies.is_empty() => {
                bray_symbols::CallableContractEvidence::foreign_assertion(obligation)
            }
            bray_symbols::CallableEvidenceOrigin::ForeignAssertion => {
                return Err(crate::semantic::codec::invalid_value(
                    crate::InterfaceValidationField::Reference,
                ));
            }
        });
    }

    if !proofs
        .windows(2)
        .all(|pair| pair[0].obligation() < pair[1].obligation())
    {
        return Err(crate::semantic::codec::invalid_value(
            crate::InterfaceValidationField::Reference,
        ));
    }

    Ok(proofs)
}

fn decode_contract_obligation(
    reader: &mut WireReader<'_>,
) -> Result<bray_symbols::CallableContractObligation, InterfaceValidationError> {
    match read_u32(reader)? {
        0 => Ok(bray_symbols::CallableContractObligation::Execution(
            bray_symbols::CallableExecutionGuarantee::new(
                decode_tag(read_u32(reader)?)?,
                read_optional_u32(reader)?.map(SymbolOrdinal::new),
            ),
        )),
        1 => Ok(bray_symbols::CallableContractObligation::Postcondition(
            SymbolOrdinal::new(read_u32(reader)?),
        )),
        value => Err(crate::semantic::codec::invalid_discriminant(
            crate::InterfaceValidationField::Dependency,
            value,
        )),
    }
}

pub(super) fn decode_callable_conditions(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
) -> Result<
    bray_symbols::CallableConditionSet<InterfaceCallableContractClause>,
    InterfaceValidationError,
> {
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

    let postconditions =
        decode_callable_clauses(reader, limits, context, CallableContractClauseKind::Ensures)?;

    if postconditions.iter().any(|clause| clause.guard().is_some()) {
        return Err(crate::semantic::codec::invalid_value(
            crate::InterfaceValidationField::Reference,
        ));
    }

    decoded_clauses.extend(postconditions);

    decoded_clauses.extend(decode_callable_clauses(
        reader,
        limits,
        context,
        CallableContractClauseKind::Guard,
    )?);

    let guarded_postconditions =
        decode_callable_clauses(reader, limits, context, CallableContractClauseKind::Ensures)?;

    if guarded_postconditions
        .iter()
        .any(|clause| clause.guard().is_none())
    {
        return Err(crate::semantic::codec::invalid_value(
            crate::InterfaceValidationField::Reference,
        ));
    }

    decoded_clauses.extend(guarded_postconditions);

    let guarantee_count = read_count(reader, limits, InterfaceLimit::RecordCount)?;
    let mut guarantees = context.allocate_items(reader, guarantee_count)?;

    for _ in 0..guarantee_count {
        let property = decode_tag(read_u32(reader)?)?;
        let guard = read_optional_u32(reader)?.map(SymbolOrdinal::new);

        guarantees.push(bray_symbols::CallableExecutionGuarantee::new(
            property, guard,
        ));
    }

    if !guarantees.windows(2).all(|pair| pair[0] < pair[1]) {
        return Err(crate::semantic::codec::invalid_value(
            crate::InterfaceValidationField::Reference,
        ));
    }

    Ok(bray_symbols::CallableConditionSet::new(decoded_clauses)
        .with_execution_guarantees(guarantees))
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
        let guard = read_optional_u32(reader)?.map(SymbolOrdinal::new);
        let raw = read_u32(reader)?;

        let clause = match raw {
            1 => InterfaceCallableContractClause::new(ordinal, kind, decode_predicate(reader)?),
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

        clauses.push(clause.with_guard(guard));
    }

    Ok(clauses)
}

pub(super) fn decode_callable_behavior(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
) -> Result<InterfaceCallablePhaseBehavior, InterfaceValidationError> {
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

    Ok(InterfaceCallablePhaseBehavior::new(
        effects,
        capabilities,
        trusted_capabilities,
        execution_requirements,
        lifecycle_obligations,
        dependency_contract,
        current_run_cancellation,
    ))
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
        1 => Ok(InterfaceDependencyRequirement::new(
            decode_dependency_subject(reader, limits, context)?,
            decode_dependency_requirement_kind(reader)?,
        )),
        2 => {
            let guard = decode_dependency_guard(reader, limits, context)?;
            let count = read_count(reader, limits, InterfaceLimit::RecordCount)?;

            let mut requirements = context.allocate_items(reader, count)?;

            for _ in 0..count {
                requirements.push(decode_dependency_requirement(
                    reader,
                    limits,
                    context,
                    depth.saturating_add(1),
                )?);
            }

            Ok(InterfaceDependencyRequirement::guarded(guard, requirements))
        }
        _ => Err(crate::semantic::codec::invalid_discriminant(
            crate::InterfaceValidationField::Dependency,
            raw,
        )),
    }
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
        4 => {
            InterfaceDependencySubjectRoot::ScopedCapability(SymbolOrdinal::new(read_u32(reader)?))
        }
        5 => InterfaceDependencySubjectRoot::ImplementationWitness(
            InterfaceImplementationInstanceId::new(read_u32(reader)?),
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
            _ => {
                return Err(crate::semantic::codec::invalid_discriminant(
                    crate::InterfaceValidationField::Dependency,
                    raw,
                ));
            }
        });
    }

    Ok(InterfaceDependencySubject::new(root, projections))
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
