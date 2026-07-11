use super::model::EncodedSemanticSection;
use super::section;
use crate::semantic::codec::common::write_symbol_reference;
use crate::tag::WireTag;
use crate::wire::WireEncoder;
use crate::{InterfaceSectionTag, InterfaceSemanticFacts};

pub(super) fn encode_fact_directory(facts: &InterfaceSemanticFacts) -> EncodedSemanticSection {
    let entries = facts.fact_directory();
    let mut encoder = WireEncoder::new();

    for entry in &*entries {
        write_symbol_reference(&mut encoder, entry.owner());
        encoder.write_u32(entry.kind().to_wire());
        encoder.write_u32(entry.section().wire_value());
        encoder.write_u32(entry.record());
    }

    section(
        InterfaceSectionTag::SymbolFactDirectory,
        entries.len(),
        encoder,
    )
}
