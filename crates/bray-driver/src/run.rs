//! Driver command execution.

mod build;
mod execute;
mod output;

pub(crate) use build::run_build_command;
pub use execute::{DriverRunResult, run, run_result};
pub(crate) use output::{write_driver_output, write_driver_output_error};
