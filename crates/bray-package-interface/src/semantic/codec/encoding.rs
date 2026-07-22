mod contract;
mod directory;
mod facts;
mod model;
mod support;
mod surface;
mod template;
mod value;

use facts::section;

pub use facts::encode_semantic_facts;
pub(crate) use facts::encode_validated_semantic_facts;
pub use model::EncodedSemanticSection;
