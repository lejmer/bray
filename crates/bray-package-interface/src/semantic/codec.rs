mod coherence;
mod common;
mod decoding;
mod encoding;
mod record;

pub(super) use common::{
    DECLARATION_SEMANTICS_FORMAT_VERSION, DECLARATION_TEMPLATE_FORMAT_VERSION,
};
pub(crate) use common::{SemanticDecodeContext, read_symbol_reference, write_symbol_reference};
pub use decoding::decode_semantics;
pub(crate) use decoding::{
    COMPLETE_SEMANTIC_SECTIONS, decode_inspection_records, decode_selected_semantic_graph,
    decode_semantic_graph, decode_template_payload, selected_semantic_sections,
    validate_decode_allocation,
};
pub use encoding::{EncodedSemanticSection, encode_semantics};
pub(crate) use encoding::{encode_template_payload, encode_validated_semantics};
