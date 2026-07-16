use super::common::{decode_tag, validate_record_count};
use crate::semantic::codec::common::{
    SemanticDecodeContext, map_wire_error, read_count, read_symbol_reference,
    read_symbol_references, read_u32,
};
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

pub(super) fn decode_contracts(
    section: ValidatedInterfaceSection<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
    facts: &mut InterfaceSemanticFacts,
) -> Result<(), InterfaceValidationError> {
    let mut reader = WireReader::new(section.bytes());

    let dependency_count = read_count(&mut reader, limits, InterfaceLimit::RecordCount)?;
    let constraint_count = read_count(&mut reader, limits, InterfaceLimit::RecordCount)?;
    let callable_count = read_count(&mut reader, limits, InterfaceLimit::RecordCount)?;

    validate_record_count(
        section,
        [dependency_count, constraint_count, callable_count],
    )?;

    let mut dependencies = context.allocate_items(&reader, dependency_count)?;

    for _ in 0..dependency_count {
        let requirement_count = read_count(&mut reader, limits, InterfaceLimit::RecordCount)?;
        let mut requirements = context.allocate_items(&reader, requirement_count)?;

        for _ in 0..requirement_count {
            requirements.push(decode_dependency_requirement(
                &mut reader,
                limits,
                context,
                0,
            )?);
        }

        dependencies.push(InterfaceDependencyContract::new(requirements));
    }

    let mut constraints = context.allocate_items(&reader, constraint_count)?;

    for _ in 0..constraint_count {
        constraints.push(InterfaceConstraint::new(
            read_symbol_reference(&mut reader, context)?,
            SymbolOrdinal::new(read_u32(&mut reader)?),
            InterfacePredicateSummary::new(InterfaceDependencyContractId::new(read_u32(
                &mut reader,
            )?)),
        ));
    }

    let mut callable_contracts = context.allocate_items(&reader, callable_count)?;

    for _ in 0..callable_count {
        let owner = read_symbol_reference(&mut reader, context)?;
        let mut decoded_clauses = decode_callable_clauses(
            &mut reader,
            limits,
            context,
            CallableContractClauseKind::Requires,
        )?;
        decoded_clauses.extend(decode_callable_clauses(
            &mut reader,
            limits,
            context,
            CallableContractClauseKind::Static,
        )?);
        decoded_clauses.extend(decode_callable_clauses(
            &mut reader,
            limits,
            context,
            CallableContractClauseKind::Ensures,
        )?);

        let invocation_behavior = decode_callable_behavior(&mut reader, limits, context)?;
        let deferred_execution_behavior = match read_u32(&mut reader)? {
            0 => None,
            1 => Some(decode_callable_behavior(&mut reader, limits, context)?),
            _ => return Err(InterfaceValidationError::Malformed),
        };

        callable_contracts.push(InterfaceCallableContract::new(
            owner,
            decoded_clauses,
            invocation_behavior,
            deferred_execution_behavior,
        ));
    }

    reader.finish().map_err(map_wire_error)?;

    facts.dependency_contracts = dependencies.into();
    facts.constraints = constraints.into();
    facts.callable_contracts = callable_contracts.into();

    Ok(())
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
        clauses.push(InterfaceCallableContractClause::new(
            SymbolOrdinal::new(read_u32(reader)?),
            kind,
            InterfacePredicateSummary::new(InterfaceDependencyContractId::new(read_u32(reader)?)),
        ));
    }

    Ok(clauses)
}

fn decode_callable_behavior(
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
