//! Small foundational cancellation, collection, and text helpers shared across Bray compiler crates.

#![forbid(unsafe_code)]

mod cancellation;
mod digest;
mod slice;
mod text;

pub use cancellation::Cancellation;
pub use digest::StableDigestHasher;
pub use slice::{shared_slice, sorted_unique_shared_slice};
pub use text::{NonEmptySharedStr, shared_str};
