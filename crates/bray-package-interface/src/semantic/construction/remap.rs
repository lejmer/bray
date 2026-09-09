use bray_symbols::CallableConditions;

use std::sync::Arc;

use super::super::model::{
    InterfaceCallableContract, InterfaceCallablePhaseBehavior, InterfaceConstraint,
    InterfaceConstraintKind, InterfaceDeclarationTemplate, InterfacePredicateSummary,
    InterfaceSemanticRecordKind, InterfaceSemantics, InterfaceStorageMember, InterfaceStorageShape,
};
use super::model::{
    InterfaceSemanticCommitError, InterfaceSemanticIdRemap, InterfaceSemanticTableKind,
};
use super::template::{
    offset, offset_support_entity, remap_checked_template, remap_support_entity,
};
use crate::InterfaceSymbolReference;

pub(super) fn commit_fragment(
    mut fragment: InterfaceSemantics,
    remap: &InterfaceSemanticIdRemap,
    checked_template_count: usize,
    support_entity_count: usize,
) -> Result<InterfaceSemantics, InterfaceSemanticCommitError> {
    remap.validate(&fragment)?;

    if let Some(table) = unexpected_package_table(&fragment) {
        return Err(InterfaceSemanticCommitError::UnexpectedPackageRecord(table));
    }

    let checked_template_base = offset(
        checked_template_count,
        InterfaceSemanticTableKind::CheckedTemplate,
    )?;

    let support_entity_base = offset(
        support_entity_count,
        InterfaceSemanticTableKind::SupportEntity,
    )?;

    for constraint in Arc::make_mut(&mut fragment.constraints) {
        remap_constraint(constraint, remap)?;
    }

    for contract in Arc::make_mut(&mut fragment.callable_contracts) {
        remap_callable_contract(contract, remap)?;
    }

    for signature in Arc::make_mut(&mut fragment.callable_signatures) {
        signature.callable_type = remap.ty(signature.callable_type)?;
        signature.result = remap.ty(signature.result)?;

        if let Some(receiver) = &mut signature.receiver {
            receiver.ty = remap.ty(receiver.ty)?;
        }
    }

    for declared_type in Arc::make_mut(&mut fragment.declared_types) {
        declared_type.ty = remap.ty(declared_type.ty)?;
    }

    for representation in Arc::make_mut(&mut fragment.type_representations) {
        representation.union_tag_type = representation
            .union_tag_type
            .map(|ty| remap.ty(ty))
            .transpose()?;

        remap_storage(&mut representation.storage, remap)?;
    }

    fragment.checked_templates = fragment
        .checked_templates
        .iter()
        .map(|template| {
            remap_checked_template(
                template,
                remap,
                support_entity_base,
                fragment.support_entities.len(),
            )
        })
        .collect::<Result<Vec<_>, _>>()?
        .into();

    fragment.support_entities = fragment
        .support_entities
        .iter()
        .map(|entity| {
            remap_support_entity(
                entity,
                remap,
                checked_template_base,
                fragment.checked_templates.len(),
            )
        })
        .collect::<Result<Vec<_>, _>>()?
        .into();

    fragment.declaration_templates = fragment
        .declaration_templates
        .iter()
        .map(|template| {
            Ok(InterfaceDeclarationTemplate::new(
                template.owner().clone(),
                template.kind(),
                template.ordinal(),
                offset_support_entity(
                    template.entity(),
                    support_entity_base,
                    fragment.support_entities.len(),
                )?,
            ))
        })
        .collect::<Result<Vec<_>, InterfaceSemanticCommitError>>()?
        .into();

    Ok(fragment)
}

fn unexpected_package_table(fragment: &InterfaceSemantics) -> Option<InterfaceSemanticTableKind> {
    [
        (
            !fragment.implementations.is_empty(),
            InterfaceSemanticTableKind::Implementation,
        ),
        (
            !fragment.coherence.is_empty(),
            InterfaceSemanticTableKind::Coherence,
        ),
        (
            !fragment.target_dependencies.is_empty(),
            InterfaceSemanticTableKind::TargetDependency,
        ),
        (
            !fragment.abi_dependencies.is_empty(),
            InterfaceSemanticTableKind::AbiDependency,
        ),
        (
            !fragment.runtime_requirements.is_empty(),
            InterfaceSemanticTableKind::RuntimeRequirement,
        ),
        (
            !fragment.provenance.is_empty(),
            InterfaceSemanticTableKind::Provenance,
        ),
    ]
    .into_iter()
    .find_map(|(present, table)| present.then_some(table))
}

