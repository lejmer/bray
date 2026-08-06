use super::common::{decode_tag, validate_record_count};
use crate::semantic::codec::common::{
    SemanticDecodeContext, map_wire_error, read_count, read_symbol_reference,
    read_symbol_references, read_u32,
};
use crate::semantic::codec::record::RecordTable;
use crate::semantic::model::{
    InterfaceCallableContract, InterfaceCallableContractClause, InterfaceCallablePhaseBehavior,
    InterfaceConstantTermId, InterfaceConstraint, InterfaceDependencyContract,
    InterfaceDependencyContractId, InterfaceDependencyGuard, InterfaceDependencyProjection,
    InterfaceDependencyRequirement, InterfaceDependencyRequirementKind, InterfaceDependencySubject,
    InterfaceDependencySubjectRoot, InterfaceImplementationInstanceId, InterfacePredicateSummary,
    InterfaceSemanticFacts, InterfaceTrustedCapabilityRequirement,
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

    let dependencies = RecordTable::read_from(&mut reader, context)?;
    let constraints = RecordTable::read_from(&mut reader, context)?;
    let callables = RecordTable::read_from(&mut reader, context)?;

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
    facts: &mut InterfaceSemanticFacts,
) -> Result<(), InterfaceValidationError> {
    let tables = decode_contract_tables(section, context)?;

    let dependencies = tables.dependencies.decode_all(context, |reader, context| {
        decode_dependency_contract(reader, limits, context)
    })?;

    let constraints = tables.constraints.decode_all(context, decode_constraint)?;

    let callables = tables.callables.decode_all(context, |reader, context| {
        decode_callable_contract(reader, limits, context)
    })?;

    facts.dependency_contracts = dependencies.into();
    facts.constraints = constraints.into();
    facts.callable_contracts = callables.into();

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

pub(super) fn decode_constraint(
    reader: &mut WireReader<'_>,
    context: &mut SemanticDecodeContext,
) -> Result<InterfaceConstraint, InterfaceValidationError> {
    let owner = read_symbol_reference(reader, context)?;
    let ordinal = SymbolOrdinal::new(read_u32(reader)?);

    match read_u32(reader)? {
        1 => Ok(InterfaceConstraint::new(
            owner,
            ordinal,
            InterfacePredicateSummary::new(InterfaceDependencyContractId::new(read_u32(reader)?)),
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
        _ => Err(InterfaceValidationError::Malformed),
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

    let invocation_behavior = decode_callable_behavior(reader, limits, context)?;

    let deferred_execution_behavior = match read_u32(reader)? {
        0 => None,
        1 => Some(decode_callable_behavior(reader, limits, context)?),
        _ => return Err(InterfaceValidationError::Malformed),
    };

    Ok(InterfaceCallableContract::new(
        owner,
        decoded_clauses,
        invocation_behavior,
        deferred_execution_behavior,
    ))
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

        let clause = match read_u32(reader)? {
            1 => InterfaceCallableContractClause::new(
                ordinal,
                kind,
                InterfacePredicateSummary::new(InterfaceDependencyContractId::new(read_u32(
                    reader,
                )?)),
            ),
            2 if kind == CallableContractClauseKind::Static => {
                InterfaceCallableContractClause::trait_satisfaction(
                    ordinal,
                    crate::InterfaceTypeId::new(read_u32(reader)?),
                    crate::InterfaceTraitApplicationId::new(read_u32(reader)?),
                )
            }
            _ => return Err(InterfaceValidationError::Malformed),
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

    match read_u32(reader)? {
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
        _ => Err(InterfaceValidationError::Malformed),
    }
}

pub(super) fn decode_dependency_subject(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
) -> Result<InterfaceDependencySubject, InterfaceValidationError> {
    let root = match read_u32(reader)? {
        1 => InterfaceDependencySubjectRoot::Receiver,
        2 => InterfaceDependencySubjectRoot::Parameter(SymbolOrdinal::new(read_u32(reader)?)),
        3 => InterfaceDependencySubjectRoot::Result,
        4 => {
            InterfaceDependencySubjectRoot::ScopedCapability(SymbolOrdinal::new(read_u32(reader)?))
        }
        5 => InterfaceDependencySubjectRoot::ImplementationWitness(
            InterfaceImplementationInstanceId::new(read_u32(reader)?),
        ),
        _ => return Err(InterfaceValidationError::Malformed),
    };

    let count = read_count(reader, limits, InterfaceLimit::RecordCount)?;

    let mut projections = context.allocate_items(reader, count)?;

    for _ in 0..count {
        projections.push(match read_u32(reader)? {
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
            _ => return Err(InterfaceValidationError::Malformed),
        });
    }

    Ok(InterfaceDependencySubject::new(root, projections))
}

pub(super) fn decode_dependency_guard(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
) -> Result<InterfaceDependencyGuard, InterfaceValidationError> {
    match read_u32(reader)? {
        1 => Ok(InterfaceDependencyGuard::NullablePresent(
            decode_dependency_subject(reader, limits, context)?,
        )),
        2 => Ok(InterfaceDependencyGuard::ActiveUnionVariant {
            subject: decode_dependency_subject(reader, limits, context)?,
            variant: read_symbol_reference(reader, context)?,
        }),
        _ => Err(InterfaceValidationError::Malformed),
    }
}

pub(super) fn decode_dependency_requirement_kind(
    reader: &mut WireReader<'_>,
) -> Result<InterfaceDependencyRequirementKind, InterfaceValidationError> {
    match read_u32(reader)? {
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
        _ => Err(InterfaceValidationError::Malformed),
    }
}
