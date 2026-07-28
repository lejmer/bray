//! Lowering from checked semantic trees into Bray MIR.

#![forbid(unsafe_code)]

mod host;
mod input;
mod lowering;
mod result;

pub use host::ExecutableHostLoweringInput;
pub use input::{LoweringFactKind, LoweringInput, LoweringInputError};
pub use lowering::{LoweringError, lower_unit};
pub use result::{CompileTimeUnit, LoweredUnit};