fn remap_constraint(
    constraint: &mut InterfaceConstraint,
    remap: &InterfaceSemanticIdRemap,
) -> Result<(), InterfaceSemanticCommitError> {
    match &mut constraint.kind {
        InterfaceConstraintKind::Predicate(predicate) => remap_predicate(predicate, remap),
        InterfaceConstraintKind::TraitSatisfaction {
            subject,
            application,
        } => {
            *subject = remap.ty(*subject)?;
            *application = remap.trait_application(*application)?;

            Ok(())
        }
        InterfaceConstraintKind::TypeEquality { left, right } => {
            *left = remap.ty(*left)?;
            *right = remap.ty(*right)?;

            Ok(())
        }
    }
}

fn remap_callable_contract(
    contract: &mut InterfaceCallableContract,
    remap: &InterfaceSemanticIdRemap,
) -> Result<(), InterfaceSemanticCommitError> {
    contract.try_map_clauses(|clause| {
        clause.try_map_ids(
            |id| remap.dependency_contract(id),
            |id| remap.constant_term(id),
            |id| remap.ty(id),
            |id| remap.trait_application(id),
        )
    })?;

    for proof in Arc::make_mut(&mut contract.evidence) {
        *proof = proof.try_map_targets(|target| {
            let callable = remap.callable_instance(target.callable())?;

            let dispatch = target
                .dispatch()
                .map(|(subject, application)| {
                    Ok::<_, InterfaceSemanticCommitError>((
                        remap.ty(subject)?,
                        remap.trait_application(application)?,
                    ))
                })
                .transpose()?;

            Ok::<_, InterfaceSemanticCommitError>(crate::InterfaceCallableEvidenceTarget::new(
                callable, dispatch,
            ))
        })?;
    }

    remap_callable_behavior(&mut contract.invocation_behavior, remap)?;

    if let Some(behavior) = &mut contract.deferred_execution_behavior {
        remap_callable_behavior(behavior, remap)?;
    }

    Ok(())
}

fn remap_callable_behavior(
    behavior: &mut InterfaceCallablePhaseBehavior,
    remap: &InterfaceSemanticIdRemap,
) -> Result<(), InterfaceSemanticCommitError> {
    behavior.dependency_contract = remap.dependency_contract(behavior.dependency_contract)?;

    Ok(())
}

fn remap_predicate(
    predicate: &mut InterfacePredicateSummary,
    remap: &InterfaceSemanticIdRemap,
) -> Result<(), InterfaceSemanticCommitError> {
    predicate.dependency_contract = remap.dependency_contract(predicate.dependency_contract)?;

    predicate.condition = predicate
        .condition
        .map(|term| remap.constant_term(term))
        .transpose()?;

    Ok(())
}

fn remap_storage(
    storage: &mut InterfaceStorageShape,
    remap: &InterfaceSemanticIdRemap,
) -> Result<(), InterfaceSemanticCommitError> {
    match storage {
        InterfaceStorageShape::Structure(members) => remap_storage_members(members, remap),
        InterfaceStorageShape::Union(variants) => {
            for variant in Arc::make_mut(variants) {
                remap_storage_members(&mut variant.members, remap)?;
            }

            Ok(())
        }
    }
}

fn remap_storage_members(
    members: &mut Arc<[InterfaceStorageMember]>,
    remap: &InterfaceSemanticIdRemap,
) -> Result<(), InterfaceSemanticCommitError> {
    for member in Arc::make_mut(members) {
        member.ty = remap.ty(member.ty)?;
    }

    Ok(())
}

pub(super) fn normalize_records<T>(
    records: &mut Arc<[T]>,
    kind: InterfaceSemanticRecordKind,
    same_key: impl Fn(&T, &T) -> bool,
    owner: impl Fn(&T) -> &InterfaceSymbolReference,
) -> Result<(), InterfaceSemanticCommitError>
where
    T: Clone + Ord,
{
    let mut normalized = records.iter().cloned().collect::<Vec<_>>();

    normalized.sort_unstable();

    for pair in normalized.windows(2) {
        if same_key(&pair[0], &pair[1]) && pair[0] != pair[1] {
            return Err(InterfaceSemanticCommitError::ConflictingRecord {
                owner: owner(&pair[1]).clone(),
                kind,
            });
        }
    }

    normalized.dedup();
    *records = normalized.into();

    Ok(())
}

