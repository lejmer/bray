//! Standard library identities, source authority, and artifact contracts.

#![forbid(unsafe_code)]

mod identity;
mod manifest;
mod source;

pub use identity::{
    PUBLIC_STANDARD_LIBRARY_PACKAGE_IDENTITY, PUBLIC_STANDARD_LIBRARY_PRODUCT_IDENTITY,
    PUBLIC_STANDARD_LIBRARY_SURFACE_IDENTITY, is_public_standard_library_package,
    is_reserved_standard_library_package,
};
pub use source::PackageSourceAuthority;
pub use manifest::{
    STANDARD_LIBRARY_MANIFEST_FILE_NAME, StandardLibraryArtifact,
    StandardLibraryArtifactDigest, StandardLibraryArtifactKind, StandardLibraryBundleDigest,
    StandardLibraryBundleManifest, StandardLibraryManifestError, StandardLibraryTargetArtifacts,
    decode_standard_library_manifest, encode_standard_library_manifest,
};
