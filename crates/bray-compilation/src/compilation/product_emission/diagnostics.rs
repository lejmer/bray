mod common;
mod evaluation;
mod linking;
mod model;
mod planning;
pub(in crate::compilation) mod product_query;
mod routing;
mod terminal;

pub use model::{ProductEmissionError, ProductEmissionErrorKind};
