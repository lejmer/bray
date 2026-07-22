mod coherence;
mod common;
mod decoding;
mod encoding;
mod record;

pub use decoding::decode_semantic_facts;
pub(crate) use decoding::{decode_inspection_records, decode_semantic_fact_graph};
pub(crate) use encoding::encode_validated_semantic_facts;
pub use encoding::{EncodedSemanticSection, encode_semantic_facts};