pub(super) fn extend_records<T: Clone>(
    records: &mut Arc<[T]>,
    additions: impl IntoIterator<Item = T>,
) {
    let mut combined = Vec::with_capacity(records.len());

    combined.extend(records.iter().cloned());
    combined.extend(additions);
    *records = combined.into();
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::super::super::model::{
        InterfaceDeclaredType, InterfaceSemanticRecordKind, InterfaceSemantics,
        InterfaceSourceProvenance, InterfaceType, InterfaceTypeId,
    };
    use super::super::commit::commit_interface_semantic_fragments;
    use super::{
        InterfaceSemanticCommitError, InterfaceSemanticIdRemap, InterfaceSemanticTableKind,
    };
    use bray_symbols::{
        CallableContractEvidence, CallableContractObligation, CallableExecutionGuarantee,
        ExecutionProperty, InterfaceSymbolId, SymbolOrdinal,
    };

    use crate::{
        InterfaceCallableContract, InterfaceCallableEvidenceTarget, InterfaceCallableInstanceId,
        InterfaceDependencyContractId, InterfaceSymbolReference, InterfaceTraitApplicationId,
    };

    fn contract_with_proof_dependencies() -> InterfaceCallableContract {
        let obligation = CallableContractObligation::Postcondition(SymbolOrdinal::new(2));

        InterfaceCallableContract::new(
            InterfaceSymbolReference::Local(InterfaceSymbolId::new(0)),
            [],
            crate::test_support::callable_phase_behavior(),
            None,
        )
        .with_evidence([CallableContractEvidence::new(
            obligation,
            [
                (
                    InterfaceCallableEvidenceTarget::new(InterfaceCallableInstanceId::new(0), None),
                    obligation,
                ),
                (
                    InterfaceCallableEvidenceTarget::new(
                        InterfaceCallableInstanceId::new(1),
                        Some((InterfaceTypeId::new(0), InterfaceTraitApplicationId::new(0))),
                    ),
                    obligation,
                ),
            ],
        )])
    }

    #[test]
    fn proof_dependencies_receive_package_wide_callable_and_dispatch_identities() {
        let mut contract = contract_with_proof_dependencies();
        let obligation = contract.evidence()[0].obligation();

        let remap = InterfaceSemanticIdRemap::new()
            .with_applications(
                [],
                [InterfaceTraitApplicationId::new(9)],
                [
                    InterfaceCallableInstanceId::new(8),
                    InterfaceCallableInstanceId::new(4),
                ],
                [],
            )
            .with_values(
                [InterfaceDependencyContractId::new(0)],
                [InterfaceTypeId::new(7)],
                [],
                [],
            );

        super::remap_callable_contract(&mut contract, &remap).unwrap();

        assert_eq!(
            contract.evidence(),
            [CallableContractEvidence::new(
                obligation,
                [
                    (
                        InterfaceCallableEvidenceTarget::new(
                            InterfaceCallableInstanceId::new(8),
                            None
                        ),
                        obligation
                    ),
                    (
                        InterfaceCallableEvidenceTarget::new(
                            InterfaceCallableInstanceId::new(4),
                            Some((InterfaceTypeId::new(7), InterfaceTraitApplicationId::new(9))),
                        ),
                        obligation
                    ),
                ],
            )]
        );

        let foreign =
            CallableContractEvidence::foreign_assertion(CallableContractObligation::Execution(
                CallableExecutionGuarantee::new(ExecutionProperty::Pure, None),
            ));

        contract = contract.with_evidence([foreign.clone()]);
        super::remap_callable_contract(&mut contract, &remap).unwrap();

        assert_eq!(contract.evidence(), [foreign]);
    }

    #[test]
    fn missing_proof_dependency_mapping_preserves_its_exact_table() {
        for (callables, types, applications, table, reference) in [
            (
                false,
                false,
                false,
                InterfaceSemanticTableKind::CallableInstance,
                1,
            ),
            (true, false, false, InterfaceSemanticTableKind::Type, 0),
            (
                true,
                true,
                false,
                InterfaceSemanticTableKind::TraitApplication,
                0,
            ),
        ] {
            let mut contract = contract_with_proof_dependencies();

            let remap = InterfaceSemanticIdRemap::new()
                .with_applications(
                    [],
                    applications.then_some(InterfaceTraitApplicationId::new(9)),
                    std::iter::once(InterfaceCallableInstanceId::new(8))
                        .chain(callables.then_some(InterfaceCallableInstanceId::new(4))),
                    [],
                )
                .with_values(
                    [InterfaceDependencyContractId::new(0)],
                    types.then_some(InterfaceTypeId::new(7)),
                    [],
                    [],
                );

            assert_eq!(
                super::remap_callable_contract(&mut contract, &remap),
                Err(InterfaceSemanticCommitError::MissingReference { table, reference }),
            );
        }
    }

    #[test]
    fn shared_fragment_types_receive_one_package_identity() {
        let base =
            InterfaceSemantics::new().with_values([], [InterfaceType::Tuple(Arc::new([]))], [], []);

        let fragments = [1, 2].map(|owner| {
            let fragment = InterfaceSemantics::new()
                .with_values([], [InterfaceType::Tuple(Arc::new([]))], [], [])
                .with_declared_types([InterfaceDeclaredType::new(
                    InterfaceSymbolReference::Local(InterfaceSymbolId::new(owner)),
                    InterfaceTypeId::new(0),
                )]);

            let remap =
                InterfaceSemanticIdRemap::new().with_values([], [InterfaceTypeId::new(0)], [], []);

            (fragment, remap)
        });

        let semantics = commit_interface_semantic_fragments(base, fragments)
            .unwrap_or_else(|error| panic!("fragments must commit: {error:?}"));

        assert_eq!(semantics.types().len(), 1);
        assert_eq!(semantics.declared_types().len(), 2);

        assert!(
            semantics
                .declared_types()
                .iter()
                .all(|declaration| declaration.ty() == InterfaceTypeId::new(0))
        );
    }

    #[test]
    fn incomplete_fragment_reports_its_missing_table_reference() {
        let fragment =
            InterfaceSemantics::new().with_values([], [InterfaceType::Tuple(Arc::new([]))], [], []);

        let error = commit_interface_semantic_fragments(
            InterfaceSemantics::new(),
            [(fragment, InterfaceSemanticIdRemap::new())],
        )
        .expect_err("fragment without a type remap must be rejected");

        assert_eq!(
            error,
            InterfaceSemanticCommitError::MissingReference {
                table: InterfaceSemanticTableKind::Type,
                reference: 0,
            }
        );
    }

    #[test]
    fn conflicting_declaration_records_report_their_category() {
        let base = InterfaceSemantics::new().with_values(
            [],
            [
                InterfaceType::Tuple(Arc::new([])),
                InterfaceType::Tuple(Arc::new([InterfaceTypeId::new(0)])),
            ],
            [],
            [],
        );

        let owner = InterfaceSymbolReference::Local(InterfaceSymbolId::new(1));

        let fragments = [InterfaceTypeId::new(0), InterfaceTypeId::new(1)].map(|ty| {
            let fragment = InterfaceSemantics::new()
                .with_values([], [InterfaceType::Tuple(Arc::new([]))], [], [])
                .with_declared_types([InterfaceDeclaredType::new(
                    owner.clone(),
                    InterfaceTypeId::new(0),
                )]);

            let remap = InterfaceSemanticIdRemap::new().with_values([], [ty], [], []);

            (fragment, remap)
        });

        let error = commit_interface_semantic_fragments(base, fragments)
            .expect_err("conflicting declaration records must be rejected");

        assert_eq!(
            error,
            InterfaceSemanticCommitError::ConflictingRecord {
                owner,
                kind: InterfaceSemanticRecordKind::DeclaredType,
            }
        );
    }

    #[test]
    fn cyclic_fragment_types_report_their_table() {
        let fragment = InterfaceSemantics::new().with_values(
            [],
            [InterfaceType::Tuple(Arc::new([InterfaceTypeId::new(0)]))],
            [],
            [],
        );

        let remap =
            InterfaceSemanticIdRemap::new().with_values([], [InterfaceTypeId::new(0)], [], []);

        let error =
            commit_interface_semantic_fragments(InterfaceSemantics::new(), [(fragment, remap)])
                .expect_err("cyclic fragment type tables must be rejected");

        assert_eq!(
            error,
            InterfaceSemanticCommitError::CyclicReference(InterfaceSemanticTableKind::Type)
        );
    }

    #[test]
    fn package_records_report_their_exact_table() {
        let provenance = InterfaceSourceProvenance::try_new(
            InterfaceSymbolReference::Local(InterfaceSymbolId::new(1)),
            "src/example.bray",
            0,
            1,
        )
        .unwrap_or_else(|| panic!("test provenance must be valid"));

        let fragment = InterfaceSemantics::new().with_provenance([provenance]);

        let error = commit_interface_semantic_fragments(
            InterfaceSemantics::new(),
            [(fragment, InterfaceSemanticIdRemap::new())],
        )
        .expect_err("package records must be rejected from declaration fragments");

        assert_eq!(
            error,
            InterfaceSemanticCommitError::UnexpectedPackageRecord(
                InterfaceSemanticTableKind::Provenance
            )
        );
    }
}
