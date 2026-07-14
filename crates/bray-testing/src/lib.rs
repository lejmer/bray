//! Shared test infrastructure for Bray compiler tests.

#![forbid(unsafe_code)]

#[cfg(feature = "bound-unit")]
mod bound_unit;
mod source;
mod syntax;

#[cfg(feature = "bound-unit")]
pub use bound_unit::test_bound_unit;
pub use source::{test_source_at, test_source_snapshot, test_source_store, try_test_source_store};
pub use syntax::{assert_single_final_eof, assert_tokens_cover_source_text};
