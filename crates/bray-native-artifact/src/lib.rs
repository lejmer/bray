//! Target-specific, content-addressed native artifact units.

mod index;
mod inspection;
mod lifecycle;
mod model;
mod resolver;
mod wire;

pub use index::{NativeArtifactIndex, NativeIndexError, ValidatedNativeArtifact};
pub use inspection::{
    alternate_name, scan_object_archive_summary, scan_object_unit_summary, summarize_native_unit,
};
pub use lifecycle::NativeStatic;
pub use model::{
    NativeCoRetentionGroup, NativeComdatSelection, NativeContentDigest, NativeDefinition,
    NativeDefinitionSelection, NativeRoot, NativeUnit, NativeUnitKind, NativeUnitSummary,
};
pub use resolver::{
    NativeResolutionError, NativeUnitInclusion, NativeUnitLocation, NativeUnitResolver,
    NativeUnitSelection,
};
pub use wire::WireError;
