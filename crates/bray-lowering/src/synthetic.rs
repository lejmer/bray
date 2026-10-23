mod context;
mod error;
mod heap;
mod lifecycle;

pub use context::{DestructionBody, SyntheticLoweringContext};
pub use error::SyntheticLoweringError;
pub use heap::{HeapStorageLoweringInput, HeapStorageMethod, lower_heap_storage};
pub use lifecycle::{expand_lifecycle_continuations, lower_lifecycle};

pub(crate) use context::SyntheticLowerer;
