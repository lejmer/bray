pub(in crate::compilation) mod diagnostics;
mod execution;

pub use diagnostics::{ProductEmissionError, ProductEmissionErrorKind};
pub use execution::ProductEmissionInputs;
