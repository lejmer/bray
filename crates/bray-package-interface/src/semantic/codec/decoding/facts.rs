use std::sync::Arc;

use super::{contract, directory, support, surface, template, value};

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

    let mut context = SemanticDecodeContext::new(limits);

    // TODO(BRA-243): Decode only the transitive record closure required by this exact fact.
    let types = required_section(sections, InterfaceSectionTag::SemanticTypes)?;
    let constants = required_section(sections, InterfaceSectionTag::Constants)?;

    let mut facts = value::decode_types(types, limits, &mut context)?;

    value::decode_constants(constants, limits, &mut context, &mut facts)?;

    let directory = required_section(sections, InterfaceSectionTag::SymbolFactDirectory)?;
    let encoded_directory = directory::decode_fact_directory(directory, limits, &mut context)?;

    match kind {
        crate::InterfaceSemanticFactKind::GenericConstraint => decode_selected_constraints(
            sections,
            owner,
            limits,
            &encoded_directory,
            &mut context,
            &mut facts,
        )?,
        crate::InterfaceSemanticFactKind::Implementation => decode_selected_implementation(
            sections,
            owner,
            limits,
            &encoded_directory,
            &mut context,
            &mut facts,
        )?,
        crate::InterfaceSemanticFactKind::TargetFact => decode_selected_target_facts(
            sections,
            owner,
            limits,
            &encoded_directory,
            &mut context,
            &mut facts,
        )?,
        _ => unreachable!("unsupported fact kinds returned above"),
    }

    facts.validate(surface, limits)?;

    Ok(facts)
}

fn decode_selected_constraints(
    sections: &[ValidatedInterfaceSection<'_>],
    owner: bray_symbols::InterfaceSymbolId,
    limits: InterfaceValidationLimits,
    encoded_directory: &[crate::InterfaceSemanticFactEntry],
    context: &mut SemanticDecodeContext,
    facts: &mut InterfaceSemanticFacts,
) -> Result<(), InterfaceValidationError> {
    let contracts = required_section(sections, InterfaceSectionTag::Contracts)?;

    contract::decode_contracts(contracts, limits, context, facts)?;

    validate_selected_fact_directory(
        facts,
        encoded_directory,
        owner,
        crate::InterfaceSemanticFactKind::GenericConstraint,
    )?;

    let owner = crate::InterfaceSymbolReference::Local(owner);

    retain_owner_constraints(facts, &owner);

    Ok(())
}

fn decode_selected_implementation(
    sections: &[ValidatedInterfaceSection<'_>],
    owner: bray_symbols::InterfaceSymbolId,
    limits: InterfaceValidationLimits,
    encoded_directory: &[crate::InterfaceSemanticFactEntry],
    context: &mut SemanticDecodeContext,
    facts: &mut InterfaceSemanticFacts,
) -> Result<(), InterfaceValidationError> {
    let contracts = required_section(sections, InterfaceSectionTag::Contracts)?;
    let implementations = required_section(sections, InterfaceSectionTag::Implementations)?;
    let targets = required_section(sections, InterfaceSectionTag::TargetDependencies)?;

    contract::decode_contracts(contracts, limits, context, facts)?;
    surface::decode_implementations(implementations, limits, context, facts)?;
    surface::decode_target_dependencies(targets, limits, context, facts)?;

    validate_selected_fact_directory(
        facts,
        encoded_directory,
        owner,
        crate::InterfaceSemanticFactKind::GenericConstraint,
    )?;

    validate_selected_fact_directory(
        facts,
        encoded_directory,
        owner,
        crate::InterfaceSemanticFactKind::Implementation,
    )?;

    validate_selected_fact_directory(
        facts,
        encoded_directory,
        owner,
        crate::InterfaceSemanticFactKind::TargetFact,
    )?;

    let owner = crate::InterfaceSymbolReference::Local(owner);

    retain_owner_constraints(facts, &owner);

    if facts
        .implementations
        .iter()
        .filter(|implementation| implementation.implementation == owner)
        .count()
        != 1
    {
        return Err(InterfaceValidationError::Malformed);
    }

    // The selected graph owns shallow copies of its implementation and coherence records.
    facts.implementations = facts
        .implementations
        .iter()
        .filter(|implementation| implementation.implementation == owner)
        .cloned()
        .collect();

    facts.coherence = facts
        .coherence
        .iter()
        .filter(|coherence| coherence.implementations.contains(&owner))
        .cloned()
        .collect();

    retain_owner_target_dependencies(facts, &owner);

    Ok(())
}

fn decode_selected_target_facts(
    sections: &[ValidatedInterfaceSection<'_>],
    owner: bray_symbols::InterfaceSymbolId,
    limits: InterfaceValidationLimits,
    encoded_directory: &[crate::InterfaceSemanticFactEntry],
    context: &mut SemanticDecodeContext,
    facts: &mut InterfaceSemanticFacts,
) -> Result<(), InterfaceValidationError> {
    let targets = required_section(sections, InterfaceSectionTag::TargetDependencies)?;

    surface::decode_target_dependencies(targets, limits, context, facts)?;

    validate_selected_fact_directory(
        facts,
        encoded_directory,
        owner,
        crate::InterfaceSemanticFactKind::TargetFact,
    )?;

    let owner = crate::InterfaceSymbolReference::Local(owner);

    retain_owner_target_dependencies(facts, &owner);

    Ok(())
}

fn retain_owner_constraints(
    facts: &mut InterfaceSemanticFacts,
    owner: &crate::InterfaceSymbolReference,
) {
    // Selected graphs retain shallow copies after validating the complete encoded table.
    facts.constraints = facts
        .constraints
        .iter()
        .filter(|constraint| &constraint.owner == owner)
        .cloned()
        .collect();

    facts.callable_contracts = Arc::from([]);
}

fn retain_owner_target_dependencies(
    facts: &mut InterfaceSemanticFacts,
    owner: &crate::InterfaceSymbolReference,
) {
    // Selected graphs retain shallow copies after validating the complete encoded table.
    facts.target_dependencies = facts
        .target_dependencies
        .iter()
        .filter(|dependency| &dependency.owner == owner)
        .cloned()
        .collect();

    facts.abi_dependencies = Arc::from([]);
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

fn validate_selected_fact_directory(
    facts: &InterfaceSemanticFacts,
    encoded_directory: &[crate::InterfaceSemanticFactEntry],
    owner: bray_symbols::InterfaceSymbolId,
    kind: crate::InterfaceSemanticFactKind,
) -> Result<(), InterfaceValidationError> {
    let owner = crate::InterfaceSymbolReference::Local(owner);

    let requested = encoded_directory
        .iter()
        .filter(|entry| entry.owner() == &owner && entry.kind() == kind);

    let decoded_directory = facts.fact_directory();

    let decoded = decoded_directory
        .iter()
        .filter(|entry| entry.owner() == &owner && entry.kind() == kind);

    if !requested.eq(decoded) {
        return Err(InterfaceValidationError::Malformed);
    }

    Ok(())
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

fn required_section<'bytes>(
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

    use super::super::test_support::{OwnedSection, owned_section_views};
    use super::decode_semantic_fact_graph;
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

        section_mut(&mut sections, InterfaceSectionTag::Implementations)
            .2
            .clear();

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
        assert_eq!(first, Err(InterfaceValidationError::Truncated));
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
