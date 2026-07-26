use bray_bound_tree::{CheckedTemplateInputId, CheckedTemplateNodeId, CheckedTemplateTemporaryId};
use bray_symbols::{InterfaceSupportEntityId, SymbolOrdinal};

use super::common::{decode_tag, validate_record_count};
use crate::semantic::codec::common::{
    SemanticDecodeContext, map_wire_error, read_count, read_symbol_reference,
    read_symbol_references, read_u32,
};
use crate::semantic::codec::record::RecordTable;
use crate::semantic::model::{
    InterfaceCheckedTemplate, InterfaceCheckedTemplateBehavior, InterfaceCheckedTemplateExecution,
    InterfaceCheckedTemplateInput, InterfaceCheckedTemplateInputKind, InterfaceCheckedTemplateNode,
    InterfaceCheckedTemplateOperation, InterfaceCheckedTemplateTemporary, InterfaceConstantTermId,
    InterfaceDeclarationTemplate, InterfaceDependencyContractId, InterfaceImplementationReference,
    InterfaceSemanticFacts, InterfaceTemplateReference, InterfaceTypeId,
};
use crate::wire::WireReader;
use crate::{
    InterfaceLimit, InterfaceValidationError, InterfaceValidationLimits, ValidatedInterfaceSection,
};

pub(super) struct TemplateRecordTables<'bytes> {
    pub(super) checked_templates: RecordTable<'bytes>,
    pub(super) declaration_templates: RecordTable<'bytes>,
}

pub(super) fn decode_template_tables<'bytes>(
    section: ValidatedInterfaceSection<'bytes>,
    context: &mut SemanticDecodeContext,
) -> Result<TemplateRecordTables<'bytes>, InterfaceValidationError> {
    let mut reader = WireReader::new(section.bytes());
    let format_version = read_u32(&mut reader)?;

    if format_version != super::super::DECLARATION_TEMPLATE_FORMAT_VERSION {
        return Err(InterfaceValidationError::Malformed);
    }

    let checked_templates = RecordTable::read_from(&mut reader, context)?;
    let declaration_templates = RecordTable::read_from(&mut reader, context)?;

    validate_record_count(
        section,
        [checked_templates.len(), declaration_templates.len()],
    )?;

    reader.finish().map_err(map_wire_error)?;

    Ok(TemplateRecordTables {
        checked_templates,
        declaration_templates,
    })
}

pub(super) fn decode_templates(
    section: ValidatedInterfaceSection<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
    facts: &mut InterfaceSemanticFacts,
) -> Result<(), InterfaceValidationError> {
    let tables = decode_template_tables(section, context)?;

    let templates = tables
        .checked_templates
        .decode_all(context, |reader, context| {
            decode_template(reader, limits, context)
        })?;

    let declarations = tables
        .declaration_templates
        .decode_all(context, decode_declaration_template)?;

    facts.checked_templates = templates.into();
    facts.declaration_templates = declarations.into();

    Ok(())
}

pub(crate) fn decode_template_payload(
    bytes: &[u8],
    limits: InterfaceValidationLimits,
) -> Result<InterfaceCheckedTemplate, InterfaceValidationError> {
    let mut reader = WireReader::new(bytes);
    let mut context = SemanticDecodeContext::new(limits);
    let template = decode_template(&mut reader, limits, &mut context)?;

    reader.finish().map_err(map_wire_error)?;

    Ok(template)
}

