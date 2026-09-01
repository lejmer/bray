mod common;
mod evaluation;
pub(in crate::compilation) mod foreign_query;
mod linking;
mod model;
mod planning;
pub(in crate::compilation) mod product_query;
mod routing;
mod terminal;

pub use model::{ProductEmissionError, ProductEmissionErrorKind};
pub(crate) use evaluation::diagnostic_evaluation_failure;
