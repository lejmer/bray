use std::ffi::OsString;
use std::process::ExitCode;

use crate::run_tack;

/// Runs the Bray Tack command.
pub fn run(arguments: impl IntoIterator<Item = OsString>) -> ExitCode {
    run_tack(arguments)
}
