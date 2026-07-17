//! Backend-independent Bray mid-level intermediate representation.

#![forbid(unsafe_code)]

mod execution;
mod unit;

pub use execution::{MirExecutionOperationKind, MirUnitExecution};
pub use unit::{
    MirBlock, MirBlockId, MirSourceOrigin, MirUnit, MirUnitBuildError, MirUnitBuilder, MirUnitId,
    MirUnitKey,
};
