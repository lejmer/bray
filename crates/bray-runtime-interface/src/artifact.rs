//! Target runtime artifact catalogs and exact product selections.

mod catalog;
mod selection;

pub use catalog::{
    RuntimeArtifactComponentMetadata, RuntimeArtifactDigest, RuntimeArtifactMetadata,
    RuntimeArtifactMetadataBuildError, RuntimeArtifactMetadataDecodeError,
    RuntimeArtifactMetadataEncodeError, RuntimeArtifactPurpose,
};
pub use selection::{
    RuntimeArtifact, RuntimeArtifactBuildError, RuntimeArtifactComponent, RuntimeArtifactSelection,
    RuntimeArtifactSelectionError,
};
