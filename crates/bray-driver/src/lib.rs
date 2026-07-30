//! User-facing command orchestration for the Bray compiler.

#![forbid(unsafe_code)]

mod command;
mod inspection;
mod output;
mod run;
mod tack;
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
pub use tack::{
    TackCliError, TackCommandKind, TackFormatInput, TackFormatMode, TackFormatRequest,
    TackFormatService, TackInvocation, TackLanguageServerRequest,
    TackLanguageServerService, TackRunResult,
    TackServiceResult, TackServices, run_tack, run_tack_result,
    run_tack_with_services,
};
