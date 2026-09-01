use super::{contract, declaration, directory, selection, support, surface, template, value};

use crate::semantic::codec::common::SemanticDecodeContext;
use crate::{
    InterfaceSectionTag, InterfaceSemantics, InterfaceValidationError, InterfaceValidationLimits,
    PackageInterfaceSurface, ValidatedInterfaceSection,
};

pub(crate) const COMPLETE_SEMANTIC_SECTIONS: &[InterfaceSectionTag] = &[
    InterfaceSectionTag::SemanticRecordDirectory,
    InterfaceSectionTag::SemanticTypes,
    InterfaceSectionTag::Constants,
    InterfaceSectionTag::Contracts,
    InterfaceSectionTag::DeclarationTemplates,
    InterfaceSectionTag::Implementations,
    InterfaceSectionTag::TargetDependencies,
    InterfaceSectionTag::SupportGraph,
    InterfaceSectionTag::DeclarationSemantics,
];

/// Decodes and validates every semantic record section in one package interface.
pub fn decode_semantics(
    sections: &[ValidatedInterfaceSection<'_>],
    surface: &PackageInterfaceSurface,
    limits: InterfaceValidationLimits,
) -> Result<InterfaceSemantics, InterfaceValidationError> {
    validate_decode_allocation(sections, limits)?;

    let mut context = SemanticDecodeContext::new(limits);

    let directory = required_section(sections, InterfaceSectionTag::SemanticRecordDirectory)?;
    let types = required_section(sections, InterfaceSectionTag::SemanticTypes)?;
    let constants = required_section(sections, InterfaceSectionTag::Constants)?;
    let contracts = required_section(sections, InterfaceSectionTag::Contracts)?;
    let declarations = required_section(sections, InterfaceSectionTag::DeclarationSemantics)?;
    let templates = required_section(sections, InterfaceSectionTag::DeclarationTemplates)?;
    let implementations = required_section(sections, InterfaceSectionTag::Implementations)?;
    let targets = required_section(sections, InterfaceSectionTag::TargetDependencies)?;
    let provenance = optional_section(sections, InterfaceSectionTag::SourceProvenance);
    let support = required_section(sections, InterfaceSectionTag::SupportGraph)?;

    let mut semantics = value::decode_types(types, limits, &mut context)?;

    value::decode_constants(constants, limits, &mut context, &mut semantics)?;
    contract::decode_contracts(contracts, limits, &mut context, &mut semantics)?;
    declaration::decode_declarations(declarations, limits, &mut context, &mut semantics)?;
    template::decode_templates(templates, limits, &mut context, &mut semantics)?;
    surface::decode_implementations(implementations, limits, &mut context, &mut semantics)?;
    surface::decode_target_dependencies(targets, limits, &mut context, &mut semantics)?;
    support::decode_support_graph(support, limits, &mut context, &mut semantics)?;

    if let Some(provenance) = provenance {
        surface::decode_provenance(provenance, limits, &mut context, &mut semantics)?;
    }

    let encoded_directory = directory::decode_semantic_directory(directory, limits, &mut context)?;

    if encoded_directory.as_ref() != semantics.semantic_directory().as_ref() {
        return Err(crate::semantic::codec::invalid_value(
            crate::InterfaceValidationField::RecordPayload,
        ));
    }

    semantics.validate(surface, limits)?;

    Ok(semantics)
}

pub(crate) fn decode_semantic_graph(
    sections: &[ValidatedInterfaceSection<'_>],
    surface: &PackageInterfaceSurface,
    owner: bray_symbols::InterfaceSymbolId,
    kind: crate::InterfaceSemanticRecordKind,
    limits: InterfaceValidationLimits,
) -> Result<InterfaceSemantics, InterfaceValidationError> {
    match decode_selected_semantic_graph(sections, surface, owner, kind, limits)? {
        Some(semantics) => Ok(semantics),
        None => decode_semantics(sections, surface, limits),
    }
}

pub(crate) fn decode_selected_semantic_graph(
    sections: &[ValidatedInterfaceSection<'_>],
    surface: &PackageInterfaceSurface,
    owner: bray_symbols::InterfaceSymbolId,
    kind: crate::InterfaceSemanticRecordKind,
    limits: InterfaceValidationLimits,
) -> Result<Option<InterfaceSemantics>, InterfaceValidationError> {
    let Some(required_tags) = selected_semantic_sections(kind) else {
        return Ok(None);
    };

    let selected_sections = sections
        .iter()
        .copied()
        .filter(|section| required_tags.contains(&section.tag()))
        .collect::<Vec<_>>();

    validate_decode_allocation(&selected_sections, limits)?;

    selection::decode_selected_record_graph(sections, surface, owner, kind, limits).map(Some)
}

pub(crate) fn selected_semantic_sections(
    kind: crate::InterfaceSemanticRecordKind,
) -> Option<&'static [InterfaceSectionTag]> {
    const GENERIC_CONSTRAINT_SECTIONS: &[InterfaceSectionTag] = &[
        InterfaceSectionTag::SemanticRecordDirectory,
        InterfaceSectionTag::SemanticTypes,
        InterfaceSectionTag::Constants,
        InterfaceSectionTag::Contracts,
    ];

    const CALLABLE_SIGNATURE_SECTIONS: &[InterfaceSectionTag] = &[
        InterfaceSectionTag::SemanticRecordDirectory,
        InterfaceSectionTag::SemanticTypes,
        InterfaceSectionTag::Constants,
        InterfaceSectionTag::Contracts,
        InterfaceSectionTag::DeclarationSemantics,
    ];

    const GENERIC_DECLARATION_SECTIONS: &[InterfaceSectionTag] = CALLABLE_SIGNATURE_SECTIONS;

    const CALLABLE_PARAMETER_DEFAULT_SECTIONS: &[InterfaceSectionTag] = &[
        InterfaceSectionTag::SemanticRecordDirectory,
        InterfaceSectionTag::DeclarationSemantics,
    ];

    const PREDICATE_DEFINITION_SECTIONS: &[InterfaceSectionTag] = &[
        InterfaceSectionTag::SemanticRecordDirectory,
        InterfaceSectionTag::DeclarationSemantics,
        InterfaceSectionTag::DeclarationTemplates,
    ];

    const IMPLEMENTATION_SECTIONS: &[InterfaceSectionTag] = &[
        InterfaceSectionTag::SemanticRecordDirectory,
        InterfaceSectionTag::SemanticTypes,
        InterfaceSectionTag::Constants,
        InterfaceSectionTag::Contracts,
        InterfaceSectionTag::Implementations,
        InterfaceSectionTag::TargetDependencies,
    ];

    const TARGET_SEMANTIC_SECTIONS: &[InterfaceSectionTag] = &[
        InterfaceSectionTag::SemanticRecordDirectory,
        InterfaceSectionTag::SemanticTypes,
        InterfaceSectionTag::Constants,
        InterfaceSectionTag::Contracts,
        InterfaceSectionTag::TargetDependencies,
    ];

    match kind {
        crate::InterfaceSemanticRecordKind::CallableSignature => Some(CALLABLE_SIGNATURE_SECTIONS),
        crate::InterfaceSemanticRecordKind::GenericDeclaration => {
            Some(GENERIC_DECLARATION_SECTIONS)
        }
        crate::InterfaceSemanticRecordKind::CallableParameterDefault => {
            Some(CALLABLE_PARAMETER_DEFAULT_SECTIONS)
        }
        crate::InterfaceSemanticRecordKind::PredicateDefinition => {
            Some(PREDICATE_DEFINITION_SECTIONS)
        }
        crate::InterfaceSemanticRecordKind::DeclaredType => Some(CALLABLE_SIGNATURE_SECTIONS),
        crate::InterfaceSemanticRecordKind::TypeRepresentation => None,
        crate::InterfaceSemanticRecordKind::GenericConstraint => Some(GENERIC_CONSTRAINT_SECTIONS),
        crate::InterfaceSemanticRecordKind::Implementation => Some(IMPLEMENTATION_SECTIONS),
        crate::InterfaceSemanticRecordKind::TargetProperty => Some(TARGET_SEMANTIC_SECTIONS),
        crate::InterfaceSemanticRecordKind::Runtime => Some(TARGET_SEMANTIC_SECTIONS),
        crate::InterfaceSemanticRecordKind::CallableContracts
        | crate::InterfaceSemanticRecordKind::DeclarationTemplate
        | crate::InterfaceSemanticRecordKind::Abi => None,
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
                InterfaceSectionTag::SemanticRecordDirectory
                    | InterfaceSectionTag::SemanticTypes
                    | InterfaceSectionTag::Constants
                    | InterfaceSectionTag::Contracts
                    | InterfaceSectionTag::DeclarationSemantics
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
    optional_section(sections, tag).ok_or(crate::semantic::codec::invalid_value(
        crate::InterfaceValidationField::RecordPayload,
    ))
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
    use super::{decode_semantic_graph, decode_semantics, selected_semantic_sections};
    use crate::semantic::codec::{encode_semantics, encode_validated_semantics};
    use crate::test_support::{local_by_kind, package_interface_export_bundle};
    use crate::{
        InterfaceCallableParameterDefault, InterfaceCallableSignature,
        InterfaceDeclarationTemplate, InterfaceDeclaredType, InterfaceDependencyContract,
        InterfaceDependencyRequirement, InterfaceDependencyRequirementKind,
        InterfaceDependencySubject, InterfaceDependencySubjectRoot, InterfaceGenericDeclaration,
        InterfaceGenericSubstitutionId, InterfaceMalformedCause, InterfacePredicateDefinitionState,
        InterfaceRuntimeRequirement, InterfaceSectionTag, InterfaceSemanticRecordKind,
        InterfaceSemantics, InterfaceStorageMember, InterfaceStorageShape,
        InterfaceSymbolReference, InterfaceType, InterfaceTypeId, InterfaceTypeRepresentation,
        InterfaceValidationContext, InterfaceValidationError, InterfaceValidationField,
        InterfaceValidationLimits, PackageInterfaceSurface,
    };

    #[test]
    fn implementation_record_decoding_ignores_unrelated_template_bytes() {
        let (surface, owner, mut sections) = implementation_fixture();

        section_mut(&mut sections, InterfaceSectionTag::DeclarationTemplates)
            .2
            .clear();

        let decoded = decode_semantic_graph(
            &owned_section_views(&sections),
            &surface,
            owner,
            InterfaceSemanticRecordKind::Implementation,
            InterfaceValidationLimits::default(),
        )
        .unwrap_or_else(|error| panic!("implementation record must decode: {error:?}"));

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
    fn implementation_record_decoding_ignores_unrelated_semantic_records() {
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

            let decoded = decode_semantic_graph(
                &owned_section_views(&sections),
                &surface,
                owner,
                InterfaceSemanticRecordKind::Implementation,
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
                decode_semantics(
                    &owned_section_views(&sections),
                    &surface,
                    InterfaceValidationLimits::default(),
                )
                .is_err()
            );
        }
    }

    #[test]
    fn implementation_record_decoding_rejects_missing_header() {
        let bundle = package_interface_export_bundle();
        let surface = bundle.surface().clone();

        let InterfaceSymbolReference::Local(owner) =
            local_by_kind(&surface, SymbolKind::NamedTraitImplementation)
        else {
            panic!("test implementation must be local");
        };

        let semantics = bundle.semantics().clone().with_implementations([], []);

        let sections = semantic_sections(&semantics, &surface);

        assert_eq!(
            decode_semantic_graph(
                &owned_section_views(&sections),
                &surface,
                owner,
                InterfaceSemanticRecordKind::Implementation,
                InterfaceValidationLimits::default(),
            ),
            Err(crate::semantic::codec::invalid_value(
                crate::InterfaceValidationField::Reference
            ))
        );
    }

    #[test]
    fn exact_record_decoding_is_independent_of_request_order() {
        let (surface, owner, sections) = implementation_fixture();

        let sections = owned_section_views(&sections);
        let limits = InterfaceValidationLimits::default();

        let forward_implementation = decode_semantic_graph(
            &sections,
            &surface,
            owner,
            InterfaceSemanticRecordKind::Implementation,
            limits,
        );

        let forward_constraint = decode_semantic_graph(
            &sections,
            &surface,
            owner,
            InterfaceSemanticRecordKind::GenericConstraint,
            limits,
        );

        let reverse_constraint = decode_semantic_graph(
            &sections,
            &surface,
            owner,
            InterfaceSemanticRecordKind::GenericConstraint,
            limits,
        );

        let reverse_implementation = decode_semantic_graph(
            &sections,
            &surface,
            owner,
            InterfaceSemanticRecordKind::Implementation,
            limits,
        );

        assert_eq!(forward_implementation, reverse_implementation);
        assert_eq!(forward_constraint, reverse_constraint);
    }

    #[test]
    fn runtime_requirements_round_trip_and_decode_as_exact_owner_semantics() {
        let bundle = package_interface_export_bundle();
        let surface = bundle.surface().clone();
        let owner = local_by_kind(&surface, SymbolKind::Function);

        let semantics = bundle
            .semantics()
            .clone()
            .with_runtime_requirements([runtime_requirement(owner.clone())]);

        let sections = semantic_sections(&semantics, &surface);
        let views = owned_section_views(&sections);
        let limits = InterfaceValidationLimits::default();

        assert_eq!(
            decode_semantics(&views, &surface, limits),
            Ok(semantics.clone())
        );

        let InterfaceSymbolReference::Local(owner_id) = owner else {
            panic!("test runtime requirement owner must be local");
        };

        let selected_tags = selected_semantic_sections(InterfaceSemanticRecordKind::Runtime)
            .unwrap_or_else(|| panic!("runtime semantics must support selective decoding"));

        let selected_views = views
            .iter()
            .copied()
            .filter(|section| selected_tags.contains(&section.tag()))
            .collect::<Vec<_>>();

        let decoded = decode_semantic_graph(
            &selected_views,
            &surface,
            owner_id,
            InterfaceSemanticRecordKind::Runtime,
            limits,
        )
        .unwrap_or_else(|error| panic!("runtime requirement must decode: {error:?}"));

        assert_eq!(
            decoded.runtime_requirements(),
            semantics.runtime_requirements()
        );

        assert!(decoded.types().is_empty());
        assert!(decoded.callable_contracts().is_empty());
    }

    #[test]
    fn runtime_requirements_reject_private_product_selection_semantics() {
        let bundle = package_interface_export_bundle();
        let surface = bundle.surface();
        let owner = local_by_kind(surface, SymbolKind::Function);

        let Some(runtime) = RuntimeIdentity::try_new("bray.runtime.test") else {
            panic!("test runtime identity must be valid");
        };

        for requirements in [
            runtime_requirements(
                Some(runtime),
                [],
                Some(ProtectedFrameAbiVersions::uniform(RuntimeAbiVersion::new(
                    1, 0,
                ))),
            ),
            runtime_requirements(
                None,
                [RuntimeAbiRole::TaskStart],
                Some(ProtectedFrameAbiVersions::uniform(RuntimeAbiVersion::new(
                    1, 0,
                ))),
            ),
        ] {
            let semantics = bundle.semantics().clone().with_runtime_requirements([
                InterfaceRuntimeRequirement::new(
                    owner.clone(),
                    [ProtectedAsyncFrameId::new([31; 32])],
                    requirements,
                ),
            ]);

            assert_eq!(
                encode_semantics(&semantics, surface, InterfaceValidationLimits::default()),
                Err(semantic_invalid_value(
                    InterfaceSemanticRecordKind::Runtime,
                    InterfaceValidationField::Reference,
                ))
            );
        }
    }

    #[test]
    fn runtime_requirements_reject_mismatched_frame_contracts() {
        let bundle = package_interface_export_bundle();
        let surface = bundle.surface();
        let owner = local_by_kind(surface, SymbolKind::Function);
        let abi = RuntimeAbiVersion::new(1, 0);

        let cases = [
            InterfaceRuntimeRequirement::new(
                owner.clone(),
                [],
                runtime_requirements(None, [], Some(ProtectedFrameAbiVersions::uniform(abi))),
            ),
            InterfaceRuntimeRequirement::new(
                owner,
                [ProtectedAsyncFrameId::new([37; 32])],
                runtime_requirements(None, [], None),
            ),
        ];

        for requirement in cases {
            let semantics = bundle
                .semantics()
                .clone()
                .with_runtime_requirements([requirement]);

            assert_eq!(
                encode_semantics(&semantics, surface, InterfaceValidationLimits::default()),
                Err(semantic_invalid_value(
                    InterfaceSemanticRecordKind::Runtime,
                    InterfaceValidationField::Reference,
                ))
            );
        }
    }

    #[test]
    fn type_representations_reject_invalid_storage_shapes() {
        let bundle = package_interface_export_bundle();
        let surface = bundle.surface();
        let structure = local_by_kind(surface, SymbolKind::Struct);

        let invalid_type = InterfaceTypeRepresentation::new(structure.clone()).with_storage(
            InterfaceStorageShape::Structure(
                [InterfaceStorageMember::new(
                    None,
                    InterfaceTypeId::new(u32::MAX),
                )]
                .into(),
            ),
        );

        let invalid_kind = InterfaceTypeRepresentation::new(structure)
            .with_storage(InterfaceStorageShape::Union([].into()));

        for (representation, field) in [
            (invalid_type, crate::InterfaceValidationField::Discriminant),
            (invalid_kind, crate::InterfaceValidationField::Reference),
        ] {
            let semantics = bundle
                .semantics()
                .clone()
                .with_type_representations([representation]);

            assert_eq!(
                encode_semantics(&semantics, surface, InterfaceValidationLimits::default()),
                Err(semantic_invalid_value(
                    InterfaceSemanticRecordKind::TypeRepresentation,
                    field,
                ))
            );
        }
    }

    #[test]
    fn declaration_semantics_decode_through_exact_narrow_sections() {
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

        let sections = semantic_sections(bundle.semantics(), &surface);
        let sections = owned_section_views(&sections);
        let limits = InterfaceValidationLimits::default();

        let signature = decode_semantic_graph(
            &sections,
            &surface,
            callable,
            InterfaceSemanticRecordKind::CallableSignature,
            limits,
        )
        .unwrap_or_else(|error| panic!("callable signature must decode: {error:?}"));

        let generic = decode_semantic_graph(
            &sections,
            &surface,
            callable,
            InterfaceSemanticRecordKind::GenericDeclaration,
            limits,
        )
        .unwrap_or_else(|error| panic!("generic declaration must decode: {error:?}"));

        let default = decode_semantic_graph(
            &sections,
            &surface,
            parameter,
            InterfaceSemanticRecordKind::CallableParameterDefault,
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

            let predicate = decode_semantic_graph(
                &sections,
                &surface,
                owner,
                InterfaceSemanticRecordKind::PredicateDefinition,
                limits,
            )
            .unwrap_or_else(|error| panic!("predicate definition must decode: {error:?}"));

            assert_eq!(predicate.predicate_definitions().len(), 1);
            assert_eq!(predicate.predicate_definitions()[0].state(), expected_state);
            assert!(predicate.declaration_templates().is_empty());
        }
    }

    #[test]
    fn declared_types_decode_with_their_transitive_type_graph() {
        let bundle = package_interface_export_bundle();
        let surface = bundle.surface().clone();

        let InterfaceSymbolReference::Local(owner) = local_by_kind(&surface, SymbolKind::Struct)
        else {
            panic!("test structure must be local");
        };

        let semantics =
            bundle
                .semantics()
                .clone()
                .with_declared_types([InterfaceDeclaredType::new(
                    InterfaceSymbolReference::Local(owner),
                    InterfaceTypeId::new(1),
                )]);

        let sections = semantic_sections(&semantics, &surface);
        let sections = owned_section_views(&sections);

        let decoded = decode_semantic_graph(
            &sections,
            &surface,
            owner,
            InterfaceSemanticRecordKind::DeclaredType,
            InterfaceValidationLimits::default(),
        )
        .unwrap_or_else(|error| panic!("declared type must decode: {error:?}"));

        assert_eq!(decoded.declared_types().len(), 1);
        assert_eq!(decoded.declared_types()[0].ty(), InterfaceTypeId::new(0));
        assert_eq!(decoded.types().len(), 1);

        assert!(matches!(
            decoded.types()[0],
            InterfaceType::Named {
                substitution,
                ..
            } if substitution == InterfaceGenericSubstitutionId::new(0)
        ));
    }

    #[test]
    fn absent_generic_declarations_decode_as_empty_semantics() {
        let bundle = package_interface_export_bundle();
        let surface = bundle.surface().clone();

        let InterfaceSymbolReference::Local(owner) = local_by_kind(&surface, SymbolKind::Struct)
        else {
            panic!("test structure must be local");
        };

        let sections = semantic_sections(bundle.semantics(), &surface);
        let sections = owned_section_views(&sections);

        let decoded = decode_semantic_graph(
            &sections,
            &surface,
            owner,
            InterfaceSemanticRecordKind::GenericDeclaration,
            InterfaceValidationLimits::default(),
        )
        .unwrap_or_else(|error| panic!("absent generic declaration must decode: {error:?}"));

        assert!(decoded.generic_declarations().is_empty());
        assert!(decoded.constraints().is_empty());
    }

    #[test]
    fn predicate_definition_decoding_rejects_opaque_state_with_definition_template() {
        let bundle = package_interface_export_bundle();
        let surface = bundle.surface().clone();
        let semantics = bundle.semantics();

        let opaque_owner = local_by_kind(&surface, SymbolKind::Predicate);

        let InterfaceSymbolReference::Local(owner) = opaque_owner.clone() else {
            panic!("test predicate must be local");
        };

        let mut declarations = semantics.declaration_templates().to_vec();

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

        let invalid = semantics.clone().with_templates(
            semantics.checked_templates().iter().cloned(),
            declarations,
            semantics.support_entities().iter().cloned(),
        );

        let sections = encode_validated_semantics(&invalid)
            .into_iter()
            .map(crate::EncodedSemanticSection::into_parts)
            .collect::<Vec<_>>();

        let decoded = decode_semantic_graph(
            &owned_section_views(&sections),
            &surface,
            owner,
            InterfaceSemanticRecordKind::PredicateDefinition,
            InterfaceValidationLimits::default(),
        );

        assert_eq!(
            decoded,
            Err(crate::semantic::codec::invalid_value(
                crate::InterfaceValidationField::Declaration
            ))
        );
    }

    #[test]
    fn callable_signature_decoding_ignores_corrupt_unrelated_contract_records() {
        let bundle = package_interface_export_bundle();
        let surface = bundle.surface().clone();
        let base = bundle.semantics();

        let dependency_contract =
            InterfaceDependencyContract::new([InterfaceDependencyRequirement::new(
                InterfaceDependencySubject::new(
                    InterfaceDependencySubjectRoot::Parameter(SymbolOrdinal::new(0)),
                    [],
                ),
                InterfaceDependencyRequirementKind::StorageInitialized,
            )]);

        let semantics = base.clone().with_values(
            [dependency_contract.clone()],
            base.types().iter().cloned(),
            base.constant_values().iter().cloned(),
            base.constant_terms().iter().cloned(),
        );

        let mut sections = semantic_sections(&semantics, &surface);

        let InterfaceSymbolReference::Local(callable) =
            local_by_kind(&surface, SymbolKind::Function)
        else {
            panic!("test callable must be local");
        };

        let contracts = section_mut(&mut sections, InterfaceSectionTag::Contracts);
        let unrelated_constraint = record_range(&contracts.2, 1, 0);

        contracts.2[unrelated_constraint.start..unrelated_constraint.start + 4]
            .copy_from_slice(&u32::MAX.to_le_bytes());

        let signature = decode_semantic_graph(
            &owned_section_views(&sections),
            &surface,
            callable,
            InterfaceSemanticRecordKind::CallableSignature,
            InterfaceValidationLimits::default(),
        )
        .unwrap_or_else(|error| panic!("callable signature must decode: {error:?}"));

        assert_eq!(signature.callable_signatures().len(), 1);

        assert_eq!(
            signature.dependency_contracts.as_ref(),
            &[dependency_contract]
        );

        assert!(
            decode_semantics(
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
            InterfaceSectionTag::DeclarationSemantics,
            InterfaceSectionTag::DeclarationTemplates,
        ];

        for &tag in VERSIONED_SECTIONS {
            let bundle = package_interface_export_bundle();
            let surface = bundle.surface().clone();
            let mut sections = semantic_sections(bundle.semantics(), &surface);
            let section = section_mut(&mut sections, tag);

            section.2[..4].copy_from_slice(&u32::MAX.to_le_bytes());

            let field = match tag {
                InterfaceSectionTag::DeclarationSemantics => {
                    crate::InterfaceValidationField::Declaration
                }
                InterfaceSectionTag::DeclarationTemplates => {
                    crate::InterfaceValidationField::Template
                }
                _ => unreachable!(),
            };

            assert_eq!(
                decode_semantics(
                    &owned_section_views(&sections),
                    &surface,
                    InterfaceValidationLimits::default(),
                ),
                Err(crate::semantic::codec::invalid_value(field)),
                "{tag:?}"
            );
        }
    }

    #[test]
    fn declaration_record_validation_rejects_surface_mismatches() {
        let bundle = package_interface_export_bundle();
        let surface = bundle.surface();
        let base = bundle.semantics();
        let signature = &base.callable_signatures()[0];

        let invalid_signature = InterfaceCallableSignature::new(
            signature.owner().clone(),
            InterfaceTypeId::new(0),
            signature.receiver().cloned(),
            signature.parameters().iter().cloned(),
            signature.result(),
        )
        .with_body(signature.has_body());

        let invalid_signature_semantics = base.clone().with_declarations(
            [invalid_signature],
            base.generic_declarations().iter().cloned(),
            base.callable_parameter_defaults().iter().cloned(),
            base.predicate_definitions().iter().cloned(),
        );

        assert_eq!(
            encode_semantics(
                &invalid_signature_semantics,
                surface,
                InterfaceValidationLimits::default(),
            ),
            Err(semantic_invalid_value(
                InterfaceSemanticRecordKind::CallableSignature,
                InterfaceValidationField::Reference,
            ))
        );

        let generic = &base.generic_declarations()[0];

        let reversed_generic = InterfaceGenericDeclaration::new(
            generic.owner().clone(),
            generic.parameters().iter().rev().cloned(),
        );

        let reversed_generic_semantics = base.clone().with_declarations(
            base.callable_signatures().iter().cloned(),
            [reversed_generic],
            base.callable_parameter_defaults().iter().cloned(),
            base.predicate_definitions().iter().cloned(),
        );

        assert_eq!(
            encode_semantics(
                &reversed_generic_semantics,
                surface,
                InterfaceValidationLimits::default(),
            ),
            Err(semantic_invalid_value(
                InterfaceSemanticRecordKind::GenericDeclaration,
                InterfaceValidationField::Reference,
            ))
        );

        let default = &base.callable_parameter_defaults()[0];

        let absent_default =
            InterfaceCallableParameterDefault::new(default.parameter().clone(), false);

        let absent_default_semantics = base.clone().with_declarations(
            base.callable_signatures().iter().cloned(),
            base.generic_declarations().iter().cloned(),
            [absent_default],
            base.predicate_definitions().iter().cloned(),
        );

        assert_eq!(
            encode_semantics(
                &absent_default_semantics,
                surface,
                InterfaceValidationLimits::default(),
            ),
            Err(semantic_invalid_value(
                InterfaceSemanticRecordKind::CallableParameterDefault,
                InterfaceValidationField::Reference,
            ))
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
            decode_semantic_graph(
                &owned_section_views(&sections),
                &surface,
                owner,
                InterfaceSemanticRecordKind::Implementation,
                InterfaceValidationLimits::default(),
            )
        };

        let first = decode();
        let second = decode();

        assert_eq!(first, second);

        assert_eq!(
            first,
            Err(crate::InterfaceValidationError::Malformed {
                context: crate::InterfaceValidationContext::Record {
                    section: crate::InterfaceSectionTag::Implementations,
                    index: 1,
                },
                cause: crate::InterfaceMalformedCause::InvalidDiscriminant {
                    field: crate::InterfaceValidationField::Discriminant,
                    actual: u64::from(u32::MAX),
                },
            })
        );
    }

    #[test]
    fn referenced_transitive_record_corruption_fails_deterministically() {
        let (surface, owner, mut sections) = implementation_fixture();

        let section = section_mut(&mut sections, InterfaceSectionTag::SemanticTypes);
        let subject_type = record_range(&section.2, 4, 1);

        section.2[subject_type.start..subject_type.start + 4]
            .copy_from_slice(&u32::MAX.to_le_bytes());

        let decode = || {
            decode_semantic_graph(
                &owned_section_views(&sections),
                &surface,
                owner,
                InterfaceSemanticRecordKind::Implementation,
                InterfaceValidationLimits::default(),
            )
        };

        let first = decode();
        let second = decode();

        assert_eq!(first, second);

        assert_eq!(
            first,
            Err(crate::InterfaceValidationError::Malformed {
                context: crate::InterfaceValidationContext::Record {
                    section: crate::InterfaceSectionTag::SemanticTypes,
                    index: 1,
                },
                cause: crate::InterfaceMalformedCause::InvalidDiscriminant {
                    field: crate::InterfaceValidationField::Type,
                    actual: u64::from(u32::MAX),
                },
            })
        );
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
            decode_semantic_graph(
                &owned_section_views(&sections),
                &surface,
                owner,
                InterfaceSemanticRecordKind::Implementation,
                InterfaceValidationLimits::default(),
            ),
            Err(crate::InterfaceValidationError::Malformed {
                context: crate::InterfaceValidationContext::Record {
                    section: crate::InterfaceSectionTag::SemanticTypes,
                    index: 2,
                },
                cause: crate::InterfaceMalformedCause::OrderingViolation {
                    field: crate::InterfaceValidationField::RecordOffset,
                    previous: 28,
                    actual: u64::from(u32::MAX),
                },
            })
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

        let sections = semantic_sections(bundle.semantics(), &surface);

        (surface, owner, sections)
    }

    fn semantic_sections(
        semantics: &InterfaceSemantics,
        surface: &PackageInterfaceSurface,
    ) -> Vec<OwnedSection> {
        encode_semantics(semantics, surface, InterfaceValidationLimits::default())
            .unwrap_or_else(|error| panic!("test semantics must encode: {error:?}"))
            .into_iter()
            .map(crate::EncodedSemanticSection::into_parts)
            .collect()
    }

    fn runtime_requirement(owner: InterfaceSymbolReference) -> InterfaceRuntimeRequirement {
        InterfaceRuntimeRequirement::new(
            owner,
            [
                ProtectedAsyncFrameId::new([23; 32]),
                ProtectedAsyncFrameId::new([29; 32]),
            ],
            runtime_requirements(
                None,
                [],
                Some(ProtectedFrameAbiVersions::uniform(RuntimeAbiVersion::new(
                    1, 0,
                ))),
            ),
        )
    }

    fn runtime_requirements<const R: usize>(
        runtime: Option<RuntimeIdentity>,
        roles: [RuntimeAbiRole; R],
        frame_abi: Option<ProtectedFrameAbiVersions>,
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
            frame_abi,
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

    const fn semantic_invalid_value(
        kind: InterfaceSemanticRecordKind,
        field: InterfaceValidationField,
    ) -> InterfaceValidationError {
        InterfaceValidationError::Malformed {
            context: InterfaceValidationContext::SemanticRecord { kind, index: 0 },
            cause: InterfaceMalformedCause::InvalidValue { field },
        }
    }

    fn section_mut(sections: &mut [OwnedSection], tag: InterfaceSectionTag) -> &mut OwnedSection {
        sections
            .iter_mut()
            .find(|section| section.0 == tag)
            .unwrap_or_else(|| panic!("test semantic section must exist"))
    }
}
