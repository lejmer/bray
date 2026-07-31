//! Driver command models and command-line input handling.

mod build;
mod cli;
mod compilation;
mod model;

pub(crate) use build::CliBuildCommand;
pub use build::{
    DriverBackend, DriverInspectionArtifact, DriverProductConfiguration, DriverRuntimeProfile,
    DriverRuntimeSelection,
};
pub use cli::DriverCliError;
pub(crate) use compilation::CliCompilationOptions;
pub use compilation::{DriverCompilationConfiguration, DriverDependencyInterface};
pub use model::{DriverCommand, DriverCommandKind, DriverInvocation, DriverOptions};
