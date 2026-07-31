use super::{contract, declaration, directory, selection, support, surface, template, value};

use crate::semantic::codec::common::SemanticDecodeContext;
use crate::{
    InterfaceSectionTag, InterfaceSemanticFacts, InterfaceValidationError,
    InterfaceValidationLimits, PackageInterfaceSurface, ValidatedInterfaceSection,
};

/// Decodes and validates every semantic fact section in one package interface.
pub fn decode_semantic_facts(
    sections: &[ValidatedInterfaceSection<'_>],
    surface: &PackageInterfaceSurface,
    limits: InterfaceValidationLimits,
) -> Result<InterfaceSemanticFacts, InterfaceValidationError> {
    validate_decode_allocation(sections, limits)?;

    let mut context = SemanticDecodeContext::new(limits);

    let directory = required_section(sections, InterfaceSectionTag::SymbolFactDirectory)?;
    let types = required_section(sections, InterfaceSectionTag::SemanticTypes)?;
    let constants = required_section(sections, InterfaceSectionTag::Constants)?;
    let contracts = required_section(sections, InterfaceSectionTag::Contracts)?;
    let declarations = required_section(sections, InterfaceSectionTag::DeclarationFacts)?;
    let templates = required_section(sections, InterfaceSectionTag::DeclarationTemplates)?;
    let implementations = required_section(sections, InterfaceSectionTag::Implementations)?;
    let targets = required_section(sections, InterfaceSectionTag::TargetDependencies)?;
    let provenance = optional_section(sections, InterfaceSectionTag::SourceProvenance);
    let support = required_section(sections, InterfaceSectionTag::SupportGraph)?;

    let mut facts = value::decode_types(types, limits, &mut context)?;

    value::decode_constants(constants, limits, &mut context, &mut facts)?;
    contract::decode_contracts(contracts, limits, &mut context, &mut facts)?;
    declaration::decode_declarations(declarations, limits, &mut context, &mut facts)?;
    template::decode_templates(templates, limits, &mut context, &mut facts)?;
    surface::decode_implementations(implementations, limits, &mut context, &mut facts)?;
    surface::decode_target_dependencies(targets, limits, &mut context, &mut facts)?;
    support::decode_support_graph(support, limits, &mut context, &mut facts)?;

    if let Some(provenance) = provenance {
        surface::decode_provenance(provenance, limits, &mut context, &mut facts)?;
    }

    let encoded_directory = directory::decode_fact_directory(directory, limits, &mut context)?;

    if encoded_directory.as_ref() != facts.fact_directory().as_ref() {
        return Err(InterfaceValidationError::Malformed);
    }

    facts.validate(surface, limits)?;

    Ok(facts)
}

pub(crate) fn decode_semantic_fact_graph(
    sections: &[ValidatedInterfaceSection<'_>],
    surface: &PackageInterfaceSurface,
    owner: bray_symbols::InterfaceSymbolId,
    kind: crate::InterfaceSemanticFactKind,
    limits: InterfaceValidationLimits,
) -> Result<InterfaceSemanticFacts, InterfaceValidationError> {
    let Some(required_tags) = selected_fact_sections(kind) else {
        return decode_semantic_facts(sections, surface, limits);
    };

    let selected_sections = sections
        .iter()
        .copied()
        .filter(|section| required_tags.contains(&section.tag()))
        .collect::<Vec<_>>();

    validate_decode_allocation(&selected_sections, limits)?;

    selection::decode_selected_fact_graph(sections, surface, owner, kind, limits)
}

fn selected_fact_sections(
    kind: crate::InterfaceSemanticFactKind,
) -> Option<&'static [InterfaceSectionTag]> {
    const GENERIC_CONSTRAINT_SECTIONS: &[InterfaceSectionTag] = &[
        InterfaceSectionTag::SymbolFactDirectory,
        InterfaceSectionTag::SemanticTypes,
        InterfaceSectionTag::Constants,
        InterfaceSectionTag::Contracts,
    ];

    const CALLABLE_SIGNATURE_SECTIONS: &[InterfaceSectionTag] = &[
        InterfaceSectionTag::SymbolFactDirectory,
        InterfaceSectionTag::SemanticTypes,
        InterfaceSectionTag::Constants,
        InterfaceSectionTag::Contracts,
        InterfaceSectionTag::DeclarationFacts,
    ];

    const GENERIC_DECLARATION_SECTIONS: &[InterfaceSectionTag] = CALLABLE_SIGNATURE_SECTIONS;

    const CALLABLE_PARAMETER_DEFAULT_SECTIONS: &[InterfaceSectionTag] = &[
        InterfaceSectionTag::SymbolFactDirectory,
        InterfaceSectionTag::DeclarationFacts,
    ];

    const PREDICATE_DEFINITION_SECTIONS: &[InterfaceSectionTag] = &[
        InterfaceSectionTag::SymbolFactDirectory,
        InterfaceSectionTag::DeclarationFacts,
        InterfaceSectionTag::DeclarationTemplates,
    ];

    const IMPLEMENTATION_SECTIONS: &[InterfaceSectionTag] = &[
        InterfaceSectionTag::SymbolFactDirectory,
        InterfaceSectionTag::SemanticTypes,
        InterfaceSectionTag::Constants,
        InterfaceSectionTag::Contracts,
        InterfaceSectionTag::Implementations,
        InterfaceSectionTag::TargetDependencies,
    ];

    const TARGET_FACT_SECTIONS: &[InterfaceSectionTag] = &[
        InterfaceSectionTag::SymbolFactDirectory,
        InterfaceSectionTag::SemanticTypes,
        InterfaceSectionTag::Constants,
        InterfaceSectionTag::TargetDependencies,
    ];

    match kind {
        crate::InterfaceSemanticFactKind::CallableSignature => Some(CALLABLE_SIGNATURE_SECTIONS),
        crate::InterfaceSemanticFactKind::GenericDeclaration => Some(GENERIC_DECLARATION_SECTIONS),
        crate::InterfaceSemanticFactKind::CallableParameterDefault => {
            Some(CALLABLE_PARAMETER_DEFAULT_SECTIONS)
        }
        crate::InterfaceSemanticFactKind::PredicateDefinition => {
            Some(PREDICATE_DEFINITION_SECTIONS)
        }
        crate::InterfaceSemanticFactKind::TypeRepresentation => None,
        crate::InterfaceSemanticFactKind::GenericConstraint => Some(GENERIC_CONSTRAINT_SECTIONS),
        crate::InterfaceSemanticFactKind::Implementation => Some(IMPLEMENTATION_SECTIONS),
        crate::InterfaceSemanticFactKind::TargetFact => Some(TARGET_FACT_SECTIONS),
        crate::InterfaceSemanticFactKind::Runtime => Some(TARGET_FACT_SECTIONS),
        crate::InterfaceSemanticFactKind::CallableContracts
        | crate::InterfaceSemanticFactKind::DeclarationTemplate
        | crate::InterfaceSemanticFactKind::Abi => None,
    }
}

pub(crate) fn validate_decode_allocation(
    sections: &[ValidatedInterfaceSection<'_>],
    limits: InterfaceValidationLimits,
) -> Result<(), InterfaceValidationError> {
    // Every decoded semantic allocation is backed by at least one wire scalar or payload byte.
    // This conservative envelope bounds aggregate allocation before any semantic tables are built.
    const MAXIMUM_WIRE_EXPANSION: u64 = 16;

    let semantic_bytes = sections
        .iter()
        .filter(|section| {
            matches!(
                section.tag(),
                InterfaceSectionTag::SymbolFactDirectory
                    | InterfaceSectionTag::SemanticTypes
                    | InterfaceSectionTag::Constants
                    | InterfaceSectionTag::Contracts
                    | InterfaceSectionTag::DeclarationFacts
                    | InterfaceSectionTag::DeclarationTemplates
                    | InterfaceSectionTag::Implementations
                    | InterfaceSectionTag::TargetDependencies
                    | InterfaceSectionTag::SourceProvenance
                    | InterfaceSectionTag::SupportGraph
            )
        })
        .try_fold(0_u64, |total, section| {
            let length = u64::try_from(section.bytes().len()).unwrap_or(u64::MAX);

            total.checked_add(length)
        })
        .ok_or(InterfaceValidationError::ResourceLimitExceeded {
            limit: crate::InterfaceLimit::DecodedAllocation,
            actual: u64::MAX,
            maximum: limits.maximum(crate::InterfaceLimit::DecodedAllocation),
        })?;

    let allocation = semantic_bytes.saturating_mul(MAXIMUM_WIRE_EXPANSION);

    limits.check(crate::InterfaceLimit::DecodedAllocation, allocation)
}

pub(super) fn required_section<'bytes>(
    sections: &'bytes [ValidatedInterfaceSection<'bytes>],
    tag: InterfaceSectionTag,
) -> Result<ValidatedInterfaceSection<'bytes>, InterfaceValidationError> {
    optional_section(sections, tag).ok_or(InterfaceValidationError::Malformed)
}

