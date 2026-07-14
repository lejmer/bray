//! Lowering from checked semantic trees into Bray MIR.

#![forbid(unsafe_code)]

mod input;
#[cfg(test)]
mod test_support;

pub use input::{LoweringInput, LoweringInputError};
