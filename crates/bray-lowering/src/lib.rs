//! Lowering from checked semantic trees into Bray IR.

#![forbid(unsafe_code)]

mod input;
#[cfg(test)]
mod test_support;
mod unit;

pub use input::{LoweringInput, LoweringInputError};
pub use unit::{
    LoweredBlock, LoweredBlockId, LoweredUnit, LoweredUnitBuildError, LoweredUnitBuilder,
};
