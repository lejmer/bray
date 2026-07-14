//! Backend-independent Bray mid-level intermediate representation.

#![forbid(unsafe_code)]

mod unit;

pub use unit::{MirBlock, MirBlockId, MirUnit, MirUnitBuildError, MirUnitBuilder};
