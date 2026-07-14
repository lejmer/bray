use bray_bound_tree::{CheckedTemplateInputId, CheckedTemplateNodeId, CheckedTemplateTemporaryId};
use bray_symbols::{InterfaceSupportEntityId, SymbolOrdinal};

use super::common::{decode_tag, validate_record_count};
use crate::semantic::codec::common::{
    SemanticDecodeContext, map_wire_error, read_count, read_symbol_reference, read_u32,
};
use crate::semantic::model::{
    InterfaceCheckedTemplate, InterfaceCheckedTemplateBehavior, InterfaceCheckedTemplateInput,
    InterfaceCheckedTemplateInputKind, InterfaceCheckedTemplateNode,
    InterfaceCheckedTemplateOperation, InterfaceCheckedTemplateTemporary, InterfaceConstantTermId,
    InterfaceDeclarationTemplate, InterfaceDependencyContractId, InterfaceImplementationReference,
    InterfaceSemanticFacts, InterfaceTemplateReference, InterfaceTypeId,
};
use crate::wire::WireReader;
use crate::{
    InterfaceLimit, InterfaceValidationError, InterfaceValidationLimits, ValidatedInterfaceSection,
};

pub(super) fn decode_templates(
    section: ValidatedInterfaceSection<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
    facts: &mut InterfaceSemanticFacts,
) -> Result<(), InterfaceValidationError> {
    let mut reader = WireReader::new(section.bytes());

    let template_count = read_count(&mut reader, limits, InterfaceLimit::RecordCount)?;
    let declaration_count = read_count(&mut reader, limits, InterfaceLimit::RecordCount)?;

    validate_record_count(section, [template_count, declaration_count])?;

    let mut templates = Vec::with_capacity(template_count);

    for _ in 0..template_count {
        templates.push(decode_template(&mut reader, limits, context)?);
    }

    let mut declarations = Vec::with_capacity(declaration_count);

    for _ in 0..declaration_count {
        declarations.push(InterfaceDeclarationTemplate::new(
            read_symbol_reference(&mut reader, context)?,
            decode_tag(read_u32(&mut reader)?)?,
            SymbolOrdinal::new(read_u32(&mut reader)?),
            InterfaceSupportEntityId::new(read_u32(&mut reader)?),
        ));
    }

    reader.finish().map_err(map_wire_error)?;

    facts.checked_templates = templates.into();
    facts.declaration_templates = declarations.into();

    Ok(())
}

fn decode_template(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
) -> Result<InterfaceCheckedTemplate, InterfaceValidationError> {
    let kind = decode_tag(read_u32(reader)?)?;
    let input_count = read_count(reader, limits, InterfaceLimit::TemplateGraphSize)?;
    let mut graph_size = input_count;
    let mut inputs = Vec::with_capacity(input_count);

    for _ in 0..input_count {
        inputs.push(InterfaceCheckedTemplateInput::new(
            decode_input_kind(reader, context)?,
            InterfaceTypeId::new(read_u32(reader)?),
        ));
    }

    let node_count = read_count(reader, limits, InterfaceLimit::TemplateGraphSize)?;

    graph_size = add_graph_count(graph_size, node_count, limits)?;

    let mut nodes = Vec::with_capacity(node_count);

    for _ in 0..node_count {
        nodes.push(InterfaceCheckedTemplateNode::new(
            decode_operation(reader, limits, context)?,
            InterfaceTypeId::new(read_u32(reader)?),
        ));
    }

    let temporary_count = read_count(reader, limits, InterfaceLimit::TemplateGraphSize)?;

    add_graph_count(graph_size, temporary_count, limits)?;

    let mut temporaries = Vec::with_capacity(temporary_count);

    for _ in 0..temporary_count {
        temporaries.push(InterfaceCheckedTemplateTemporary::new(
            CheckedTemplateNodeId::new(read_u32(reader)?),
            InterfaceTypeId::new(read_u32(reader)?),
            InterfaceDependencyContractId::new(read_u32(reader)?),
        ));
    }

    let result = CheckedTemplateNodeId::new(read_u32(reader)?);

    let effects = read_symbol_references(reader, limits, context)?;
    let capabilities = read_symbol_references(reader, limits, context)?;
    let trusted_obligations = read_symbol_references(reader, limits, context)?;
    let lifecycle_count = read_count(reader, limits, InterfaceLimit::RecordCount)?;

    let mut lifecycle_obligations = Vec::with_capacity(lifecycle_count);

    for _ in 0..lifecycle_count {
        lifecycle_obligations.push(decode_tag(read_u32(reader)?)?);
    }

    let dependency_contract = InterfaceDependencyContractId::new(read_u32(reader)?);

    let witness_count = read_count(reader, limits, InterfaceLimit::RecordCount)?;
    let mut witnesses = Vec::with_capacity(witness_count);

    for _ in 0..witness_count {
        witnesses.push(decode_implementation_reference(reader, context)?);
    }

    let behavior = InterfaceCheckedTemplateBehavior::new(
        effects,
        capabilities,
        trusted_obligations,
        lifecycle_obligations,
        dependency_contract,
        witnesses,
    );

    Ok(InterfaceCheckedTemplate::new(
        kind,
        inputs,
        nodes,
        temporaries,
        result,
        behavior,
    ))
}

