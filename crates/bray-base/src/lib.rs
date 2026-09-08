//! Small foundational filesystem, cancellation, collection, and text helpers shared across Bray
//! compiler crates.

#![forbid(unsafe_code)]

mod cancellation;
mod digest;
mod directory;
mod file_staging;
mod graph;
mod hex;
mod path;
mod slice;
mod text;

pub use cancellation::Cancellation;
pub use digest::{Sha256Reader, StableDigestHasher, sha256_file, sha256_reader};
pub use directory::{
    atomic_rename_exclusive, atomic_rename_exclusive_is_supported, retry_permission_denied,
    sync_directory,
};
pub use file_staging::{
    CompletedStagedFile, FileReplacementMode, StagedFile, is_staged_file_name,
    write_file_atomically,
};
pub use graph::strongly_connected_components;
pub use hex::{decode_lowercase_hex, is_lowercase_hex, lowercase_hex};
pub use path::is_canonical_relative_path;
pub use slice::{shared_slice, sorted_unique_shared_slice};
pub use text::{NonEmptySharedStr, shared_str};
