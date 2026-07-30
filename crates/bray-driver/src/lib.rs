//! User-facing command orchestration for the Bray compiler.

#![forbid(unsafe_code)]

mod command;
mod inspection;
mod output;
mod run;
#[cfg(test)]
mod test_support;

pub use command::{
    DriverBackend, DriverCliError, DriverCommand, DriverCommandKind, DriverInspectionArtifact,
    DriverInvocation, DriverOptions, DriverOutputFormat, DriverProductConfiguration,
    DriverRuntimeProfile, DriverRuntimeSelection, DriverSourceInputError, DriverTarget,
    UnitInspectionTarget, compilation_request_from_file_arguments,
    source_inputs_from_file_arguments,
};
pub use run::{DriverRunResult, run, run_result};
