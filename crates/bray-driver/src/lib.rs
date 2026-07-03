//! User-facing command orchestration for the Bray compiler.

#![forbid(unsafe_code)]

mod cli;
mod command;
mod diagnostic_output;
mod exit_status;
mod file_arguments;
mod run;
mod terminal_style;
#[cfg(test)]
mod test_support;

pub use cli::DriverCliError;
pub use command::{
    DriverCommand, DriverCommandKind, DriverInvocation, DriverOptions, DriverOutputFormat,
};
pub use file_arguments::{
    DriverSourceInputError, compilation_request_from_file_arguments,
    source_inputs_from_file_arguments,
};
pub use run::{DriverRunResult, run, run_result};
