mod context;
mod error;
mod generator;
mod heap;
mod lifecycle;
mod numeric;

pub use context::{DestructionBody, SyntheticLoweringContext};
pub use error::SyntheticLoweringError;
pub(crate) use generator::GeneratorIterationLowerer;
pub use generator::lower_generator_callable;
pub use heap::{HeapStorageLoweringInput, HeapStorageMethod, lower_heap_storage};
pub use lifecycle::{expand_lifecycle_continuations, lower_lifecycle};

pub(crate) use context::SyntheticLowerer;

pub use numeric::lower_numeric_callable;
