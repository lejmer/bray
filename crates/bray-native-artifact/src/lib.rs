//! Target-specific, content-addressed native artifact units.

mod index;
mod inspection;
mod model;
mod resolver;
mod wire;

pub use index::{NativeArtifactIndex, NativeIndexError, ValidatedNativeArtifact};
pub use inspection::{scan_bitcode_unit_summary, scan_object_unit_summary};
pub use resolver::{NativeResolutionError, NativeUnitInclusion, NativeUnitResolver, NativeUnitSelection};
pub use wire::WireError;
pub use model::{
    NativeCoRetentionGroup, NativeComdatSelection, NativeContentDigest, NativeDefinition,
    NativeDefinitionSelection, NativeRoot, NativeUnit, NativeUnitKind, NativeUnitSummary,
};
