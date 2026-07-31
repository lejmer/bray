//! Loose-file compiler command orchestration used by `brayc`.

#![forbid(unsafe_code)]

mod command;
mod run;
#[cfg(test)]
mod test_support;

pub use bray_tooling::{InspectionTarget, OutputFormat};
pub use command::{
    DriverBackend, DriverCliError, DriverCommand, DriverCommandKind, DriverInspectionArtifact,
    DriverInvocation, DriverOptions, DriverProductConfiguration,
    DriverRuntimeProfile, DriverRuntimeSelection,
};
pub use run::{DriverRunResult, run, run_result};
