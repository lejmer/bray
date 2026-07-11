mod common;
mod contract;
mod directory;
mod surface;
mod value;

use crate::semantic::codec::common::SemanticDecodeContext;
use crate::{
    InterfaceSectionTag, InterfaceSemanticFacts, InterfaceValidationError,
    InterfaceValidationLimits, ValidatedInterfaceSection,
};

pub fn decode_semantic_facts(
    sections: &[ValidatedInterfaceSection<'_>],
    symbol_count: usize,
    dependency_count: usize,
    limits: InterfaceValidationLimits,
) -> Result<InterfaceSemanticFacts, InterfaceValidationError> {
    validate_decode_allocation(sections, limits)?;

    let mut context = SemanticDecodeContext::new(limits);

    let directory = required_section(sections, InterfaceSectionTag::SymbolFactDirectory)?;
    let types = required_section(sections, InterfaceSectionTag::SemanticTypes)?;
    let constants = required_section(sections, InterfaceSectionTag::Constants)?;
    let contracts = required_section(sections, InterfaceSectionTag::Contracts)?;
    let implementations = required_section(sections, InterfaceSectionTag::Implementations)?;
    let targets = required_section(sections, InterfaceSectionTag::TargetDependencies)?;
    let provenance = optional_section(sections, InterfaceSectionTag::SourceProvenance);

    let mut facts = value::decode_types(types, limits, &mut context)?;

    value::decode_constants(constants, limits, &mut context, &mut facts)?;
    contract::decode_contracts(contracts, limits, &mut context, &mut facts)?;
    surface::decode_implementations(implementations, limits, &mut context, &mut facts)?;
    surface::decode_target_dependencies(targets, limits, &mut context, &mut facts)?;

    if let Some(provenance) = provenance {
        surface::decode_provenance(provenance, limits, &mut context, &mut facts)?;
    }

    let encoded_directory = directory::decode_fact_directory(directory, limits, &mut context)?;

    if encoded_directory.as_ref() != facts.fact_directory().as_ref() {
        return Err(InterfaceValidationError::Malformed);
    }

    facts.validate(symbol_count, dependency_count, limits)?;

    Ok(facts)
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
                    | InterfaceSectionTag::Implementations
                    | InterfaceSectionTag::TargetDependencies
                    | InterfaceSectionTag::SourceProvenance
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
