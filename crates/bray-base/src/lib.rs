//! Small foundational filesystem, cancellation, collection, and text helpers shared across Bray
//! compiler crates.

#![forbid(unsafe_code)]

mod cancellation;
mod digest;
mod file_staging;
mod slice;
mod text;

pub use cancellation::Cancellation;
pub use digest::StableDigestHasher;
pub use file_staging::{CompletedStagedFile, FileReplacementMode, StagedFile};
pub use slice::{shared_slice, sorted_unique_shared_slice};
pub use text::{NonEmptySharedStr, shared_str};
