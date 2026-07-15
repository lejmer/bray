mod contract;
mod directory;
mod model;
mod support;
mod surface;
mod template;
mod value;

pub use model::EncodedSemanticSection;

use crate::wire::WireEncoder;
use crate::{
    InterfaceSectionTag, InterfaceSemanticFacts, InterfaceValidationError,
    InterfaceValidationLimits, PackageInterfaceSurface,
};

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
        template::encode_templates(facts),
        surface::encode_implementations(facts),
        surface::encode_target_dependencies(facts),
        surface::encode_provenance(facts),
        support::encode_support_graph(facts),
    ]
}

fn section(
    tag: InterfaceSectionTag,
    record_count: usize,
    encoder: WireEncoder,
) -> EncodedSemanticSection {
    EncodedSemanticSection::new(tag, record_count, encoder.into_bytes())
}
