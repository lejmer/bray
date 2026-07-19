//! Shared test infrastructure for Bray compiler tests.

#![forbid(unsafe_code)]

#[cfg(feature = "bound-unit")]
mod bound_unit;
#[cfg(feature = "mir-unit")]
mod mir_unit;
mod source;
mod syntax;
mod temporary_file;

#[cfg(feature = "bound-unit")]
pub use bound_unit::test_bound_unit;
#[cfg(feature = "mir-unit")]
pub use mir_unit::{test_mir_target, test_mir_type, test_mir_unit};
pub use source::{test_source_at, test_source_snapshot, test_source_store, try_test_source_store};
pub use syntax::{
    assert_single_final_eof, assert_tokens_cover_source_text, first_syntax_descendant,
    syntax_descendants,
};
pub use temporary_file::{TemporaryFile, unique_temporary_directory};
