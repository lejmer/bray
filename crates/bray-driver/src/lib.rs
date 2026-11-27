//! Loose-file compiler command orchestration used by `brayc`.

#![forbid(unsafe_code)]

mod command;
mod run;

pub use bray_tooling::{InspectionTarget, OutputFormat};
pub use command::{
    DriverBackend, DriverCliError, DriverCommand, DriverCommandKind,
    DriverCompilationConfiguration, DriverInspectionArtifact, DriverInvocation, DriverOptions,
    DriverProductConfiguration, DriverRuntimeProfile, DriverRuntimeSelection,
};
pub use run::{BuildPhase, DriverRunResult, run, run_build_request, run_result};