fn optional_section<'bytes>(
    sections: &'bytes [ValidatedInterfaceSection<'bytes>],
    tag: InterfaceSectionTag,
) -> Option<ValidatedInterfaceSection<'bytes>> {
    sections
        .iter()
        .copied()
        .find(|section| section.tag() == tag)
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::CheckedTemplateKind;
    use bray_runtime_interface::{
        ExecutionLaneRequirement, PanicAbiIdentity, ProtectedAsyncFrameId,
        ProtectedFrameAbiVersions, RuntimeAbiRole, RuntimeAbiVersion, RuntimeCapability,
        RuntimeIdentity, RuntimeRequirements,
    };
    use bray_symbols::{InterfaceSymbolId, SymbolKind, SymbolOrdinal};
    use bray_target::TargetIdentity;

    use super::super::test_support::{
        OwnedSection, append_record, owned_section_views, record_directory_entry, record_range,
        record_range_with_local_owner,
    };
    use super::{decode_semantic_fact_graph, decode_semantic_facts};
    use crate::semantic::codec::{encode_semantic_facts, encode_validated_semantic_facts};
    use crate::test_support::{local_by_kind, package_interface_export_bundle};
    use crate::{
        InterfaceCallableParameterDefault, InterfaceCallableSignature,
        InterfaceDeclarationTemplate, InterfaceDependencyContract, InterfaceDependencyRequirement,
        InterfaceDependencyRequirementKind, InterfaceDependencySubject,
        InterfaceDependencySubjectRoot, InterfaceGenericDeclaration,
        InterfacePredicateDefinitionState, InterfaceRuntimeRequirement, InterfaceSectionTag,
        InterfaceSemanticFactKind, InterfaceSemanticFacts, InterfaceSymbolReference,
        InterfaceTypeId, InterfaceValidationError, InterfaceValidationLimits,
        PackageInterfaceSurface,
    };

    #[test]
    fn implementation_fact_decoding_ignores_unrelated_template_bytes() {
        let (surface, owner, mut sections) = implementation_fixture();

        section_mut(&mut sections, InterfaceSectionTag::DeclarationTemplates)
            .2
            .clear();

        let decoded = decode_semantic_fact_graph(
            &owned_section_views(&sections),
            &surface,
            owner,
            InterfaceSemanticFactKind::Implementation,
            InterfaceValidationLimits::default(),
        )
        .unwrap_or_else(|error| panic!("implementation fact must decode: {error:?}"));

        assert_eq!(decoded.implementations().len(), 1);
        assert_eq!(decoded.constraints().len(), 1);
        assert_eq!(decoded.coherence().len(), 1);
        assert_eq!(decoded.target_dependencies().len(), 1);

        assert_eq!(
            decoded.target_dependencies()[0].owner(),
            &InterfaceSymbolReference::Local(owner)
        );

        assert!(decoded.declaration_templates().is_empty());
        assert!(decoded.callable_contracts().is_empty());
    }

    #[test]
    fn implementation_fact_decoding_ignores_unrelated_semantic_records() {
        const UNRELATED_RECORDS: &[(InterfaceSectionTag, usize)] = &[
            (InterfaceSectionTag::SemanticTypes, 4),
            (InterfaceSectionTag::Constants, 0),
            (InterfaceSectionTag::Contracts, 1),
            (InterfaceSectionTag::Implementations, 0),
            (InterfaceSectionTag::Implementations, 1),
        ];

        for &(tag, table_index) in UNRELATED_RECORDS {
            let (surface, owner, mut sections) = implementation_fixture();

            let section = section_mut(&mut sections, tag);

            append_record(&mut section.2, table_index, &[u8::MAX]);
            section.1 += 1;

            let decoded = decode_semantic_fact_graph(
                &owned_section_views(&sections),
                &surface,
                owner,
                InterfaceSemanticFactKind::Implementation,
                InterfaceValidationLimits::default(),
            )
            .unwrap_or_else(|error| {
                panic!("unrelated record in {tag:?} table {table_index} must be ignored: {error:?}")
            });

            assert_eq!(decoded.types().len(), 1);
            assert_eq!(decoded.constant_values().len(), 1);
            assert_eq!(decoded.constraints().len(), 1);
            assert_eq!(decoded.implementations().len(), 1);
            assert_eq!(decoded.coherence().len(), 1);

            assert!(
                decode_semantic_facts(
                    &owned_section_views(&sections),
                    &surface,
                    InterfaceValidationLimits::default(),
                )
                .is_err()
            );
        }
    }

    #[test]
    fn implementation_fact_decoding_rejects_missing_header() {
        let bundle = package_interface_export_bundle();
        let surface = bundle.surface().clone();

        let InterfaceSymbolReference::Local(owner) =
            local_by_kind(&surface, SymbolKind::NamedTraitImplementation)
        else {
            panic!("test implementation must be local");
        };

        let facts = bundle.semantic_facts().clone().with_implementations([], []);

        let sections = semantic_sections(&facts, &surface);

        assert_eq!(
            decode_semantic_fact_graph(
                &owned_section_views(&sections),
                &surface,
                owner,
                InterfaceSemanticFactKind::Implementation,
                InterfaceValidationLimits::default(),
            ),
            Err(InterfaceValidationError::Malformed)
        );
    }

    #[test]
    fn exact_fact_decoding_is_independent_of_request_order() {
        let (surface, owner, sections) = implementation_fixture();

        let sections = owned_section_views(&sections);
        let limits = InterfaceValidationLimits::default();

        let forward_implementation = decode_semantic_fact_graph(
            &sections,
            &surface,
            owner,
            InterfaceSemanticFactKind::Implementation,
            limits,
        );

        let forward_constraint = decode_semantic_fact_graph(
            &sections,
            &surface,
            owner,
            InterfaceSemanticFactKind::GenericConstraint,
            limits,
        );

        let reverse_constraint = decode_semantic_fact_graph(
            &sections,
            &surface,
            owner,
            InterfaceSemanticFactKind::GenericConstraint,
            limits,
        );

        let reverse_implementation = decode_semantic_fact_graph(
            &sections,
            &surface,
            owner,
            InterfaceSemanticFactKind::Implementation,
            limits,
        );

        assert_eq!(forward_implementation, reverse_implementation);
        assert_eq!(forward_constraint, reverse_constraint);
    }

    #[test]
    fn runtime_requirements_round_trip_and_decode_as_exact_owner_facts() {
        let bundle = package_interface_export_bundle();
        let surface = bundle.surface().clone();
        let owner = local_by_kind(&surface, SymbolKind::Function);

        let facts = bundle
            .semantic_facts()
            .clone()
            .with_runtime_requirements([runtime_requirement(owner.clone())]);

        let sections = semantic_sections(&facts, &surface);
        let views = owned_section_views(&sections);
        let limits = InterfaceValidationLimits::default();

        assert_eq!(
            decode_semantic_facts(&views, &surface, limits),
            Ok(facts.clone())
        );

        let InterfaceSymbolReference::Local(owner_id) = owner else {
            panic!("test runtime requirement owner must be local");
        };

        let decoded = decode_semantic_fact_graph(
            &views,
            &surface,
            owner_id,
            InterfaceSemanticFactKind::Runtime,
            limits,
        )
        .unwrap_or_else(|error| panic!("runtime requirement must decode: {error:?}"));

        assert_eq!(decoded.runtime_requirements(), facts.runtime_requirements());
        assert!(decoded.types().is_empty());
        assert!(decoded.callable_contracts().is_empty());
    }

    #[test]
    fn runtime_requirements_reject_private_product_selection_facts() {
        let bundle = package_interface_export_bundle();
        let surface = bundle.surface();
        let owner = local_by_kind(surface, SymbolKind::Function);

        let Some(runtime) = RuntimeIdentity::try_new("bray.runtime.test") else {
            panic!("test runtime identity must be valid");
        };

        for requirements in [
            runtime_requirements(Some(runtime), []),
            runtime_requirements(None, [RuntimeAbiRole::TaskStart]),
        ] {
            let facts = bundle.semantic_facts().clone().with_runtime_requirements([
                InterfaceRuntimeRequirement::new(owner.clone(), None, requirements),
            ]);

            assert_eq!(
                encode_semantic_facts(&facts, surface, InterfaceValidationLimits::default()),
                Err(InterfaceValidationError::Malformed)
            );
        }
    }

    #[test]
    fn declaration_facts_decode_through_exact_narrow_sections() {
        let bundle = package_interface_export_bundle();
        let surface = bundle.surface().clone();

        let InterfaceSymbolReference::Local(callable) =
            local_by_kind(&surface, SymbolKind::Function)
        else {
            panic!("test callable must be local");
        };

        let InterfaceSymbolReference::Local(parameter) =
            local_by_kind(&surface, SymbolKind::CallableParameter)
        else {
            panic!("test callable parameter must be local");
        };

        let predicate_owners = [
            (
                SymbolKind::Predicate,
                InterfacePredicateDefinitionState::OpaqueTrusted,
            ),
            (
                SymbolKind::TraitPredicateMember,
                InterfacePredicateDefinitionState::Required,
            ),
            (
                SymbolKind::TraitPredicateFulfillment,
                InterfacePredicateDefinitionState::Defined,
            ),
        ];

        let sections = semantic_sections(bundle.semantic_facts(), &surface);
        let sections = owned_section_views(&sections);
        let limits = InterfaceValidationLimits::default();

        let signature = decode_semantic_fact_graph(
            &sections,
            &surface,
            callable,
            InterfaceSemanticFactKind::CallableSignature,
            limits,
        )
        .unwrap_or_else(|error| panic!("callable signature must decode: {error:?}"));

        let generic = decode_semantic_fact_graph(
            &sections,
            &surface,
            callable,
            InterfaceSemanticFactKind::GenericDeclaration,
            limits,
        )
        .unwrap_or_else(|error| panic!("generic declaration must decode: {error:?}"));

        let default = decode_semantic_fact_graph(
            &sections,
            &surface,
            parameter,
            InterfaceSemanticFactKind::CallableParameterDefault,
            limits,
        )
        .unwrap_or_else(|error| panic!("callable default must decode: {error:?}"));

        assert_eq!(signature.callable_signatures().len(), 1);
        assert!(signature.callable_signatures()[0].has_body());
        assert_eq!(signature.types().len(), 3);
        assert_eq!(signature.constant_terms().len(), 1);
        assert_eq!(generic.generic_declarations().len(), 1);
        assert_eq!(generic.constraints().len(), 1);
        assert_eq!(default.callable_parameter_defaults().len(), 1);
        assert!(default.types().is_empty());
        assert!(default.constant_terms().is_empty());

        for (kind, expected_state) in predicate_owners {
            let InterfaceSymbolReference::Local(owner) = local_by_kind(&surface, kind) else {
                panic!("test predicate must be local");
            };

            let predicate = decode_semantic_fact_graph(
                &sections,
                &surface,
                owner,
                InterfaceSemanticFactKind::PredicateDefinition,
                limits,
            )
            .unwrap_or_else(|error| panic!("predicate definition must decode: {error:?}"));

            assert_eq!(predicate.predicate_definitions().len(), 1);
            assert_eq!(predicate.predicate_definitions()[0].state(), expected_state);
            assert!(predicate.declaration_templates().is_empty());
        }
    }

    #[test]
    fn predicate_definition_decoding_rejects_opaque_state_with_definition_template() {
        let bundle = package_interface_export_bundle();
        let surface = bundle.surface().clone();
        let facts = bundle.semantic_facts();

        let opaque_owner = local_by_kind(&surface, SymbolKind::Predicate);

        let InterfaceSymbolReference::Local(owner) = opaque_owner.clone() else {
            panic!("test predicate must be local");
        };

        let mut declarations = facts.declaration_templates().to_vec();

        let definition = declarations
            .iter_mut()
            .find(|template| template.kind() == CheckedTemplateKind::PredicateDefinition)
            .unwrap_or_else(|| panic!("test predicate definition template must be present"));

        *definition = InterfaceDeclarationTemplate::new(
            opaque_owner,
            definition.kind(),
            definition.ordinal(),
            definition.entity(),
        );

        declarations.sort();

        let invalid = facts.clone().with_templates(
            facts.checked_templates().iter().cloned(),
            declarations,
            facts.support_entities().iter().cloned(),
        );

        let sections = encode_validated_semantic_facts(&invalid)
            .into_iter()
            .map(crate::EncodedSemanticSection::into_parts)
            .collect::<Vec<_>>();

        let decoded = decode_semantic_fact_graph(
            &owned_section_views(&sections),
            &surface,
            owner,
            InterfaceSemanticFactKind::PredicateDefinition,
            InterfaceValidationLimits::default(),
        );

        assert_eq!(decoded, Err(InterfaceValidationError::Malformed));
    }

    #[test]
    fn callable_signature_decoding_ignores_corrupt_unrelated_contract_records() {
        let bundle = package_interface_export_bundle();
        let surface = bundle.surface().clone();
        let base = bundle.semantic_facts();

        let dependency_contract =
            InterfaceDependencyContract::new([InterfaceDependencyRequirement::new(
                InterfaceDependencySubject::new(
                    InterfaceDependencySubjectRoot::Parameter(SymbolOrdinal::new(0)),
                    [],
                ),
                InterfaceDependencyRequirementKind::StorageInitialized,
            )]);

        let facts = base.clone().with_values(
            [dependency_contract.clone()],
            base.types().iter().cloned(),
            base.constant_values().iter().cloned(),
            base.constant_terms().iter().cloned(),
        );

        let mut sections = semantic_sections(&facts, &surface);

        let InterfaceSymbolReference::Local(callable) =
            local_by_kind(&surface, SymbolKind::Function)
        else {
            panic!("test callable must be local");
        };

        let contracts = section_mut(&mut sections, InterfaceSectionTag::Contracts);
        let unrelated_constraint = record_range(&contracts.2, 1, 0);

        contracts.2[unrelated_constraint.start..unrelated_constraint.start + 4]
            .copy_from_slice(&u32::MAX.to_le_bytes());

        let signature = decode_semantic_fact_graph(
            &owned_section_views(&sections),
            &surface,
            callable,
            InterfaceSemanticFactKind::CallableSignature,
            InterfaceValidationLimits::default(),
        )
        .unwrap_or_else(|error| panic!("callable signature must decode: {error:?}"));

        assert_eq!(signature.callable_signatures().len(), 1);

        assert_eq!(
            signature.dependency_contracts.as_ref(),
            &[dependency_contract]
        );

        assert!(
            decode_semantic_facts(
                &owned_section_views(&sections),
                &surface,
                InterfaceValidationLimits::default(),
            )
            .is_err()
        );
    }

    #[test]
    fn declaration_section_format_versions_are_rejected_before_record_decoding() {
        const VERSIONED_SECTIONS: &[InterfaceSectionTag] = &[
            InterfaceSectionTag::DeclarationFacts,
            InterfaceSectionTag::DeclarationTemplates,
        ];

        for &tag in VERSIONED_SECTIONS {
            let bundle = package_interface_export_bundle();
            let surface = bundle.surface().clone();
            let mut sections = semantic_sections(bundle.semantic_facts(), &surface);
            let section = section_mut(&mut sections, tag);

            section.2[..4].copy_from_slice(&u32::MAX.to_le_bytes());

            assert_eq!(
                decode_semantic_facts(
                    &owned_section_views(&sections),
                    &surface,
                    InterfaceValidationLimits::default(),
                ),
                Err(InterfaceValidationError::Malformed),
                "{tag:?}"
            );
        }
    }

    #[test]
    fn declaration_fact_validation_rejects_surface_mismatches() {
        let bundle = package_interface_export_bundle();
        let surface = bundle.surface();
        let base = bundle.semantic_facts();
        let signature = &base.callable_signatures()[0];

        let invalid_signature = InterfaceCallableSignature::new(
            signature.owner().clone(),
            InterfaceTypeId::new(0),
            signature.receiver().cloned(),
            signature.parameters().iter().cloned(),
            signature.result(),
        )
        .with_body(signature.has_body());

        let invalid_signature_facts = base.clone().with_declarations(
            [invalid_signature],
            base.generic_declarations().iter().cloned(),
            base.callable_parameter_defaults().iter().cloned(),
            base.predicate_definitions().iter().cloned(),
        );

        assert_eq!(
            encode_semantic_facts(
                &invalid_signature_facts,
                surface,
                InterfaceValidationLimits::default(),
            ),
            Err(InterfaceValidationError::Malformed)
        );

        let generic = &base.generic_declarations()[0];

        let reversed_generic = InterfaceGenericDeclaration::new(
            generic.owner().clone(),
            generic.parameters().iter().rev().cloned(),
        );

        let reversed_generic_facts = base.clone().with_declarations(
            base.callable_signatures().iter().cloned(),
            [reversed_generic],
            base.callable_parameter_defaults().iter().cloned(),
            base.predicate_definitions().iter().cloned(),
        );

        assert_eq!(
            encode_semantic_facts(
                &reversed_generic_facts,
                surface,
                InterfaceValidationLimits::default(),
            ),
            Err(InterfaceValidationError::Malformed)
        );

        let default = &base.callable_parameter_defaults()[0];

        let absent_default =
            InterfaceCallableParameterDefault::new(default.parameter().clone(), false);

        let absent_default_facts = base.clone().with_declarations(
            base.callable_signatures().iter().cloned(),
            base.generic_declarations().iter().cloned(),
            [absent_default],
            base.predicate_definitions().iter().cloned(),
        );

        assert_eq!(
            encode_semantic_facts(
                &absent_default_facts,
                surface,
                InterfaceValidationLimits::default(),
            ),
            Err(InterfaceValidationError::Malformed)
        );
    }

    #[test]
    fn referenced_implementation_corruption_fails_deterministically() {
        let (surface, owner, mut sections) = implementation_fixture();

        let section = section_mut(&mut sections, InterfaceSectionTag::Implementations);
        let implementation = record_range_with_local_owner(&section.2, 0, owner);

        section.2[implementation.start..implementation.start + 4]
            .copy_from_slice(&u32::MAX.to_le_bytes());

        let decode = || {
            decode_semantic_fact_graph(
                &owned_section_views(&sections),
                &surface,
                owner,
                InterfaceSemanticFactKind::Implementation,
                InterfaceValidationLimits::default(),
            )
        };

        let first = decode();
        let second = decode();

        assert_eq!(first, second);
        assert_eq!(first, Err(InterfaceValidationError::Malformed));
    }

    #[test]
    fn referenced_transitive_record_corruption_fails_deterministically() {
        let (surface, owner, mut sections) = implementation_fixture();

        let section = section_mut(&mut sections, InterfaceSectionTag::SemanticTypes);
        let subject_type = record_range(&section.2, 4, 1);

        section.2[subject_type.start..subject_type.start + 4]
            .copy_from_slice(&u32::MAX.to_le_bytes());

        let decode = || {
            decode_semantic_fact_graph(
                &owned_section_views(&sections),
                &surface,
                owner,
                InterfaceSemanticFactKind::Implementation,
                InterfaceValidationLimits::default(),
            )
        };

        let first = decode();
        let second = decode();

        assert_eq!(first, second);
        assert_eq!(first, Err(InterfaceValidationError::Malformed));
    }

    #[test]
    fn narrow_decoding_validates_unrelated_record_ranges() {
        let (surface, owner, mut sections) = implementation_fixture();

        let section = section_mut(&mut sections, InterfaceSectionTag::SemanticTypes);

        append_record(&mut section.2, 4, &[u8::MAX]);
        section.1 += 1;

        let unrelated = record_directory_entry(&section.2, 4, 2);

        section.2[unrelated.start..unrelated.start + 4].copy_from_slice(&u32::MAX.to_le_bytes());

        assert_eq!(
            decode_semantic_fact_graph(
                &owned_section_views(&sections),
                &surface,
                owner,
                InterfaceSemanticFactKind::Implementation,
                InterfaceValidationLimits::default(),
            ),
            Err(InterfaceValidationError::Malformed)
        );
    }

    fn implementation_fixture() -> (
        PackageInterfaceSurface,
        InterfaceSymbolId,
        Vec<OwnedSection>,
    ) {
        let bundle = package_interface_export_bundle();
        let surface = bundle.surface().clone();

        let InterfaceSymbolReference::Local(owner) =
            local_by_kind(&surface, SymbolKind::NamedTraitImplementation)
        else {
            panic!("test implementation must be local");
        };

        let sections = semantic_sections(bundle.semantic_facts(), &surface);

        (surface, owner, sections)
    }

    fn semantic_sections(
        facts: &InterfaceSemanticFacts,
        surface: &PackageInterfaceSurface,
    ) -> Vec<OwnedSection> {
        encode_semantic_facts(facts, surface, InterfaceValidationLimits::default())
            .unwrap_or_else(|error| panic!("test semantic facts must encode: {error:?}"))
            .into_iter()
            .map(crate::EncodedSemanticSection::into_parts)
            .collect()
    }

    fn runtime_requirement(owner: InterfaceSymbolReference) -> InterfaceRuntimeRequirement {
        InterfaceRuntimeRequirement::new(
            owner,
            Some(ProtectedAsyncFrameId::new([23; 32])),
            runtime_requirements(None, []),
        )
    }

    fn runtime_requirements<const R: usize>(
        runtime: Option<RuntimeIdentity>,
        roles: [RuntimeAbiRole; R],
    ) -> RuntimeRequirements {
        let Some(target) = TargetIdentity::try_new("x86_64-unknown-linux-gnu") else {
            panic!("test target identity must be valid");
        };

        let Some(panic_abi) = PanicAbiIdentity::try_new("bray.panic.test") else {
            panic!("test panic ABI identity must be valid");
        };

        let abi = RuntimeAbiVersion::new(1, 0);

        RuntimeRequirements::new(
            runtime,
            abi,
            Some(ProtectedFrameAbiVersions::uniform(abi)),
            target,
            panic_abi,
            roles,
            [
                RuntimeCapability::CooperativeExecution,
                RuntimeCapability::MainThreadLane,
            ],
            [ExecutionLaneRequirement::MainThread],
        )
    }

    fn section_mut(sections: &mut [OwnedSection], tag: InterfaceSectionTag) -> &mut OwnedSection {
        sections
            .iter_mut()
            .find(|section| section.0 == tag)
            .unwrap_or_else(|| panic!("test semantic section must exist"))
    }
}
