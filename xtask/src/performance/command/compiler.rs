use std::path::Path;
use std::process::Command;
use std::time::Instant;

use bray_compilation::CompilationProfileReport;

use super::super::compilation::external_invocation;
use super::super::model::ToolInvocationReport;

#[expect(
    clippy::too_many_arguments,
    reason = "the arguments record the exact application compiler inputs"
)]
pub(super) fn application_arguments(
    package: &str,
    product: &str,
    source: &Path,
    output: &Path,
    target: bray_target::NativeTarget,
    standard_library: &Path,
    runtime: &Path,
    profile: Option<&Path>,
    linker_map: Option<&Path>,
) -> Vec<String> {
    arguments(BuildArguments {
        package,
        product,
        product_kind: "executable",
        artifact: "executable",
        source,
        output,
        target,
        standard_library: Some(standard_library),
        runtime: Some(runtime),
        profile,
        linker_map,
    })
}

pub(super) fn library_arguments(
    package: &str,
    product: &str,
    source: &Path,
    output: &Path,
    target: bray_target::NativeTarget,
    profile: Option<&Path>,
) -> Vec<String> {
    arguments(BuildArguments {
        package,
        product,
        product_kind: "library",
        artifact: "relocatable-object",
        source,
        output,
        target,
        standard_library: None,
        runtime: None,
        profile,
        linker_map: None,
    })
}

struct BuildArguments<'input> {
    package: &'input str,
    product: &'input str,
    product_kind: &'input str,
    artifact: &'input str,
    source: &'input Path,
    output: &'input Path,
    target: bray_target::NativeTarget,
    standard_library: Option<&'input Path>,
    runtime: Option<&'input Path>,
    profile: Option<&'input Path>,
    linker_map: Option<&'input Path>,
}

fn arguments(input: BuildArguments<'_>) -> Vec<String> {
    let mut arguments = Vec::new();

    if let Some(profile) = input.profile {
        arguments.extend([
            "--profile".to_owned(),
            "summary".to_owned(),
            "--profile-output".to_owned(),
            crate::path::slash_separated(profile),
        ]);
    }

    if let Some(standard_library) = input.standard_library {
        arguments.extend([
            "--standard-library-root".to_owned(),
            crate::path::slash_separated(standard_library),
        ]);
    }

    arguments.extend([
        "build".to_owned(),
        "--package".to_owned(),
        input.package.to_owned(),
        "--product".to_owned(),
        input.product.to_owned(),
        "--product-kind".to_owned(),
        input.product_kind.to_owned(),
        "--target".to_owned(),
        input.target.as_str().to_owned(),
        "--release".to_owned(),
        "--artifact".to_owned(),
        input.artifact.to_owned(),
    ]);

    if let Some(runtime) = input.runtime {
        arguments.extend([
            "--runtime-artifact".to_owned(),
            crate::path::slash_separated(runtime),
        ]);
    }

    if let Some(linker_map) = input.linker_map {
        arguments.extend([
            "--linker-map-output".to_owned(),
            crate::path::slash_separated(linker_map),
        ]);
    }

    arguments.extend([
        "--output".to_owned(),
        crate::path::slash_separated(input.output),
        crate::path::slash_separated(input.source),
    ]);

    arguments
}

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
