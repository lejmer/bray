use super::{contract, directory, selection, support, surface, template, value};

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
    let templates = required_section(sections, InterfaceSectionTag::DeclarationTemplates)?;
    let implementations = required_section(sections, InterfaceSectionTag::Implementations)?;
    let targets = required_section(sections, InterfaceSectionTag::TargetDependencies)?;
    let provenance = optional_section(sections, InterfaceSectionTag::SourceProvenance);
    let support = required_section(sections, InterfaceSectionTag::SupportGraph)?;

    let mut facts = value::decode_types(types, limits, &mut context)?;

    value::decode_constants(constants, limits, &mut context, &mut facts)?;
    contract::decode_contracts(contracts, limits, &mut context, &mut facts)?;
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
        crate::InterfaceSemanticFactKind::GenericConstraint => Some(GENERIC_CONSTRAINT_SECTIONS),
        crate::InterfaceSemanticFactKind::Implementation => Some(IMPLEMENTATION_SECTIONS),
        crate::InterfaceSemanticFactKind::TargetFact => Some(TARGET_FACT_SECTIONS),
        crate::InterfaceSemanticFactKind::CallableContracts
        | crate::InterfaceSemanticFactKind::DeclarationTemplate
        | crate::InterfaceSemanticFactKind::Abi => None,
    }
}

fn validate_decode_allocation(
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
    use bray_symbols::{InterfaceSymbolId, SymbolKind};

    use super::super::test_support::{
        OwnedSection, append_record, owned_section_views, record_directory_entry, record_range,
    };
    use super::{decode_semantic_fact_graph, decode_semantic_facts};
    use crate::semantic::codec::encode_semantic_facts;
    use crate::test_support::{local_by_kind, package_interface_export_bundle};
    use crate::{
        InterfaceSectionTag, InterfaceSemanticFactKind, InterfaceSemanticFacts,
        InterfaceSymbolReference, InterfaceValidationError, InterfaceValidationLimits,
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
    fn referenced_implementation_corruption_fails_deterministically() {
        let (surface, owner, mut sections) = implementation_fixture();
        let section = section_mut(&mut sections, InterfaceSectionTag::Implementations);
        let implementation = record_range(&section.2, 0, 0);

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

    fn section_mut(sections: &mut [OwnedSection], tag: InterfaceSectionTag) -> &mut OwnedSection {
        sections
            .iter_mut()
            .find(|section| section.0 == tag)
            .unwrap_or_else(|| panic!("test semantic section must exist"))
    }
}
