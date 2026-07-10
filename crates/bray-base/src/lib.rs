//! Small foundational helpers shared across Bray compiler crates.

#![forbid(unsafe_code)]

mod slice;
mod text;

pub use slice::{shared_slice, sorted_unique_shared_slice};
pub use text::shared_str;
