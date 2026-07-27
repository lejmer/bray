//! User-facing command orchestration for the Bray compiler.

#![forbid(unsafe_code)]

mod cli;
mod command;
mod declaration_inspection;
mod diagnostic_output;
mod exit_status;
mod file_arguments;
mod inspection;
mod output_path;
mod run;
mod source_inspection;
mod source_location_output;
mod source_origin_output;
mod syntax_inspection;
mod terminal_style;
#[cfg(test)]
mod test_support;
mod token_inspection;

pub use cli::DriverCliError;
pub use command::{
    DriverCommand, DriverCommandKind, DriverInvocation, DriverOptions, DriverOutputFormat,
};
pub use file_arguments::{
    DriverSourceInputError, compilation_request_from_file_arguments,
    source_inputs_from_file_arguments,
};
pub use run::{DriverRunResult, run, run_result};
