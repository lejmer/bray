#[macro_use]
mod macros;
mod callback;
mod event;
mod export;
mod frame;
mod host;
#[doc(hidden)]
pub mod implementation;
mod state;
mod static_finalizer;
mod task_observation;
mod workers;

pub(crate) use state::{RetainedRuntime, retain_runtime};
pub(crate) use static_finalizer::{
    run_static_finalizer, with_retained_static_cleanup_runtime, with_static_cleanup_runtime,
};
