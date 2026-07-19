//! Lowering from checked semantic trees into Bray MIR.

#![forbid(unsafe_code)]

mod host;
mod input;

pub use host::ExecutableHostLoweringInput;
pub use input::{LoweringFactKind, LoweringFacts, LoweringInput, LoweringInputError};
