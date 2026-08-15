use super::bundle::section;
use super::model::EncodedSemanticSection;
use crate::semantic::codec::common::write_symbol_reference;
use crate::tag::WireTag;
use crate::wire::WireEncoder;
use crate::{InterfaceSectionTag, InterfaceSemantics};

pub(super) fn encode_semantic_directory(semantics: &InterfaceSemantics) -> EncodedSemanticSection {
    let entries = semantics.semantic_directory();
    let mut encoder = WireEncoder::new();

    for entry in &*entries {
        write_symbol_reference(&mut encoder, entry.owner());
        encoder.write_u32(entry.kind().to_wire());
        encoder.write_u32(entry.section().wire_value());
        encoder.write_u32(entry.record());
    }

    section(
        InterfaceSectionTag::SemanticRecordDirectory,
        entries.len(),
        encoder,
    )
}
