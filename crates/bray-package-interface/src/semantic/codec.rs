mod common;
mod decoding;
mod encoding;

pub(crate) use decoding::decode_semantic_fact_graph;
pub use decoding::decode_semantic_facts;
pub(crate) use encoding::encode_validated_semantic_facts;
pub use encoding::{EncodedSemanticSection, encode_semantic_facts};
