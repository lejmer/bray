use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

use bray_compilation::CompilationOptions;

use crate::file_arguments::compilation_request_from_file_arguments;

/// Runs the Bray compiler driver for the provided process arguments.
pub fn run(arguments: impl IntoIterator<Item = OsString>) -> ExitCode {
    let file_arguments = arguments.into_iter().skip(1).map(PathBuf::from);

    match compilation_request_from_file_arguments(file_arguments, CompilationOptions::default()) {
        Ok(_) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}
