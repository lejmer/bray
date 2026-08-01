//! Immutable standard library bundle inventory and canonical wire encoding.

mod codec;
mod model;
mod wire;

pub use codec::{decode_standard_library_manifest, encode_standard_library_manifest};
pub use model::{
    STANDARD_LIBRARY_MANIFEST_FILE_NAME, StandardLibraryArtifact, StandardLibraryArtifactDigest,
    StandardLibraryArtifactKind, StandardLibraryBundleDigest, StandardLibraryBundleManifest,
    StandardLibraryManifestError, StandardLibraryTargetArtifacts,
};
