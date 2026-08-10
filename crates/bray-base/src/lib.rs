//! Small foundational filesystem, cancellation, collection, and text helpers shared across Bray
//! compiler crates.

#![forbid(unsafe_code)]

mod cancellation;
mod digest;
mod directory;
mod file_staging;
mod hex;
mod path;
mod slice;
mod text;

pub use cancellation::Cancellation;
pub use digest::{StableDigestHasher, sha256_file};
pub use directory::{
    atomic_rename_exclusive, atomic_rename_exclusive_is_supported, sync_directory,
};
pub use file_staging::{CompletedStagedFile, FileReplacementMode, StagedFile};
pub use hex::{decode_lowercase_hex, is_lowercase_hex, lowercase_hex};
pub use path::is_canonical_relative_path;
pub use slice::{shared_slice, sorted_unique_shared_slice};
pub use text::{NonEmptySharedStr, shared_str};
