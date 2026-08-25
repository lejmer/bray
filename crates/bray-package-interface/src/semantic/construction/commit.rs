use std::sync::Arc;

use bray_symbols::InterfaceSupportEntityId;

use super::super::model::{
    InterfaceCallableContract, InterfaceCallableContractClause,
    InterfaceCallableContractClauseValue,
    InterfaceCallablePhaseBehavior, InterfaceCheckedTemplate, InterfaceCheckedTemplateBehavior,
    InterfaceCheckedTemplateExecution, InterfaceCheckedTemplateId, InterfaceCheckedTemplateInput,
    InterfaceCheckedTemplateNode, InterfaceCheckedTemplateOperation,
    InterfaceCheckedTemplateTemporary,
    InterfaceConstraint, InterfaceConstraintKind, InterfaceDeclarationTemplate,
    InterfaceImplementationReference,
    InterfacePredicateSummary, InterfaceSemanticRecordKind, InterfaceSemantics,
    InterfaceStorageMember, InterfaceStorageShape, InterfaceSupportEntity,
    InterfaceSupportImplementation, InterfaceTemplateReference,
};
use super::model::{
    InterfaceSemanticCommitError, InterfaceSemanticIdRemap, InterfaceSemanticTableKind,
};
use crate::InterfaceSymbolReference;

/// Commits declaration-owned semantic fragments onto one package-wide value table.
///
/// The caller supplies fragments in stable symbol order. Package-level implementation,
/// compatibility, runtime, and provenance records belong in `base`.
pub fn commit_interface_semantic_fragments(
    mut base: InterfaceSemantics,
    fragments: impl IntoIterator<Item = (InterfaceSemantics, InterfaceSemanticIdRemap)>,
) -> Result<InterfaceSemantics, InterfaceSemanticCommitError> {
    let mut checked_template_count = base.checked_templates.len();
    let mut support_entity_count = base.support_entities.len();
    let mut committed = Vec::new();

    for (fragment, remap) in fragments {
        let fragment = commit_fragment(
            fragment,
            &remap,
            checked_template_count,
            support_entity_count,
        )?;

        checked_template_count = checked_template_count
            .checked_add(fragment.checked_templates.len())
            .ok_or(InterfaceSemanticCommitError::IdentityOverflow(
                InterfaceSemanticTableKind::CheckedTemplate,
            ))?;

        support_entity_count = support_entity_count
            .checked_add(fragment.support_entities.len())
            .ok_or(InterfaceSemanticCommitError::IdentityOverflow(
                InterfaceSemanticTableKind::SupportEntity,
            ))?;

        committed.push(fragment);
    }

    extend_records(
        &mut base.constraints,
        committed
            .iter()
            .flat_map(|fragment| fragment.constraints.iter().cloned()),
    );

    extend_records(
        &mut base.callable_contracts,
        committed
            .iter()
            .flat_map(|fragment| fragment.callable_contracts.iter().cloned()),
    );

    extend_records(
        &mut base.callable_signatures,
        committed
            .iter()
            .flat_map(|fragment| fragment.callable_signatures.iter().cloned()),
    );

    extend_records(
        &mut base.generic_declarations,
        committed
            .iter()
            .flat_map(|fragment| fragment.generic_declarations.iter().cloned()),
    );

    extend_records(
        &mut base.callable_parameter_defaults,
        committed
            .iter()
            .flat_map(|fragment| fragment.callable_parameter_defaults.iter().cloned()),
    );

    extend_records(
        &mut base.predicate_definitions,
        committed
            .iter()
            .flat_map(|fragment| fragment.predicate_definitions.iter().cloned()),
    );

    extend_records(
        &mut base.declared_types,
        committed
            .iter()
            .flat_map(|fragment| fragment.declared_types.iter().cloned()),
    );

    extend_records(
        &mut base.type_representations,
        committed
            .iter()
            .flat_map(|fragment| fragment.type_representations.iter().cloned()),
    );

    extend_records(
        &mut base.checked_templates,
        committed
            .iter()
            .flat_map(|fragment| fragment.checked_templates.iter().cloned()),
    );

    extend_records(
        &mut base.support_entities,
        committed
            .iter()
            .flat_map(|fragment| fragment.support_entities.iter().cloned()),
    );

    extend_records(
        &mut base.declaration_templates,
        committed
            .iter()
            .flat_map(|fragment| fragment.declaration_templates.iter().cloned()),
    );

    normalize_records(
        &mut base.constraints,
        InterfaceSemanticRecordKind::GenericConstraint,
        |left, right| left.owner == right.owner && left.ordinal == right.ordinal,
        |record| &record.owner,
    )?;

    normalize_records(
        &mut base.callable_contracts,
        InterfaceSemanticRecordKind::CallableContracts,
        |left, right| left.owner == right.owner,
        InterfaceCallableContract::owner,
    )?;

    normalize_records(
        &mut base.callable_signatures,
        InterfaceSemanticRecordKind::CallableSignature,
        |left, right| left.owner == right.owner,
        |record| &record.owner,
    )?;

    normalize_records(
        &mut base.generic_declarations,
        InterfaceSemanticRecordKind::GenericDeclaration,
        |left, right| left.owner == right.owner,
        |record| &record.owner,
    )?;

    normalize_records(
        &mut base.callable_parameter_defaults,
        InterfaceSemanticRecordKind::CallableParameterDefault,
        |left, right| left.parameter == right.parameter,
        |record| &record.parameter,
    )?;

    normalize_records(
        &mut base.predicate_definitions,
        InterfaceSemanticRecordKind::PredicateDefinition,
        |left, right| left.owner == right.owner,
        |record| &record.owner,
    )?;

    normalize_records(
        &mut base.declared_types,
        InterfaceSemanticRecordKind::DeclaredType,
        |left, right| left.owner == right.owner,
        |record| &record.owner,
    )?;

    normalize_records(
        &mut base.type_representations,
        InterfaceSemanticRecordKind::TypeRepresentation,
        |left, right| left.owner == right.owner,
        |record| &record.owner,
    )?;

    normalize_records(
        &mut base.declaration_templates,
        InterfaceSemanticRecordKind::DeclarationTemplate,
        |left, right| {
            left.owner() == right.owner()
                && left.kind() == right.kind()
                && left.ordinal() == right.ordinal()
        },
        InterfaceDeclarationTemplate::owner,
    )?;

    Ok(base)
}

