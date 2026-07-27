//! Driver command execution.

mod execute;
mod status;

pub use execute::{DriverRunResult, run, run_result};
pub(crate) use status::exit_code_from_diagnostics;