fn add_graph_count(
    current: usize,
    additional: usize,
    limits: InterfaceValidationLimits,
) -> Result<usize, InterfaceValidationError> {
    let total =
        current
            .checked_add(additional)
            .ok_or(InterfaceValidationError::ResourceLimitExceeded {
                limit: InterfaceLimit::TemplateGraphSize,
                actual: u64::MAX,
                maximum: limits.maximum(InterfaceLimit::TemplateGraphSize),
            })?;

    limits.check(
        InterfaceLimit::TemplateGraphSize,
        u64::try_from(total).unwrap_or(u64::MAX),
    )?;

    Ok(total)
}

fn decode_input_kind(
    reader: &mut WireReader<'_>,
    context: &mut SemanticDecodeContext,
) -> Result<InterfaceCheckedTemplateInputKind, InterfaceValidationError> {
    match read_u32(reader)? {
        1 => Ok(InterfaceCheckedTemplateInputKind::GenericType(
            read_symbol_reference(reader, context)?,
        )),
        2 => Ok(InterfaceCheckedTemplateInputKind::GenericConstant(
            read_symbol_reference(reader, context)?,
        )),
        3 => Ok(InterfaceCheckedTemplateInputKind::Receiver),
        4 => Ok(InterfaceCheckedTemplateInputKind::Parameter(
            SymbolOrdinal::new(read_u32(reader)?),
        )),
        5 => Ok(InterfaceCheckedTemplateInputKind::PostconditionResult),
        _ => Err(InterfaceValidationError::Malformed),
    }
}

