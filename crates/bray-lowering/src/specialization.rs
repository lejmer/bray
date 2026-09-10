mod destruction;
mod destructor;
mod future;
mod lifecycle;
mod outcome;
mod task;

pub use destruction::specialize_destruction_body;
pub use destructor::specialize_destructor_body;
pub use lifecycle::specialize_lifecycle_execution;
