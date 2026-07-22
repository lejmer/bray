use super::{
    contract, declaration, directory, model::EncodedSemanticSection, support, surface, template,
    value,
};
use crate::wire::WireEncoder;
use crate::{
    InterfaceSectionTag, InterfaceSemanticFacts, InterfaceValidationError,
    InterfaceValidationLimits, PackageInterfaceSurface,
};

/// Encodes one validated semantic fact bundle into canonical interface sections.
///
/// Returns an error when the facts violate the interface surface or resource limits.
pub fn encode_semantic_facts(
    facts: &InterfaceSemanticFacts,
    surface: &PackageInterfaceSurface,
    limits: InterfaceValidationLimits,
) -> Result<Vec<EncodedSemanticSection>, InterfaceValidationError> {
    facts.validate(surface, limits)?;

    Ok(encode_validated_semantic_facts(facts))
}

pub(crate) fn encode_validated_semantic_facts(
    facts: &InterfaceSemanticFacts,
) -> Vec<EncodedSemanticSection> {
    vec![
        directory::encode_fact_directory(facts),
        value::encode_types(facts),
        value::encode_constants(facts),
        contract::encode_contracts(facts),
        declaration::encode_declaration_facts(facts),
        template::encode_templates(facts),
        surface::encode_implementations(facts),
        surface::encode_target_dependencies(facts),
        surface::encode_provenance(facts),
        support::encode_support_graph(facts),
    ]
}

pub(super) fn section(
    tag: InterfaceSectionTag,
    record_count: usize,
    encoder: WireEncoder,
) -> EncodedSemanticSection {
    EncodedSemanticSection::new(tag, record_count, encoder.into_bytes())
}
