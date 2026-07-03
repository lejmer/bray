//! User-facing command orchestration for the Bray compiler.

#![forbid(unsafe_code)]

mod cli;
mod command;
mod file_arguments;
mod run;

pub use cli::DriverCliError;
pub use command::{
    DriverCommand, DriverCommandKind, DriverInvocation, DriverOptions, DriverOutputFormat,
};
pub use file_arguments::{
    DriverSourceInputError, compilation_request_from_file_arguments,
    source_inputs_from_file_arguments,
};
pub use run::run;
