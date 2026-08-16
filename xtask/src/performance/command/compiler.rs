use std::path::Path;
use std::process::Command;
use std::time::Instant;

use bray_compilation::CompilationProfileReport;

use super::super::compilation::external_invocation;
use super::super::model::ToolInvocationReport;

pub(super) struct TimedBrayCompilerRun {
    pub(super) elapsed_nanoseconds: u64,
    pub(super) invocation: ToolInvocationReport,
}

pub(super) fn run_timed(
    compiler: &Path,
    arguments: Vec<String>,
    description: &str,
) -> Result<TimedBrayCompilerRun, String> {
    let mut command = Command::new(compiler);

    command.args(&arguments);

    let started = Instant::now();

    crate::command::require_success(command, description)?;

    let elapsed_nanoseconds = super::super::peer::elapsed_nanoseconds(started);

    Ok(TimedBrayCompilerRun {
        elapsed_nanoseconds,
        invocation: external_invocation(
            crate::path::slash_separated(compiler),
            arguments,
            std::collections::BTreeMap::new(),
        ),
    })
}

pub(super) fn run_profiled(
    compiler: &Path,
    arguments: &[String],
    profile: &Path,
    description: &str,
) -> Result<CompilationProfileReport, String> {
    let mut command = Command::new(compiler);

    command.args(arguments);
    crate::command::require_success(command, description)?;

    let profile_bytes = std::fs::read(profile)
        .map_err(|error| format!("could not read {}: {error}", profile.display()))?;

    serde_json::from_slice(&profile_bytes)
        .map_err(|error| format!("could not decode compiler profile: {error}"))
}
