pub(in crate::compilation) mod diagnostics;
mod execution;
mod publishing;

pub use diagnostics::{ProductEmissionError, ProductEmissionErrorKind};
pub use execution::ProductEmissionInputs;
