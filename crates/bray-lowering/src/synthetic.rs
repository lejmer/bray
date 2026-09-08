mod context;
mod error;
mod frame;
mod heap;
mod lifecycle;
mod memory;
mod task;

pub use context::SyntheticLoweringContext;
pub use error::SyntheticLoweringError;
pub use heap::{HeapStorageLoweringInput, HeapStorageMethod, lower_heap_storage};
pub use lifecycle::lower_lifecycle;
pub use task::{TaskObservationMethod, lower_task_observation};

pub(crate) use context::SyntheticLowerer;
