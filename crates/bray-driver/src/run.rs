use std::ffi::OsString;
use std::process::ExitCode;

use crate::command::DriverInvocation;

/// Runs the Bray compiler driver for the provided process arguments.
pub fn run(arguments: impl IntoIterator<Item = OsString>) -> ExitCode {
    let invocation = match DriverInvocation::try_from_arguments(arguments) {
        Ok(invocation) => invocation,
        Err(error) => return error.exit_code(),
    };

    match invocation.into_compilation_request() {
        Ok(_) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}
