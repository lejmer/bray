//! Lowering from checked semantic trees into Bray MIR.

#![forbid(unsafe_code)]

mod input;

pub use input::{LoweringInput, LoweringInputError};