fn decode_operation(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
) -> Result<InterfaceCheckedTemplateOperation, InterfaceValidationError> {
    match read_u32(reader)? {
        1 => Ok(InterfaceCheckedTemplateOperation::Input(
            CheckedTemplateInputId::new(read_u32(reader)?),
        )),
        2 => Ok(InterfaceCheckedTemplateOperation::Constant(
            InterfaceConstantTermId::new(read_u32(reader)?),
        )),
        3 => Ok(InterfaceCheckedTemplateOperation::Declaration(
            decode_template_reference(reader, context)?,
        )),
        4 => {
            let callable = decode_template_reference(reader, context)?;
            let arguments = read_node_ids(reader, limits)?;

            let implementation = match read_u32(reader)? {
                0 => None,
                1 => Some(decode_implementation_reference(reader, context)?),
                _ => return Err(InterfaceValidationError::Malformed),
            };

            Ok(InterfaceCheckedTemplateOperation::call(
                callable,
                arguments,
                implementation,
            ))
        }
        5 => Ok(InterfaceCheckedTemplateOperation::Convert {
            value: CheckedTemplateNodeId::new(read_u32(reader)?),
            target: InterfaceTypeId::new(read_u32(reader)?),
        }),
        6 => Ok(InterfaceCheckedTemplateOperation::tuple(read_node_ids(
            reader, limits,
        )?)),
        7 => Ok(InterfaceCheckedTemplateOperation::array(read_node_ids(
            reader, limits,
        )?)),
        8 => Ok(InterfaceCheckedTemplateOperation::Project {
            subject: CheckedTemplateNodeId::new(read_u32(reader)?),
            member: decode_template_reference(reader, context)?,
        }),
        9 => Ok(InterfaceCheckedTemplateOperation::Conditional {
            condition: CheckedTemplateNodeId::new(read_u32(reader)?),
            when_true: CheckedTemplateNodeId::new(read_u32(reader)?),
            when_false: CheckedTemplateNodeId::new(read_u32(reader)?),
        }),
        10 => Ok(InterfaceCheckedTemplateOperation::ShortCircuit {
            kind: decode_tag(read_u32(reader)?)?,
            left: CheckedTemplateNodeId::new(read_u32(reader)?),
            right: CheckedTemplateNodeId::new(read_u32(reader)?),
        }),
        11 => Ok(InterfaceCheckedTemplateOperation::Temporary(
            CheckedTemplateTemporaryId::new(read_u32(reader)?),
        )),
        _ => Err(InterfaceValidationError::Malformed),
    }
}

fn read_node_ids(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
) -> Result<Vec<CheckedTemplateNodeId>, InterfaceValidationError> {
    let count = read_count(reader, limits, InterfaceLimit::TemplateGraphSize)?;
    let mut nodes = Vec::with_capacity(count);

    for _ in 0..count {
        nodes.push(CheckedTemplateNodeId::new(read_u32(reader)?));
    }

    Ok(nodes)
}

fn read_symbol_references(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
) -> Result<Vec<crate::InterfaceSymbolReference>, InterfaceValidationError> {
    let count = read_count(reader, limits, InterfaceLimit::RecordCount)?;
    let mut references = Vec::with_capacity(count);

    for _ in 0..count {
        references.push(read_symbol_reference(reader, context)?);
    }

    Ok(references)
}

fn decode_template_reference(
    reader: &mut WireReader<'_>,
    context: &mut SemanticDecodeContext,
) -> Result<InterfaceTemplateReference, InterfaceValidationError> {
    match read_u32(reader)? {
        1 => Ok(InterfaceTemplateReference::Symbol(read_symbol_reference(
            reader, context,
        )?)),
        2 => Ok(InterfaceTemplateReference::Support(
            InterfaceSupportEntityId::new(read_u32(reader)?),
        )),
        _ => Err(InterfaceValidationError::Malformed),
    }
}

