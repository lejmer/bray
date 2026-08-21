//! Immutable standard library bundle inventory and canonical wire encoding.

mod codec;
mod model;
mod optimization;
mod wire;

pub use codec::{decode_standard_library_manifest, encode_standard_library_manifest};
pub use model::{
    STANDARD_LIBRARY_MANIFEST_FILE_NAME, StandardLibraryArtifact, StandardLibraryArtifactDigest,
    StandardLibraryArtifactKind, StandardLibraryBundleDigest, StandardLibraryBundleManifest,
    StandardLibraryManifestError, StandardLibraryTargetArtifacts,
    standard_library_target_artifact_directory,
};
#[cfg(any(test, feature = "test-support"))]
pub use model::target_artifacts_for_test;
#[cfg(test)]
pub(crate) use model::TEST_OPTIMIZATION_ARTIFACT_BYTES;
pub use optimization::{
    StandardLibraryOptimizationCompatibility, StandardLibraryOptimizationDependency,
    StandardLibraryOptimizationFallback, StandardLibraryOptimizationLifecycleRoot,
    StandardLibraryOptimizationMetadata,
    StandardLibraryOptimizationProducer, StandardLibraryOptimizationProducerKind,
    StandardLibraryOptimizationSemantics,
};
