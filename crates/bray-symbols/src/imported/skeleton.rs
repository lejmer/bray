mod build;
mod error;
mod input;
mod records;
mod snapshot;

pub use error::ImportedSymbolSkeletonBuildError;
pub use input::{ImportedLookupEdge, ImportedSymbolRelationship, ImportedSymbolSkeletonInput};
// rust-style: allow(wildcard-import, reason = "the macro-generated skeleton surface has no maintainable explicit symbol inventory")
pub use snapshot::*;

#[cfg(any(test, feature = "test-support"))]
pub(crate) mod test_support;
