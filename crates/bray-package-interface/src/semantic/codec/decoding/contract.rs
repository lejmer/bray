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
    let raw = read_u32(reader)?;

    match raw {
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
    );

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
    fn witness_dependencies_decode_separate_value_and_storage_inputs() {
        let words = [
            1, // One result requirement.
            3, 2, 3, 4, 1, // Witness call and one input.
            1, 0, // Receiver input with no projection.
            1, 1, 2, 5, 0, 7, // Values carried by parameter five.
            1, 1, 8, 0, 1, // Evaluation-owned storage.
        ];

        let decoded = decode_words(&words, InterfaceValidationLimits::default()).unwrap();

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
                &[1, 3, 2, 3, 4, 1, 1, 1, 4, 0, 0],
                InterfaceValidationLimits::default()
            ),
            Err(crate::semantic::codec::invalid_value(
                crate::InterfaceValidationField::Dependency
            )),
        );

        assert!(matches!(
            decode_words(
                &[1, 3, 2, 3, 4, u32::MAX],
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
        let words = [1, 6, 1, 1, 5, 0, 0, 1, 5, 0, 0];
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
            decode_words(&[1, 6, u32::MAX], InterfaceValidationLimits::default()),
            Err(InterfaceValidationError::ResourceLimitExceeded {
                limit: InterfaceLimit::RecordCount,
                ..
            })
        ));
    }
}
