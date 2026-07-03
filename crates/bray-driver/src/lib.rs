//! User-facing command orchestration for the Bray compiler.

#![forbid(unsafe_code)]

mod file_arguments;
mod run;

pub use file_arguments::{
    DriverSourceInputError, compilation_request_from_file_arguments,
    source_inputs_from_file_arguments,
};
pub use run::run;
