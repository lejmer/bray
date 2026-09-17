//! Driver command execution.

mod build;
mod diagnostic;
mod execute;
mod output;
mod runtime;

pub use build::run_build_request;
pub use execute::{DriverRunResult, run, run_result};
pub(crate) use output::{write_driver_output, write_driver_output_error};