fn decode_template(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
) -> Result<InterfaceCheckedTemplate, InterfaceValidationError> {
    let kind = decode_tag(read_u32(reader)?)?;
    let input_count = read_count(reader, limits, InterfaceLimit::TemplateGraphSize)?;
    let mut graph_size = input_count;
    let mut inputs = context.allocate_items(reader, input_count)?;

    for _ in 0..input_count {
        inputs.push(InterfaceCheckedTemplateInput::new(
            decode_input_kind(reader, context)?,
            InterfaceTypeId::new(read_u32(reader)?),
        ));
    }

    let node_count = read_count(reader, limits, InterfaceLimit::TemplateGraphSize)?;

    graph_size = add_graph_count(graph_size, node_count, limits)?;

    let mut nodes = context.allocate_items(reader, node_count)?;

    for _ in 0..node_count {
        nodes.push(InterfaceCheckedTemplateNode::new(
            decode_operation(reader, limits, context)?,
            InterfaceTypeId::new(read_u32(reader)?),
        ));
    }

    let temporary_count = read_count(reader, limits, InterfaceLimit::TemplateGraphSize)?;

    add_graph_count(graph_size, temporary_count, limits)?;

    let mut temporaries = context.allocate_items(reader, temporary_count)?;

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
    let execution_requirements = read_symbol_references(reader, limits, context)?;
    let lifecycle_count = read_count(reader, limits, InterfaceLimit::RecordCount)?;

    let mut lifecycle_obligations = context.allocate_items(reader, lifecycle_count)?;

    for _ in 0..lifecycle_count {
        lifecycle_obligations.push(decode_tag(read_u32(reader)?)?);
    }

    let dependency_contract = InterfaceDependencyContractId::new(read_u32(reader)?);
    let current_run_cancellation = decode_tag(read_u32(reader)?)?;

    let witness_count = read_count(reader, limits, InterfaceLimit::RecordCount)?;
    let mut witnesses = context.allocate_items(reader, witness_count)?;

    for _ in 0..witness_count {
        witnesses.push(decode_implementation_reference(reader, context)?);
    }

    let behavior = InterfaceCheckedTemplateBehavior::new(
        effects,
        capabilities,
        trusted_obligations,
        InterfaceCheckedTemplateExecution::new(execution_requirements, current_run_cancellation),
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

pub(super) fn decode_declaration_template(
    reader: &mut WireReader<'_>,
    context: &mut SemanticDecodeContext,
) -> Result<InterfaceDeclarationTemplate, InterfaceValidationError> {
    Ok(InterfaceDeclarationTemplate::new(
        read_symbol_reference(reader, context)?,
        decode_tag(read_u32(reader)?)?,
        SymbolOrdinal::new(read_u32(reader)?),
        InterfaceSupportEntityId::new(read_u32(reader)?),
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
        2 => Ok(InterfaceCheckedTemplateOperation::Constant {
            term: InterfaceConstantTermId::new(read_u32(reader)?),
            usage: bray_bound_tree::CheckedTemplateConstantUsage::new(
                reader.read_u64().map_err(map_wire_error)?,
                reader.read_u64().map_err(map_wire_error)?,
                reader.read_u64().map_err(map_wire_error)?,
            ),
        }),
        3 => Ok(InterfaceCheckedTemplateOperation::Declaration(
            decode_template_reference(reader, context)?,
        )),
        4 => {
            let callable = decode_template_reference(reader, context)?;
            let substitution = crate::InterfaceGenericSubstitutionId::new(read_u32(reader)?);
            let arguments = read_node_ids(reader, limits, context)?;

            let implementation = match read_u32(reader)? {
                0 => None,
                1 => Some((
                    decode_implementation_reference(reader, context)?,
                    crate::InterfaceGenericSubstitutionId::new(read_u32(reader)?),
                )),
                _ => return Err(InterfaceValidationError::Malformed),
            };

            Ok(InterfaceCheckedTemplateOperation::call(
                callable,
                substitution,
                arguments,
                implementation,
            ))
        }
        5 => Ok(InterfaceCheckedTemplateOperation::Convert {
            value: CheckedTemplateNodeId::new(read_u32(reader)?),
            target: InterfaceTypeId::new(read_u32(reader)?),
        }),
        6 => Ok(InterfaceCheckedTemplateOperation::tuple(read_node_ids(
            reader, limits, context,
        )?)),
        7 => Ok(InterfaceCheckedTemplateOperation::array(read_node_ids(
            reader, limits, context,
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
        12 => Ok(InterfaceCheckedTemplateOperation::Unary {
            operation: decode_tag(read_u32(reader)?)?,
            operand: CheckedTemplateNodeId::new(read_u32(reader)?),
        }),
        13 => Ok(InterfaceCheckedTemplateOperation::Binary {
            operation: decode_tag(read_u32(reader)?)?,
            left: CheckedTemplateNodeId::new(read_u32(reader)?),
            right: CheckedTemplateNodeId::new(read_u32(reader)?),
        }),
        _ => Err(InterfaceValidationError::Malformed),
    }
}

fn read_node_ids(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
) -> Result<Vec<CheckedTemplateNodeId>, InterfaceValidationError> {
    let count = read_count(reader, limits, InterfaceLimit::TemplateGraphSize)?;
    let mut nodes = context.allocate_items(reader, count)?;

    for _ in 0..count {
        nodes.push(CheckedTemplateNodeId::new(read_u32(reader)?));
    }

    Ok(nodes)
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
        AnySymbolId, CallableParameterDefaultProviderSymbolId, ConstantSymbolId, ExternalSymbolKey,
        FunctionSymbolId, GenericConstParameterSymbolId, GenericTypeParameterSymbolId,
        InterfaceSupportEntityId, LifecycleObligationKind, ModuleSymbolId, PackageIdentity,
        PackageSymbolId, PredicateSymbolId, SemanticValueStore, StructSymbolId, SymbolId,
        SymbolKind, SymbolOrdinal, SynthesizedSymbolRole,
    };

    use super::super::decode_semantic_facts;
    use super::super::test_support::{
        interface_surface, key_by_kind as symbol_key, local_by_kind as symbol_reference,
        owned_section_views, owned_sections, record_range,
    };
    use crate::semantic::codec::encode_semantic_facts;
    use crate::test_support::{module_key as test_module_key, named_key};
    use crate::{
        InterfaceCheckedTemplate, InterfaceCheckedTemplateBehavior, InterfaceCheckedTemplateId,
        InterfaceCheckedTemplateInput, InterfaceCheckedTemplateInputKind,
        InterfaceCheckedTemplateNode, InterfaceCheckedTemplateOperation,
        InterfaceCheckedTemplateTemporary, InterfaceConstantTerm, InterfaceConstantValue,
        InterfaceConstantValueId, InterfaceConstantValueKind, InterfaceDeclarationTemplate,
        InterfaceDependencyContract, InterfaceImplementationReference,
        InterfacePredicateDefinition, InterfacePredicateDefinitionState, InterfaceSectionTag,
        InterfaceSemanticFacts, InterfaceSupportEntity, InterfaceSupportImplementation,
        InterfaceSymbolReference, InterfaceSymbolResolver, InterfaceTemplateReference,
        InterfaceType, InterfaceTypeId, InterfaceValidationError, InterfaceValidationLimits,
    };

    #[test]
    fn every_declaration_template_kind_round_trips_with_private_support() {
        let (surface, facts) = template_fixture();

        let limits = InterfaceValidationLimits::default();
        let sections = encode_semantic_facts(&facts, &surface, limits);

        let Ok(sections) = sections else {
            panic!("valid checked templates must encode");
        };

        let owned = owned_sections(&sections);
        let views = owned_section_views(&owned);
        let decoded = decode_semantic_facts(&views, &surface, limits);

        assert_eq!(decoded, Ok(facts));
    }

    #[test]
    fn declaration_templates_intern_to_ordinary_checked_templates() {
        let (surface, facts) = template_fixture();

        let resolver = resolver(&surface);

        let store = SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("semantic store creation failed: {error:?}"));

        let imported = facts
            .intern(&store, &resolver)
            .unwrap_or_else(|error| panic!("template interning failed: {error:?}"));

        assert_eq!(imported.declaration_templates().len(), 5);
        assert_eq!(imported.predicate_definitions().len(), 1);

        assert_eq!(
            imported.predicate_definitions()[0].state(),
            InterfacePredicateDefinitionState::Defined
        );

        for template in imported.declaration_templates() {
            assert_eq!(template.kind(), template.template().kind());
            assert_eq!(template.ordinal(), SymbolOrdinal::new(0));
        }
    }

    #[test]
    fn every_checked_template_operation_round_trips() {
        let (surface, facts) = operation_fixture();

        let limits = InterfaceValidationLimits::default();
        let sections = encode_semantic_facts(&facts, &surface, limits);

        let Ok(sections) = sections else {
            panic!("valid checked-template operations must encode");
        };

        let owned = owned_sections(&sections);

        assert_eq!(
            decode_semantic_facts(&owned_section_views(&owned), &surface, limits),
            Ok(facts)
        );
    }

    #[test]
    fn template_validation_rejects_forward_nodes_and_wrong_support_categories() {
        let (surface, facts) = template_fixture();

        let behavior = InterfaceCheckedTemplateBehavior::new(
            [],
            [],
            [],
            crate::InterfaceCheckedTemplateExecution::new(
                [],
                bray_symbols::CurrentRunCancellation::NotEntered,
            ),
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
                symbol_reference(&surface, SymbolKind::CallableParameterDefaultProvider),
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
                &surface,
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
                &surface,
                InterfaceValidationLimits::default()
            ),
            Err(InterfaceValidationError::Malformed)
        );
    }

    #[test]
    fn template_decode_rejects_unknown_kinds_and_graph_limit_excess() {
        let (surface, facts) = template_fixture();

        let limits = InterfaceValidationLimits::default();
        let sections = encode_semantic_facts(&facts, &surface, limits);

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

        let kind = record_range(&templates[4..], 0, 0);
        let kind_start = kind.start + 4;

        templates[kind_start..kind_start + 4].copy_from_slice(&u32::MAX.to_le_bytes());

        assert_eq!(
            decode_semantic_facts(&owned_section_views(&owned), &surface, limits),
            Err(InterfaceValidationError::Malformed)
        );

        let constrained = limits.with_template_graph_size(1);

        assert!(matches!(
            encode_semantic_facts(&facts, &surface, constrained),
            Err(InterfaceValidationError::ResourceLimitExceeded {
                limit: crate::InterfaceLimit::TemplateGraphSize,
                ..
            })
        ));
    }

    #[test]
    fn template_validation_rejects_invalid_owners_and_input_identities() {
        let (surface, facts) = template_fixture();

        let limits = InterfaceValidationLimits::default();
        let mut declarations = facts.declaration_templates().to_vec();

        let runtime = declarations
            .iter_mut()
            .find(|declaration| declaration.kind() == CheckedTemplateKind::RuntimeDefault)
            .unwrap_or_else(|| panic!("runtime default template declaration must be present"));

        *runtime = InterfaceDeclarationTemplate::new(
            symbol_reference(&surface, SymbolKind::Module),
            runtime.kind(),
            runtime.ordinal(),
            runtime.entity(),
        );

        declarations.sort();

        let invalid_owner = facts.clone().with_templates(
            facts.checked_templates().iter().cloned(),
            declarations,
            facts.support_entities().iter().cloned(),
        );

        assert_eq!(
            encode_semantic_facts(&invalid_owner, &surface, limits),
            Err(InterfaceValidationError::Malformed)
        );

        let mut templates = facts.checked_templates().to_vec();
        let template = templates[0].clone();
        let input = template.inputs()[0].clone();

        templates[0] = InterfaceCheckedTemplate::new(
            template.kind(),
            [input.clone(), input],
            template.nodes().iter().cloned(),
            template.temporaries().iter().copied(),
            template.result(),
            template.behavior().clone(),
        );

        let duplicate_input = facts.clone().with_templates(
            templates,
            facts.declaration_templates().iter().cloned(),
            facts.support_entities().iter().cloned(),
        );

        assert_eq!(
            encode_semantic_facts(&duplicate_input, &surface, limits),
            Err(InterfaceValidationError::Malformed)
        );

        let mut templates = facts.checked_templates().to_vec();
        let template = templates[0].clone();

        templates[0] = InterfaceCheckedTemplate::new(
            template.kind(),
            [InterfaceCheckedTemplateInput::new(
                InterfaceCheckedTemplateInputKind::GenericType(symbol_reference(
                    &surface,
                    SymbolKind::GenericConstParameter,
                )),
                template.inputs()[0].ty(),
            )],
            template.nodes().iter().cloned(),
            template.temporaries().iter().copied(),
            template.result(),
            template.behavior().clone(),
        );

        let wrong_generic_category = facts.clone().with_templates(
            templates,
            facts.declaration_templates().iter().cloned(),
            facts.support_entities().iter().cloned(),
        );

        assert_eq!(
            encode_semantic_facts(&wrong_generic_category, &surface, limits),
            Err(InterfaceValidationError::Malformed)
        );
    }

    #[test]
    fn template_validation_rejects_calls_to_noncallable_declarations() {
        let (surface, facts) = operation_fixture();

        let mut templates = facts.checked_templates().to_vec();
        let template = templates[0].clone();
        let mut nodes = template.nodes().to_vec();

        nodes[3] = InterfaceCheckedTemplateNode::new(
            InterfaceCheckedTemplateOperation::call(
                InterfaceTemplateReference::Symbol(symbol_reference(
                    &surface,
                    SymbolKind::Constant,
                )),
                crate::InterfaceGenericSubstitutionId::new(0),
                [CheckedTemplateNodeId::new(0), CheckedTemplateNodeId::new(1)],
                None,
            ),
            InterfaceTypeId::new(0),
        );

        templates[0] = InterfaceCheckedTemplate::new(
            template.kind(),
            template.inputs().iter().cloned(),
            nodes,
            template.temporaries().iter().copied(),
            template.result(),
            template.behavior().clone(),
        );

        let invalid = facts.clone().with_templates(
            templates,
            facts.declaration_templates().iter().cloned(),
            facts.support_entities().iter().cloned(),
        );

        assert_eq!(
            encode_semantic_facts(&invalid, &surface, InterfaceValidationLimits::default()),
            Err(InterfaceValidationError::Malformed)
        );
    }

    #[test]
    fn support_validation_rejects_foreign_and_exported_keys() {
        let (surface, facts) = template_fixture();

        let limits = InterfaceValidationLimits::default();
        let templates = facts.checked_templates().to_vec();
        let declarations = facts.declaration_templates().to_vec();
        let mut entities = facts.support_entities().to_vec();

        let foreign_package = PackageIdentity::try_new("foreign.templates")
            .unwrap_or_else(|| panic!("foreign test package identity must be valid"));

        let foreign_module = test_module_key(foreign_package, "templates");

        entities[0] = InterfaceSupportEntity::Declaration(named_key(
            foreign_module,
            SymbolKind::Function,
            "helper",
        ));

        let foreign_support =
            facts
                .clone()
                .with_templates(templates.clone(), declarations.clone(), entities.clone());

        assert_eq!(
            encode_semantic_facts(&foreign_support, &surface, limits),
            Err(InterfaceValidationError::Malformed)
        );

        entities[0] =
            InterfaceSupportEntity::Declaration(symbol_key(&surface, SymbolKind::Function));

        let exported_alias = facts.with_templates(templates, declarations, entities);

        assert_eq!(
            encode_semantic_facts(&exported_alias, &surface, limits),
            Err(InterfaceValidationError::Malformed)
        );
    }

    #[test]
    fn template_decode_rejects_impossible_table_counts_before_reserving() {
        let (surface, facts) = operation_fixture();

        let limits = InterfaceValidationLimits::default();

        let sections = encode_semantic_facts(&facts, &surface, limits)
            .unwrap_or_else(|error| panic!("valid checked templates must encode: {error:?}"));

        let mut owned = owned_sections(&sections);

        let (_, record_count, templates) = owned
            .iter_mut()
            .find(|(tag, _, _)| *tag == InterfaceSectionTag::DeclarationTemplates)
            .unwrap_or_else(|| panic!("template section must be encoded"));

        templates[4..8].copy_from_slice(&10_000_000_u32.to_le_bytes());
        *record_count = 10_000_001;

        let decoded = decode_semantic_facts(&owned_section_views(&owned), &surface, limits);

        assert_eq!(decoded, Err(InterfaceValidationError::Truncated));
    }

    fn template_fixture() -> (crate::PackageInterfaceSurface, InterfaceSemanticFacts) {
        let surface = template_surface();
        let facts = template_facts(&surface);

        (surface, facts)
    }

    fn template_facts(surface: &crate::PackageInterfaceSurface) -> InterfaceSemanticFacts {
        let kinds = [
            CheckedTemplateKind::RuntimeDefault,
            CheckedTemplateKind::ConstantDefinition,
            CheckedTemplateKind::PredicateDefinition,
            CheckedTemplateKind::GenericConstraint,
            CheckedTemplateKind::CallableContract,
        ];

        let generic_type = symbol_reference(surface, SymbolKind::GenericTypeParameter);
        let generic_constant = symbol_reference(surface, SymbolKind::GenericConstParameter);

        let owners = [
            symbol_reference(surface, SymbolKind::CallableParameterDefaultProvider),
            symbol_reference(surface, SymbolKind::Constant),
            symbol_reference(surface, SymbolKind::Predicate),
            symbol_reference(surface, SymbolKind::Struct),
            symbol_reference(surface, SymbolKind::Function),
        ];

        let templates = kinds.into_iter().enumerate().map(|(index, kind)| {
            let input_kind = match index {
                0 => InterfaceCheckedTemplateInputKind::GenericType(generic_type.clone()),
                1 => InterfaceCheckedTemplateInputKind::GenericConstant(generic_constant.clone()),
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
                crate::InterfaceCheckedTemplateExecution::new(
                    [],
                    bray_symbols::CurrentRunCancellation::NotEntered,
                ),
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

        let mut declarations: Vec<_> = kinds
            .into_iter()
            .enumerate()
            .map(|(index, kind)| {
                InterfaceDeclarationTemplate::new(
                    owners[index].clone(),
                    kind,
                    SymbolOrdinal::new(0),
                    InterfaceSupportEntityId::new(index_u32(index + 2)),
                )
            })
            .collect();

        declarations.sort();

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
                [InterfaceType::TypeParameter(generic_type.clone())],
                [],
                [],
            )
            .with_declarations(
                [],
                [],
                [],
                [InterfacePredicateDefinition::new(
                    symbol_reference(surface, SymbolKind::Predicate),
                    InterfacePredicateDefinitionState::Defined,
                )],
            )
            .with_templates(templates, declarations, support)
    }

    fn operation_fixture() -> (crate::PackageInterfaceSurface, InterfaceSemanticFacts) {
        let surface = template_surface();
        let facts = operation_facts(&surface);

        (surface, facts)
    }

    fn operation_facts(surface: &crate::PackageInterfaceSurface) -> InterfaceSemanticFacts {
        let declaration = InterfaceTemplateReference::Support(InterfaceSupportEntityId::new(0));

        let callable =
            InterfaceTemplateReference::Symbol(symbol_reference(surface, SymbolKind::Function));

        let implementation =
            InterfaceImplementationReference::Support(InterfaceSupportEntityId::new(1));

        let nodes = [
            InterfaceCheckedTemplateNode::new(
                InterfaceCheckedTemplateOperation::Input(CheckedTemplateInputId::new(0)),
                InterfaceTypeId::new(0),
            ),
            InterfaceCheckedTemplateNode::new(
                InterfaceCheckedTemplateOperation::Constant {
                    term: crate::InterfaceConstantTermId::new(0),
                    usage: bray_bound_tree::CheckedTemplateConstantUsage::new(2, 3, 4),
                },
                InterfaceTypeId::new(0),
            ),
            InterfaceCheckedTemplateNode::new(
                InterfaceCheckedTemplateOperation::Declaration(declaration.clone()),
                InterfaceTypeId::new(0),
            ),
            InterfaceCheckedTemplateNode::new(
                InterfaceCheckedTemplateOperation::call(
                    callable,
                    crate::InterfaceGenericSubstitutionId::new(0),
                    [CheckedTemplateNodeId::new(0), CheckedTemplateNodeId::new(1)],
                    None,
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
            InterfaceCheckedTemplateNode::new(
                InterfaceCheckedTemplateOperation::Unary {
                    operation: bray_symbols::ConstantUnaryOperation::Identity,
                    operand: CheckedTemplateNodeId::new(0),
                },
                InterfaceTypeId::new(0),
            ),
            InterfaceCheckedTemplateNode::new(
                InterfaceCheckedTemplateOperation::Binary {
                    operation: bray_symbols::ConstantBinaryOperation::Add,
                    left: CheckedTemplateNodeId::new(0),
                    right: CheckedTemplateNodeId::new(1),
                },
                InterfaceTypeId::new(0),
            ),
        ];

        let behavior = InterfaceCheckedTemplateBehavior::new(
            [symbol_reference(surface, SymbolKind::Function)],
            [symbol_reference(surface, SymbolKind::Function)],
            [symbol_reference(surface, SymbolKind::Function)],
            crate::InterfaceCheckedTemplateExecution::new(
                [symbol_reference(surface, SymbolKind::Function)],
                bray_symbols::CurrentRunCancellation::MayEnter,
            ),
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
            CheckedTemplateNodeId::new(12),
            behavior,
        );

        let types = [
            InterfaceType::TypeParameter(symbol_reference(
                surface,
                SymbolKind::GenericTypeParameter,
            )),
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
            .with_applications(
                [crate::InterfaceGenericSubstitution::new(
                    symbol_reference(surface, SymbolKind::Function),
                    [],
                )],
                [],
                [],
                [],
            )
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
                    symbol_reference(surface, SymbolKind::CallableParameterDefaultProvider),
                    CheckedTemplateKind::RuntimeDefault,
                    SymbolOrdinal::new(0),
                    InterfaceSupportEntityId::new(2),
                )],
                support,
            )
    }

    fn template_surface() -> crate::PackageInterfaceSurface {
        let module = test_module_key(package_identity(), "templates");
        let function = named_key(module.clone(), SymbolKind::Function, "run");
        let structure = named_key(module.clone(), SymbolKind::Struct, "record");

        let runtime_provider = ExternalSymbolKey::synthesized(
            function.clone(),
            SynthesizedSymbolRole::CallableParameterDefaultProvider,
            Some(SymbolOrdinal::new(0)),
        )
        .unwrap_or_else(|| panic!("test runtime default provider key must be valid"));

        let generic_type = ExternalSymbolKey::ordinal(
            structure.clone(),
            SymbolKind::GenericTypeParameter,
            SymbolOrdinal::new(0),
        )
        .unwrap_or_else(|| panic!("test generic type parameter key must be valid"));

        let generic_constant = ExternalSymbolKey::ordinal(
            structure.clone(),
            SymbolKind::GenericConstParameter,
            SymbolOrdinal::new(1),
        )
        .unwrap_or_else(|| panic!("test generic constant parameter key must be valid"));

        let symbols = [
            function,
            structure,
            runtime_provider,
            generic_type,
            generic_constant,
            named_key(module.clone(), SymbolKind::Constant, "answer"),
            named_key(module, SymbolKind::Predicate, "valid"),
        ];

        interface_surface(package_identity(), symbols, [])
    }

    struct Resolver {
        symbols: Vec<AnySymbolId>,
        keys: Vec<ExternalSymbolKey>,
    }

    impl InterfaceSymbolResolver for Resolver {
        fn resolve(&self, reference: &InterfaceSymbolReference) -> Option<AnySymbolId> {
            let InterfaceSymbolReference::Local(symbol) = reference else {
                return None;
            };

            self.symbols.get(symbol.to_index()?).copied()
        }

        fn external_key(&self, reference: &InterfaceSymbolReference) -> Option<ExternalSymbolKey> {
            let InterfaceSymbolReference::Local(symbol) = reference else {
                return None;
            };

            self.keys.get(symbol.to_index()?).cloned()
        }
    }

    fn resolver(surface: &crate::PackageInterfaceSurface) -> Resolver {
        let keys = surface
            .symbols()
            .symbols()
            .iter()
            .map(|identity| identity.key().clone())
            .collect();

        let symbols = surface
            .symbols()
            .symbols()
            .iter()
            .map(|identity| {
                let id = SymbolId::new(identity.id().raw());

                match identity.kind() {
                    SymbolKind::Package => PackageSymbolId::from_symbol_id(id).into(),
                    SymbolKind::Module => ModuleSymbolId::from_symbol_id(id).into(),
                    SymbolKind::Function => FunctionSymbolId::from_symbol_id(id).into(),
                    SymbolKind::Struct => StructSymbolId::from_symbol_id(id).into(),
                    SymbolKind::CallableParameterDefaultProvider => {
                        CallableParameterDefaultProviderSymbolId::from_symbol_id(id).into()
                    }
                    SymbolKind::GenericTypeParameter => {
                        GenericTypeParameterSymbolId::from_symbol_id(id).into()
                    }
                    SymbolKind::GenericConstParameter => {
                        GenericConstParameterSymbolId::from_symbol_id(id).into()
                    }
                    SymbolKind::Constant => ConstantSymbolId::from_symbol_id(id).into(),
                    SymbolKind::Predicate => PredicateSymbolId::from_symbol_id(id).into(),
                    kind => panic!("unexpected test symbol kind: {kind:?}"),
                }
            })
            .collect();

        Resolver { symbols, keys }
    }

    fn helper_key() -> ExternalSymbolKey {
        named_key(
            test_module_key(package_identity(), "templates"),
            SymbolKind::Function,
            "helper",
        )
    }

    fn implementation_key() -> ExternalSymbolKey {
        let Some(key) = ExternalSymbolKey::ordinal(
            test_module_key(package_identity(), "templates"),
            SymbolKind::InherentImplementation,
            SymbolOrdinal::new(0),
        ) else {
            panic!("implementation support key must be valid");
        };

        key
    }

    fn package_identity() -> PackageIdentity {
        PackageIdentity::try_new("example.templates")
            .unwrap_or_else(|| panic!("test package identity must be valid"))
    }

    fn index_u32(index: usize) -> u32 {
        match u32::try_from(index) {
            Ok(index) => index,
            Err(_) => panic!("small test index must fit in u32"),
        }
    }
}
