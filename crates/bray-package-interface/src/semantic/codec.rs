mod coherence;
mod common;
mod decoding;
mod encoding;
mod record;

const DECLARATION_FACT_FORMAT_VERSION: u32 = 2;
const DECLARATION_TEMPLATE_FORMAT_VERSION: u32 = 1;

pub use decoding::decode_semantic_facts;
pub(crate) use decoding::{
    decode_inspection_records, decode_semantic_fact_graph, validate_decode_allocation,
};
pub(crate) use encoding::encode_validated_semantic_facts;
pub use encoding::{EncodedSemanticSection, encode_semantic_facts};