fn commit_fragment(
    mut fragment: InterfaceSemantics,
    remap: &InterfaceSemanticIdRemap,
    checked_template_count: usize,
    support_entity_count: usize,
) -> Result<InterfaceSemantics, InterfaceSemanticCommitError> {
    remap.validate(&fragment)?;

    if !fragment.implementations.is_empty()
        || !fragment.coherence.is_empty()
        || !fragment.target_dependencies.is_empty()
        || !fragment.abi_dependencies.is_empty()
        || !fragment.runtime_requirements.is_empty()
        || !fragment.provenance.is_empty()
    {
        return Err(InterfaceSemanticCommitError::UnexpectedPackageRecord);
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
    for clause in Arc::make_mut(&mut contract.invocation_preconditions) {
        remap_callable_clause(clause, remap)?;
    }

    for clause in Arc::make_mut(&mut contract.static_constraints) {
        remap_callable_clause(clause, remap)?;
    }

    for clause in Arc::make_mut(&mut contract.normal_completion_postconditions) {
        remap_callable_clause(clause, remap)?;
    }

    remap_callable_behavior(&mut contract.invocation_behavior, remap)?;

    if let Some(behavior) = &mut contract.deferred_execution_behavior {
        remap_callable_behavior(behavior, remap)?;
    }

    Ok(())
}

fn remap_callable_clause(
    clause: &mut InterfaceCallableContractClause,
    remap: &InterfaceSemanticIdRemap,
) -> Result<(), InterfaceSemanticCommitError> {
    match &mut clause.value {
        InterfaceCallableContractClauseValue::Predicate(predicate) => {
            remap_predicate(predicate, remap)
        }
        InterfaceCallableContractClauseValue::TraitSatisfaction {
            subject,
            application,
        } => {
            *subject = remap.ty(*subject)?;
            *application = remap.trait_application(*application)?;

            Ok(())
        }
    }
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

fn remap_checked_template(
    template: &InterfaceCheckedTemplate,
    remap: &InterfaceSemanticIdRemap,
    support_base: u32,
    support_count: usize,
) -> Result<InterfaceCheckedTemplate, InterfaceSemanticCommitError> {
    let inputs = template
        .inputs()
        .iter()
        .map(|input| {
            Ok(InterfaceCheckedTemplateInput::new(
                input.kind().clone(),
                remap.ty(input.ty())?,
            ))
        })
        .collect::<Result<Vec<_>, InterfaceSemanticCommitError>>()?;

    let nodes = template
        .nodes()
        .iter()
        .map(|node| {
            Ok(InterfaceCheckedTemplateNode::new(
                remap_checked_operation(node.operation(), remap, support_base, support_count)?,
                remap.ty(node.ty())?,
            ))
        })
        .collect::<Result<Vec<_>, InterfaceSemanticCommitError>>()?;

    let temporaries = template
        .temporaries()
        .iter()
        .copied()
        .map(|temporary| {
            Ok(InterfaceCheckedTemplateTemporary::new(
                temporary.initializer(),
                remap.ty(temporary.ty())?,
                remap.dependency_contract(temporary.dependency_contract())?,
            ))
        })
        .collect::<Result<Vec<_>, InterfaceSemanticCommitError>>()?;

    let behavior =
        remap_checked_behavior(template.behavior(), remap, support_base, support_count)?;

    Ok(InterfaceCheckedTemplate::new(
        template.kind(),
        inputs,
        nodes,
        temporaries,
        template.result(),
        behavior,
    ))
}

fn remap_checked_behavior(
    behavior: &InterfaceCheckedTemplateBehavior,
    remap: &InterfaceSemanticIdRemap,
    support_base: u32,
    support_count: usize,
) -> Result<InterfaceCheckedTemplateBehavior, InterfaceSemanticCommitError> {
    let execution = InterfaceCheckedTemplateExecution::new(
        behavior.execution_requirements().iter().cloned(),
        behavior.current_run_cancellation(),
    );

    let witnesses = behavior
        .witnesses()
        .iter()
        .map(|reference| remap_implementation_reference(reference, support_base, support_count))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(InterfaceCheckedTemplateBehavior::new(
        behavior.effects().iter().cloned(),
        behavior.capabilities().iter().cloned(),
        behavior.trusted_obligations().iter().cloned(),
        execution,
        behavior.lifecycle_obligations().iter().copied(),
        remap.dependency_contract(behavior.dependency_contract())?,
        witnesses,
    ))
}

fn remap_checked_operation(
    operation: &InterfaceCheckedTemplateOperation,
    remap: &InterfaceSemanticIdRemap,
    support_base: u32,
    support_count: usize,
) -> Result<InterfaceCheckedTemplateOperation, InterfaceSemanticCommitError> {
    let operation = match operation {
        InterfaceCheckedTemplateOperation::Input(input) => {
            InterfaceCheckedTemplateOperation::Input(*input)
        }
        InterfaceCheckedTemplateOperation::Constant { term, usage } => {
            InterfaceCheckedTemplateOperation::Constant {
                term: remap.constant_term(*term)?,
                usage: *usage,
            }
        }
        InterfaceCheckedTemplateOperation::Unary { operation, operand } => {
            InterfaceCheckedTemplateOperation::Unary {
                operation: *operation,
                operand: *operand,
            }
        }
        InterfaceCheckedTemplateOperation::Binary {
            operation,
            left,
            right,
        } => InterfaceCheckedTemplateOperation::Binary {
            operation: *operation,
            left: *left,
            right: *right,
        },
        InterfaceCheckedTemplateOperation::Borrow { kind, operand } => {
            InterfaceCheckedTemplateOperation::Borrow {
                kind: *kind,
                operand: *operand,
            }
        }
        InterfaceCheckedTemplateOperation::Declaration {
            declaration,
            substitution,
        } => InterfaceCheckedTemplateOperation::Declaration {
            declaration: remap_template_reference(declaration, support_base, support_count)?,
            substitution: substitution
                .map(|substitution| remap.substitution(substitution))
                .transpose()?,
        },
        InterfaceCheckedTemplateOperation::Call {
            callable,
            substitution,
            arguments,
            implementation,
        } => InterfaceCheckedTemplateOperation::Call {
            callable: remap_template_reference(callable, support_base, support_count)?,
            substitution: remap.substitution(*substitution)?,
            arguments: Arc::clone(arguments),
            implementation: implementation
                .as_ref()
                .map(|(implementation, substitution)| {
                    Ok((
                        remap_implementation_reference(
                            implementation,
                            support_base,
                            support_count,
                        )?,
                        remap.substitution(*substitution)?,
                    ))
                })
                .transpose()?,
        },
        InterfaceCheckedTemplateOperation::Convert { value, target } => {
            InterfaceCheckedTemplateOperation::Convert {
                value: *value,
                target: remap.ty(*target)?,
            }
        }
        InterfaceCheckedTemplateOperation::Tuple(elements) => {
            InterfaceCheckedTemplateOperation::Tuple(Arc::clone(elements))
        }
        InterfaceCheckedTemplateOperation::Array(elements) => {
            InterfaceCheckedTemplateOperation::Array(Arc::clone(elements))
        }
        InterfaceCheckedTemplateOperation::Project { subject, member } => {
            InterfaceCheckedTemplateOperation::Project {
                subject: *subject,
                member: remap_template_reference(member, support_base, support_count)?,
            }
        }
        InterfaceCheckedTemplateOperation::Conditional {
            condition,
            when_true,
            when_false,
        } => InterfaceCheckedTemplateOperation::Conditional {
            condition: *condition,
            when_true: *when_true,
            when_false: *when_false,
        },
        InterfaceCheckedTemplateOperation::ShortCircuit { kind, left, right } => {
            InterfaceCheckedTemplateOperation::ShortCircuit {
                kind: *kind,
                left: *left,
                right: *right,
            }
        }
        InterfaceCheckedTemplateOperation::Temporary(temporary) => {
            InterfaceCheckedTemplateOperation::Temporary(*temporary)
        }
    };

    Ok(operation)
}

fn remap_template_reference(
    reference: &InterfaceTemplateReference,
    support_base: u32,
    support_count: usize,
) -> Result<InterfaceTemplateReference, InterfaceSemanticCommitError> {
    match reference {
        InterfaceTemplateReference::Symbol(symbol) => {
            Ok(InterfaceTemplateReference::Symbol(symbol.clone()))
        }
        InterfaceTemplateReference::Support(entity) => {
            offset_support_entity(*entity, support_base, support_count)
                .map(InterfaceTemplateReference::Support)
        }
    }
}

fn remap_implementation_reference(
    reference: &InterfaceImplementationReference,
    support_base: u32,
    support_count: usize,
) -> Result<InterfaceImplementationReference, InterfaceSemanticCommitError> {
    match reference {
        InterfaceImplementationReference::Symbol(symbol) => {
            Ok(InterfaceImplementationReference::Symbol(symbol.clone()))
        }
        InterfaceImplementationReference::Support(entity) => {
            offset_support_entity(*entity, support_base, support_count)
                .map(InterfaceImplementationReference::Support)
        }
    }
}

fn remap_support_entity(
    entity: &InterfaceSupportEntity,
    remap: &InterfaceSemanticIdRemap,
    checked_template_base: u32,
    checked_template_count: usize,
) -> Result<InterfaceSupportEntity, InterfaceSemanticCommitError> {
    match entity {
        InterfaceSupportEntity::Declaration(declaration) => {
            Ok(InterfaceSupportEntity::Declaration(declaration.clone()))
        }
        InterfaceSupportEntity::Implementation(implementation) => {
            Ok(InterfaceSupportEntity::Implementation(
                InterfaceSupportImplementation::new(
                    implementation.declaration().clone(),
                    remap.ty(implementation.subject())?,
                    implementation
                        .trait_application()
                        .map(|application| remap.trait_application(application))
                        .transpose()?,
                ),
            ))
        }
        InterfaceSupportEntity::CheckedTemplate(template) => offset_checked_template(
            *template,
            checked_template_base,
            checked_template_count,
        )
        .map(InterfaceSupportEntity::CheckedTemplate),
    }
}

fn offset_support_entity(
    id: InterfaceSupportEntityId,
    base: u32,
    count: usize,
) -> Result<InterfaceSupportEntityId, InterfaceSemanticCommitError> {
    offset_reference(
        id.raw(),
        base,
        count,
        InterfaceSemanticTableKind::SupportEntity,
    )
    .map(InterfaceSupportEntityId::new)
}

fn offset_checked_template(
    id: InterfaceCheckedTemplateId,
    base: u32,
    count: usize,
) -> Result<InterfaceCheckedTemplateId, InterfaceSemanticCommitError> {
    offset_reference(
        id.raw(),
        base,
        count,
        InterfaceSemanticTableKind::CheckedTemplate,
    )
    .map(InterfaceCheckedTemplateId::new)
}

fn offset_reference(
    raw: u32,
    base: u32,
    count: usize,
    table: InterfaceSemanticTableKind,
) -> Result<u32, InterfaceSemanticCommitError> {
    let index = usize::try_from(raw).ok();

    if index.is_none_or(|index| index >= count) {
        return Err(InterfaceSemanticCommitError::MissingReference {
            table,
            reference: raw,
        });
    }

    base.checked_add(raw)
        .ok_or(InterfaceSemanticCommitError::IdentityOverflow(table))
}

fn offset(
    length: usize,
    table: InterfaceSemanticTableKind,
) -> Result<u32, InterfaceSemanticCommitError> {
    u32::try_from(length).map_err(|_| InterfaceSemanticCommitError::IdentityOverflow(table))
}

fn normalize_records<T>(
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

fn extend_records<T: Clone>(
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
        InterfaceDeclaredType, InterfaceSemanticRecordKind, InterfaceSemantics, InterfaceType,
        InterfaceTypeId,
    };
    use super::{
        InterfaceSemanticCommitError, InterfaceSemanticIdRemap, InterfaceSemanticTableKind,
        commit_interface_semantic_fragments,
    };
    use bray_symbols::InterfaceSymbolId;

    use crate::InterfaceSymbolReference;

    #[test]
    fn shared_fragment_types_receive_one_package_identity() {
        let base = InterfaceSemantics::new().with_values(
            [],
            [InterfaceType::Tuple(Arc::new([]))],
            [],
            [],
        );

        let fragments = [1, 2].map(|owner| {
            let fragment = InterfaceSemantics::new()
                .with_values([], [InterfaceType::Tuple(Arc::new([]))], [], [])
                .with_declared_types([InterfaceDeclaredType::new(
                    InterfaceSymbolReference::Local(InterfaceSymbolId::new(owner)),
                    InterfaceTypeId::new(0),
                )]);

            let remap = InterfaceSemanticIdRemap::new().with_values(
                [],
                [InterfaceTypeId::new(0)],
                [],
                [],
            );

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
        let fragment = InterfaceSemantics::new().with_values(
            [],
            [InterfaceType::Tuple(Arc::new([]))],
            [],
            [],
        );

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

        let remap = InterfaceSemanticIdRemap::new().with_values(
            [],
            [InterfaceTypeId::new(0)],
            [],
            [],
        );

        let error = commit_interface_semantic_fragments(
            InterfaceSemantics::new(),
            [(fragment, remap)],
        )
        .expect_err("cyclic fragment type tables must be rejected");

        assert_eq!(
            error,
            InterfaceSemanticCommitError::CyclicReference(InterfaceSemanticTableKind::Type)
        );
    }
}
