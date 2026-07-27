//! Driver command models and command-line input handling.

mod cli;
mod file_arguments;
mod model;

pub use cli::DriverCliError;
pub use file_arguments::{
    DriverSourceInputError, compilation_request_from_file_arguments,
    source_inputs_from_file_arguments,
};
pub use model::{
    BoundInspectionTarget, DriverCommand, DriverCommandKind, DriverInvocation, DriverOptions,
    DriverOutputFormat,
};
