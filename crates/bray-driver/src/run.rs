//! Driver command execution.

mod build;
mod execute;
mod status;

pub(crate) use build::run_build_command;
pub use execute::{DriverRunResult, run, run_result};
pub(crate) use status::exit_code_from_diagnostics;