fn decode_implementation_reference(
    reader: &mut WireReader<'_>,
    context: &mut SemanticDecodeContext,
) -> Result<InterfaceImplementationReference, InterfaceValidationError> {
    match read_u32(reader)? {
        1 => Ok(InterfaceImplementationReference::Symbol(
            read_symbol_reference(reader, context)?,
        )),
        2 => Ok(InterfaceImplementationReference::Support(
            InterfaceSupportEntityId::new(read_u32(reader)?),
        )),
        _ => Err(InterfaceValidationError::Malformed),
    }
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{
        CheckedTemplateInputId, CheckedTemplateKind, CheckedTemplateNodeId,
        CheckedTemplateShortCircuitKind, CheckedTemplateTemporaryId,
    };
    use bray_symbols::{
        ExternalSymbolKey, InterfaceSupportEntityId, InterfaceSymbolId, LifecycleObligationKind,
        ModulePathKey, PackageIdentity, SymbolKind, SymbolName, SymbolOrdinal,
    };

    use super::super::decode_semantic_facts;
    use crate::semantic::codec::encode_semantic_facts;
    use crate::{
        EncodedSemanticSection, InterfaceCheckedTemplate, InterfaceCheckedTemplateBehavior,
        InterfaceCheckedTemplateId, InterfaceCheckedTemplateInput,
        InterfaceCheckedTemplateInputKind, InterfaceCheckedTemplateNode,
        InterfaceCheckedTemplateOperation, InterfaceCheckedTemplateTemporary,
        InterfaceConstantTerm, InterfaceConstantValue, InterfaceConstantValueId,
        InterfaceConstantValueKind, InterfaceDeclarationTemplate, InterfaceDependencyContract,
        InterfaceImplementationReference, InterfaceSectionTag, InterfaceSemanticFacts,
        InterfaceSupportEntity, InterfaceSupportImplementation, InterfaceSymbolReference,
        InterfaceTemplateReference, InterfaceType, InterfaceTypeId, InterfaceValidationError,
        InterfaceValidationLimits, ValidatedInterfaceSection,
    };

    #[test]
    fn every_declaration_template_kind_round_trips_with_private_support() {
        let facts = template_facts();
        let limits = InterfaceValidationLimits::default();
        let sections = encode_semantic_facts(&facts, 5, 0, limits);

        let Ok(sections) = sections else {
            panic!("valid checked templates must encode");
        };

        let owned = owned_sections(&sections);
        let views = section_views(&owned);
        let decoded = decode_semantic_facts(&views, 5, 0, limits);

        assert_eq!(decoded, Ok(facts));
    }

    #[test]
    fn every_checked_template_operation_round_trips() {
        let facts = operation_facts();
        let limits = InterfaceValidationLimits::default();
        let sections = encode_semantic_facts(&facts, 1, 0, limits);

        let Ok(sections) = sections else {
            panic!("valid checked-template operations must encode");
        };

        let owned = owned_sections(&sections);

        assert_eq!(
            decode_semantic_facts(&section_views(&owned), 1, 0, limits),
            Ok(facts)
        );
    }

    #[test]
    fn template_validation_rejects_forward_nodes_and_wrong_support_categories() {
        let facts = template_facts();

        let behavior = InterfaceCheckedTemplateBehavior::new(
            [],
            [],
            [],
            [],
            crate::InterfaceDependencyContractId::new(0),
            [],
        );

        let invalid = InterfaceCheckedTemplate::new(
            CheckedTemplateKind::RuntimeDefault,
            [],
            [InterfaceCheckedTemplateNode::new(
                InterfaceCheckedTemplateOperation::tuple([CheckedTemplateNodeId::new(0)]),
                InterfaceTypeId::new(0),
            )],
            [],
            CheckedTemplateNodeId::new(0),
            behavior,
        );

        let forward_reference = facts.clone().with_templates(
            [invalid],
            [InterfaceDeclarationTemplate::new(
                local(0),
                CheckedTemplateKind::RuntimeDefault,
                SymbolOrdinal::new(0),
                InterfaceSupportEntityId::new(0),
            )],
            [InterfaceSupportEntity::CheckedTemplate(
                InterfaceCheckedTemplateId::new(0),
            )],
        );

        assert_eq!(
            encode_semantic_facts(
                &forward_reference,
                5,
                0,
                InterfaceValidationLimits::default()
            ),
            Err(InterfaceValidationError::Malformed)
        );

        let mut entities = facts.support_entities().to_vec();

        let InterfaceSupportEntity::CheckedTemplate(template) = entities[2] else {
            panic!("fixture template support entity must be present");
        };

        entities[2] = InterfaceSupportEntity::Declaration(helper_key());
        entities[0] = InterfaceSupportEntity::CheckedTemplate(template);

        let templates = facts.checked_templates().to_vec();
        let declarations = facts.declaration_templates().to_vec();

        let wrong_support_category = facts.with_templates(templates, declarations, entities);

        assert_eq!(
            encode_semantic_facts(
                &wrong_support_category,
                5,
                0,
                InterfaceValidationLimits::default()
            ),
            Err(InterfaceValidationError::Malformed)
        );
    }

    #[test]
    fn template_decode_rejects_unknown_kinds_and_graph_limit_excess() {
        let facts = template_facts();
        let limits = InterfaceValidationLimits::default();
        let sections = encode_semantic_facts(&facts, 5, 0, limits);

        let Ok(sections) = sections else {
            panic!("valid checked templates must encode");
        };

        let mut owned = owned_sections(&sections);

        let Some((_, _, templates)) = owned
            .iter_mut()
            .find(|(tag, _, _)| *tag == InterfaceSectionTag::DeclarationTemplates)
        else {
            panic!("template section must be encoded");
        };

        templates[8..12].copy_from_slice(&u32::MAX.to_le_bytes());

        assert_eq!(
            decode_semantic_facts(&section_views(&owned), 5, 0, limits),
            Err(InterfaceValidationError::Malformed)
        );

        let constrained = limits.with_template_graph_size(1);

        assert!(matches!(
            encode_semantic_facts(&facts, 5, 0, constrained),
            Err(InterfaceValidationError::ResourceLimitExceeded {
                limit: crate::InterfaceLimit::TemplateGraphSize,
                ..
            })
        ));
    }

    fn template_facts() -> InterfaceSemanticFacts {
        let kinds = [
            CheckedTemplateKind::RuntimeDefault,
            CheckedTemplateKind::ConstantDefinition,
            CheckedTemplateKind::PredicateDefinition,
            CheckedTemplateKind::GenericConstraint,
            CheckedTemplateKind::CallableContract,
        ];

        let templates = kinds.into_iter().enumerate().map(|(index, kind)| {
            let input_kind = match index {
                0 => InterfaceCheckedTemplateInputKind::GenericType(local(0)),
                1 => InterfaceCheckedTemplateInputKind::GenericConstant(local(1)),
                2 => InterfaceCheckedTemplateInputKind::Receiver,
                3 => InterfaceCheckedTemplateInputKind::Parameter(SymbolOrdinal::new(0)),
                _ => InterfaceCheckedTemplateInputKind::PostconditionResult,
            };

            let operation = if index == 0 {
                InterfaceCheckedTemplateOperation::Declaration(InterfaceTemplateReference::Support(
                    InterfaceSupportEntityId::new(0),
                ))
            } else {
                InterfaceCheckedTemplateOperation::Input(CheckedTemplateInputId::new(0))
            };

            let witnesses = (index == 1).then_some(InterfaceImplementationReference::Support(
                InterfaceSupportEntityId::new(1),
            ));

            let behavior = InterfaceCheckedTemplateBehavior::new(
                [],
                [],
                [],
                [],
                crate::InterfaceDependencyContractId::new(0),
                witnesses,
            );

            InterfaceCheckedTemplate::new(
                kind,
                [InterfaceCheckedTemplateInput::new(
                    input_kind,
                    InterfaceTypeId::new(0),
                )],
                [InterfaceCheckedTemplateNode::new(
                    operation,
                    InterfaceTypeId::new(0),
                )],
                [],
                CheckedTemplateNodeId::new(0),
                behavior,
            )
        });

        let declarations = kinds.into_iter().enumerate().map(|(index, kind)| {
            InterfaceDeclarationTemplate::new(
                local(index),
                kind,
                SymbolOrdinal::new(0),
                InterfaceSupportEntityId::new(index_u32(index + 2)),
            )
        });

        let template_entities = (0..kinds.len()).map(|index| {
            InterfaceSupportEntity::CheckedTemplate(InterfaceCheckedTemplateId::new(index_u32(
                index,
            )))
        });

        let support = [
            InterfaceSupportEntity::Declaration(helper_key()),
            InterfaceSupportEntity::Implementation(InterfaceSupportImplementation::new(
                implementation_key(),
                InterfaceTypeId::new(0),
                None,
            )),
        ]
        .into_iter()
        .chain(template_entities);

        InterfaceSemanticFacts::new()
            .with_values(
                [InterfaceDependencyContract::new([])],
                [InterfaceType::TypeParameter(local(0))],
                [],
                [],
            )
            .with_templates(templates, declarations, support)
    }

    fn operation_facts() -> InterfaceSemanticFacts {
        let declaration = InterfaceTemplateReference::Support(InterfaceSupportEntityId::new(0));

        let implementation =
            InterfaceImplementationReference::Support(InterfaceSupportEntityId::new(1));

        let nodes = [
            InterfaceCheckedTemplateNode::new(
                InterfaceCheckedTemplateOperation::Input(CheckedTemplateInputId::new(0)),
                InterfaceTypeId::new(0),
            ),
            InterfaceCheckedTemplateNode::new(
                InterfaceCheckedTemplateOperation::Constant(crate::InterfaceConstantTermId::new(0)),
                InterfaceTypeId::new(0),
            ),
            InterfaceCheckedTemplateNode::new(
                InterfaceCheckedTemplateOperation::Declaration(declaration.clone()),
                InterfaceTypeId::new(0),
            ),
            InterfaceCheckedTemplateNode::new(
                InterfaceCheckedTemplateOperation::call(
                    declaration.clone(),
                    [CheckedTemplateNodeId::new(0), CheckedTemplateNodeId::new(1)],
                    Some(implementation.clone()),
                ),
                InterfaceTypeId::new(0),
            ),
            InterfaceCheckedTemplateNode::new(
                InterfaceCheckedTemplateOperation::Convert {
                    value: CheckedTemplateNodeId::new(3),
                    target: InterfaceTypeId::new(0),
                },
                InterfaceTypeId::new(0),
            ),
            InterfaceCheckedTemplateNode::new(
                InterfaceCheckedTemplateOperation::tuple([
                    CheckedTemplateNodeId::new(0),
                    CheckedTemplateNodeId::new(1),
                ]),
                InterfaceTypeId::new(1),
            ),
            InterfaceCheckedTemplateNode::new(
                InterfaceCheckedTemplateOperation::array([
                    CheckedTemplateNodeId::new(0),
                    CheckedTemplateNodeId::new(1),
                ]),
                InterfaceTypeId::new(2),
            ),
            InterfaceCheckedTemplateNode::new(
                InterfaceCheckedTemplateOperation::Project {
                    subject: CheckedTemplateNodeId::new(2),
                    member: declaration,
                },
                InterfaceTypeId::new(0),
            ),
            InterfaceCheckedTemplateNode::new(
                InterfaceCheckedTemplateOperation::Conditional {
                    condition: CheckedTemplateNodeId::new(0),
                    when_true: CheckedTemplateNodeId::new(4),
                    when_false: CheckedTemplateNodeId::new(7),
                },
                InterfaceTypeId::new(0),
            ),
            InterfaceCheckedTemplateNode::new(
                InterfaceCheckedTemplateOperation::ShortCircuit {
                    kind: CheckedTemplateShortCircuitKind::And,
                    left: CheckedTemplateNodeId::new(4),
                    right: CheckedTemplateNodeId::new(7),
                },
                InterfaceTypeId::new(0),
            ),
            InterfaceCheckedTemplateNode::new(
                InterfaceCheckedTemplateOperation::Temporary(CheckedTemplateTemporaryId::new(0)),
                InterfaceTypeId::new(0),
            ),
        ];

        let behavior = InterfaceCheckedTemplateBehavior::new(
            [local(0)],
            [local(0)],
            [local(0)],
            [LifecycleObligationKind::Joining],
            crate::InterfaceDependencyContractId::new(0),
            [implementation],
        );

        let template = InterfaceCheckedTemplate::new(
            CheckedTemplateKind::RuntimeDefault,
            [InterfaceCheckedTemplateInput::new(
                InterfaceCheckedTemplateInputKind::Receiver,
                InterfaceTypeId::new(0),
            )],
            nodes,
            [InterfaceCheckedTemplateTemporary::new(
                CheckedTemplateNodeId::new(4),
                InterfaceTypeId::new(0),
                crate::InterfaceDependencyContractId::new(0),
            )],
            CheckedTemplateNodeId::new(10),
            behavior,
        );

        let types = [
            InterfaceType::TypeParameter(local(0)),
            InterfaceType::Tuple([InterfaceTypeId::new(0), InterfaceTypeId::new(0)].into()),
            InterfaceType::Array {
                element: InterfaceTypeId::new(0),
                length: crate::InterfaceConstantTermId::new(0),
            },
        ];

        let support = [
            InterfaceSupportEntity::Declaration(helper_key()),
            InterfaceSupportEntity::Implementation(InterfaceSupportImplementation::new(
                implementation_key(),
                InterfaceTypeId::new(0),
                None,
            )),
            InterfaceSupportEntity::CheckedTemplate(InterfaceCheckedTemplateId::new(0)),
        ];

        InterfaceSemanticFacts::new()
            .with_values(
                [InterfaceDependencyContract::new([])],
                types,
                [InterfaceConstantValue::new(
                    InterfaceTypeId::new(0),
                    InterfaceConstantValueKind::Unit,
                )],
                [InterfaceConstantTerm::Value(InterfaceConstantValueId::new(
                    0,
                ))],
            )
            .with_templates(
                [template],
                [InterfaceDeclarationTemplate::new(
                    local(0),
                    CheckedTemplateKind::RuntimeDefault,
                    SymbolOrdinal::new(0),
                    InterfaceSupportEntityId::new(2),
                )],
                support,
            )
    }

    fn local(index: usize) -> InterfaceSymbolReference {
        InterfaceSymbolReference::Local(InterfaceSymbolId::new(index_u32(index)))
    }

    fn helper_key() -> ExternalSymbolKey {
        let Some(name) = SymbolName::try_new("helper") else {
            panic!("test helper name must be valid");
        };

        let Some(key) = ExternalSymbolKey::named(module_key(), SymbolKind::Function, name) else {
            panic!("function support key must be valid");
        };

        key
    }

    fn implementation_key() -> ExternalSymbolKey {
        let Some(key) = ExternalSymbolKey::ordinal(
            module_key(),
            SymbolKind::InherentImplementation,
            SymbolOrdinal::new(0),
        ) else {
            panic!("implementation support key must be valid");
        };

        key
    }

    fn module_key() -> ExternalSymbolKey {
        let Some(package) = PackageIdentity::try_new("example.templates") else {
            panic!("test package identity must be valid");
        };

        let Some(path) = ModulePathKey::try_new(["templates"]) else {
            panic!("test module path must be valid");
        };

        let Some(module) = ExternalSymbolKey::module(ExternalSymbolKey::package(package), path)
        else {
            panic!("test module key must be valid");
        };

        module
    }

    fn owned_sections(
        sections: &[EncodedSemanticSection],
    ) -> Vec<(InterfaceSectionTag, u64, Vec<u8>)> {
        sections
            .iter()
            .map(|section| {
                (
                    section.tag(),
                    section.record_count(),
                    section.payload().to_vec(),
                )
            })
            .collect()
    }

    fn section_views(
        sections: &[(InterfaceSectionTag, u64, Vec<u8>)],
    ) -> Vec<ValidatedInterfaceSection<'_>> {
        sections
            .iter()
            .map(|(tag, count, payload)| ValidatedInterfaceSection::for_test(*tag, *count, payload))
            .collect()
    }

    fn index_u32(index: usize) -> u32 {
        match u32::try_from(index) {
            Ok(index) => index,
            Err(_) => panic!("small test index must fit in u32"),
        }
    }
}
