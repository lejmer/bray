use std::path::Path;

use super::super::corpus::Workload;

#[expect(
    clippy::too_many_arguments,
    reason = "the measurement requires the exact compiler and application inputs"
)]
pub(super) fn measure(
    compiler: &Path,
    workload: &Workload,
    package: &str,
    product: &str,
    target: bray_target::NativeTarget,
    standard_library: &Path,
    runtime: &Path,
    output: &Path,
) -> Result<super::super::model::WorkloadCompilationReport, String> {
    let directory = output.join("compilation");
    let process_output = directory.join("process");
    let profiled_output = directory.join("profiled");
    let profile = directory.join("profile.json");
    let source = directory.join(format!("{}.bray", workload.id));

    std::fs::create_dir_all(&directory)
        .map_err(|error| format!("could not create {}: {error}", directory.display()))?;

    std::fs::write(&source, workload.source)
        .map_err(|error| format!("could not write {}: {error}", source.display()))?;

    let process_arguments = super::compiler::application_arguments(
        package,
        product,
        &source,
        &process_output,
        target,
        standard_library,
        runtime,
        None,
        None,
    );

    let process = super::super::compiler_timing::bray_process(
        compiler,
        &process_arguments,
        "measuring Bray compiler process",
    )?;

    let profiled_arguments = super::compiler::application_arguments(
        package,
        product,
        &source,
        &profiled_output,
        target,
        standard_library,
        runtime,
        Some(&profile),
        None,
    );

    let compiler_work = super::super::compiler_timing::bray(
        compiler,
        &profiled_arguments,
        &profile,
        "measuring Bray compiler work",
    )?;

    Ok(super::super::compiler_timing::report(
        process,
        compiler_work,
    ))
}
