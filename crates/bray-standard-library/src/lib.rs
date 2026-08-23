//! Standard library identities, source authority, and artifact contracts.

#![forbid(unsafe_code)]

mod identity;
mod manifest;
mod resolver;
mod root;
mod source;

pub use identity::{
    PUBLIC_STANDARD_LIBRARY_PACKAGE_IDENTITY, PUBLIC_STANDARD_LIBRARY_PRODUCT_IDENTITY,
    PUBLIC_STANDARD_LIBRARY_SURFACE_IDENTITY, is_public_standard_library_package,
    is_reserved_standard_library_package,
};
#[cfg(any(test, feature = "test-support"))]
pub use manifest::target_artifacts_for_test;
pub use manifest::{
    STANDARD_LIBRARY_MANIFEST_FILE_NAME, StandardLibraryArtifact, StandardLibraryArtifactDigest,
    StandardLibraryArtifactKind, StandardLibraryBundleDigest, StandardLibraryBundleManifest,
    StandardLibraryManifestError, StandardLibraryOptimizationCompatibility,
    StandardLibraryOptimizationDependency, StandardLibraryOptimizationFallback,
    StandardLibraryOptimizationLifecycleRoot, StandardLibraryOptimizationMetadata,
    StandardLibraryOptimizationMetadataProblem, StandardLibraryOptimizationProducer,
    StandardLibraryOptimizationProducerKind, StandardLibraryOptimizationSemantics,
    StandardLibraryTargetArtifacts, decode_standard_library_manifest,
    encode_standard_library_manifest, standard_library_target_artifact_directory,
};
pub use resolver::{
    ResolvedStandardLibraryArtifact, StandardLibraryLoadError, StandardLibraryResolver,
};
pub use root::StandardLibraryRoot;
pub use source::PackageSourceAuthority;
