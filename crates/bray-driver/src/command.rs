//! Driver command models and command-line input handling.

mod build;
mod cli;
mod model;

pub use build::{
    DriverBackend, DriverInspectionArtifact, DriverProductConfiguration, DriverRuntimeSelection,
    DriverRuntimeProfile,
};
pub(crate) use build::CliBuildCommand;
pub use cli::DriverCliError;
pub use model::{
    DriverCommand, DriverCommandKind, DriverInvocation, DriverOptions,
};
