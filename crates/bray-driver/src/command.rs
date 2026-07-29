//! Driver command models and command-line input handling.

mod build;
mod cli;
mod file_arguments;
mod model;

pub use build::{
    DriverBackend, DriverInspectionArtifact, DriverProductConfiguration, DriverRuntimeSelection,
    DriverRuntimeProfile, DriverTarget,
};
pub(crate) use build::CliBuildCommand;
pub use cli::DriverCliError;
pub use file_arguments::{
    DriverSourceInputError, compilation_request_from_file_arguments,
    source_inputs_from_file_arguments,
};
pub use model::{
    DriverCommand, DriverCommandKind, DriverInvocation, DriverOptions, DriverOutputFormat,
    UnitInspectionTarget,
};
