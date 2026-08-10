mod coherence;
mod common;
mod decoding;
mod encoding;
mod record;

pub(super) use common::{DECLARATION_FACT_FORMAT_VERSION, DECLARATION_TEMPLATE_FORMAT_VERSION};
pub(crate) use common::{SemanticDecodeContext, read_symbol_reference, write_symbol_reference};
pub use decoding::decode_semantic_facts;
pub(crate) use decoding::{
    decode_inspection_records, decode_selected_semantic_fact_graph, decode_semantic_fact_graph,
    decode_template_payload, selected_fact_sections, validate_decode_allocation,
};
pub use encoding::{EncodedSemanticSection, encode_semantic_facts};
pub(crate) use encoding::{encode_template_payload, encode_validated_semantic_facts};
