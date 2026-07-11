mod contract;
mod directory;
mod model;
mod surface;
mod value;

pub use model::EncodedSemanticSection;

use crate::wire::WireEncoder;
use crate::{
    InterfaceSectionTag, InterfaceSemanticFacts, InterfaceValidationError,
    InterfaceValidationLimits,
};

pub fn encode_semantic_facts(
    facts: &InterfaceSemanticFacts,
    symbol_count: usize,
    dependency_count: usize,
    limits: InterfaceValidationLimits,
) -> Result<Vec<EncodedSemanticSection>, InterfaceValidationError> {
    facts.validate(symbol_count, dependency_count, limits)?;

    Ok(vec![
        directory::encode_fact_directory(facts),
        value::encode_types(facts),
        value::encode_constants(facts),
        contract::encode_contracts(facts),
        surface::encode_implementations(facts),
        surface::encode_target_dependencies(facts),
        surface::encode_provenance(facts),
    ])
}

fn section(
    tag: InterfaceSectionTag,
    record_count: usize,
    encoder: WireEncoder,
) -> EncodedSemanticSection {
    EncodedSemanticSection::new(tag, record_count, encoder.into_bytes())
}
