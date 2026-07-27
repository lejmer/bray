//! User-facing command orchestration for the Bray compiler.

#![forbid(unsafe_code)]

mod command;
mod inspection;
mod output;
mod run;
#[cfg(test)]
mod test_support;

pub use command::{
    BoundInspectionTarget, DriverCliError, DriverCommand, DriverCommandKind, DriverInvocation,
    DriverOptions, DriverOutputFormat, DriverSourceInputError,
    compilation_request_from_file_arguments, source_inputs_from_file_arguments,
};
pub use run::{DriverRunResult, run, run_result};
