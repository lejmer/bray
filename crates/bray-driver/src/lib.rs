//! User-facing command orchestration for the Bray compiler.

#![forbid(unsafe_code)]

use std::ffi::OsString;
use std::process::ExitCode;

/// Runs the Bray compiler driver for the provided process arguments.
pub fn run(_arguments: impl IntoIterator<Item = OsString>) -> ExitCode {
    ExitCode::SUCCESS
}
