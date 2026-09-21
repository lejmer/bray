//! Target-specific, content-addressed native artifact units.

mod index;
mod model;
mod wire;

pub use index::{NativeArtifactIndex, NativeIndexError, ValidatedNativeArtifact};
pub use model::{
    NativeCoRetentionGroup, NativeComdatSelection, NativeContentDigest, NativeDefinition,
    NativeDefinitionSelection, NativeRoot, NativeUnit, NativeUnitKind, NativeUnitSummary,
};
