use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use bray_base::lowercase_hex;
use bray_compilation::{
    CompilationOptions, CompilationProfileConfiguration, CompilationProfileMode,
    CompilationRequest, SelectedTarget, WorkerBudget,
};
use bray_emitter::{ArtifactKind as EmittedArtifactKind, resolve_published_artifact};
use bray_source::{SourceIdentity, SourceInput, SourceVersion};
use bray_standard_library::StandardLibraryRoot;
use bray_symbols::{PackageIdentity, ProductIdentity, ProductKind};
use bray_tooling::{load_llvm_compilation, source_inputs_from_file_arguments};
use sha2::{Digest as _, Sha256};

use super::super::comparison::compare;
use super::super::corpus::{WORKLOADS, Workload};
use super::super::model::{
    ArtifactKind, Observation, PerformanceReport, SCHEMA_REVISION, WorkloadReport,
};
use super::super::{report, retention, statistics};
use super::identity::{expected_output_digest, report_identity};
use super::options::Options;

const USAGE: &str = "usage: cargo xtask standard-library performance \
    --output <directory> [--baseline <report.json>] [--target <triple>] \
    [--warmup <count>] [--samples <count>] [--workload <identity>]...";
const MAX_BASELINE_REPORT_BYTES: usize = 16 * 1024 * 1024;

pub(in crate::standard_library) fn run(
    arguments: impl Iterator<Item = String>,
) -> Result<(), super::super::super::command::BuildError> {
    let options = Options::parse(arguments).map_err(|detail| {
        super::super::super::command::BuildError::conformance(
            "performance",
            format!("{detail}. {USAGE}"),
        )
    })?;

    execute(options).map_err(|detail| {
        super::super::super::command::BuildError::conformance("performance", detail)
    })
}

fn execute(mut options: Options) -> Result<(), String> {
    let root = crate::workspace::root()?;

    if options.output.is_relative() {
        options.output = root.join(&options.output);
    }

    if options.output.exists() {
        return Err(format!(
            "performance output already exists: {}",
            options.output.display()
        ));
    }

    fs::create_dir_all(&options.output)
        .map_err(|error| format!("could not create {}: {error}", options.output.display()))?;

    let runtime_directory = options.output.join("runtime");
    let runtime = crate::runtime_artifact::build_for_readiness(options.target, &runtime_directory)?;
    let toolchain = options.output.join("toolchain");

    crate::native_toolchain::assemble(&root, options.target, &runtime, &toolchain)?;

    let selected = WORKLOADS
        .iter()
        .filter(|workload| {
            options.workloads.is_empty() || options.workloads.contains(workload.id)
        })
        .collect::<Vec<_>>();

    let observation_runtime = crate::runtime_artifact::build_for_performance_observation(
        options.target,
        &options.output.join("observation-runtime"),
    )?;

    let identity = report_identity(&root, &options, &selected)?;
    let mut workloads = Vec::with_capacity(selected.len());

    for workload in selected {
        workloads.push(run_workload(
            &options,
            workload,
            &toolchain,
            &runtime,
            &observation_runtime,
        )?);
    }

    let candidate = PerformanceReport {
        schema_revision: SCHEMA_REVISION,
        identity,
        workloads,
    };

    let candidate_path = options.output.join("candidate.json");

    crate::json::write_pretty(&candidate_path, &candidate)?;
    report::write_candidate(&options.output.join("candidate.html"), &candidate)?;

    if let Some(path) = options.baseline {
        let bytes = read_baseline(&path)?;

        let baseline: PerformanceReport = serde_json::from_slice(&bytes)
            .map_err(|error| format!("baseline {} is invalid: {error}", path.display()))?;

        let comparison = compare(&baseline, &candidate)?;

        crate::json::write_pretty(&options.output.join("comparison.json"), &comparison)?;
        report::write_comparison(&options.output.join("comparison.html"), &comparison)?;
    }

    println!("{}", candidate_path.display());

    Ok(())
}

fn read_baseline(path: &Path) -> Result<Vec<u8>, String> {
    let file = fs::File::open(path)
        .map_err(|error| format!("could not read baseline {}: {error}", path.display()))?;

    let read_limit = u64::try_from(MAX_BASELINE_REPORT_BYTES.saturating_add(1))
        .map_err(|_| "baseline report limit cannot be represented by this host".to_owned())?;

    let mut reader = file.take(read_limit);
    let mut bytes = Vec::new();

    reader
        .read_to_end(&mut bytes)
        .map_err(|error| format!("could not read baseline {}: {error}", path.display()))?;

    if bytes.len() > MAX_BASELINE_REPORT_BYTES {
        return Err(format!(
            "baseline {} exceeds the {} byte report limit",
            path.display(),
            MAX_BASELINE_REPORT_BYTES
        ));
    }

    Ok(bytes)
}

fn run_workload(
    options: &Options,
    workload: &Workload,
    toolchain: &Path,
    runtime: &Path,
    observation_runtime: &Path,
) -> Result<WorkloadReport, String> {
    let output = options.output.join("workloads").join(workload.id);

    fs::create_dir_all(&output)
        .map_err(|error| format!("could not create {}: {error}", output.display()))?;

    let ordinary_package = format!("bray.performance.{}", workload.id);

    let package_name = if workload.standard_library_sources.is_empty() {
        ordinary_package.as_str()
    } else {
        "std"
    };

    let product_name = "application";

    let package = PackageIdentity::try_new(package_name)
        .ok_or_else(|| format!("invalid workload package identity: {package_name}"))?;

    let product = ProductIdentity::try_new(package.clone(), product_name)
        .ok_or_else(|| format!("invalid workload product identity: {}", workload.id))?;

    let selected = SelectedTarget::for_native(options.target);

    let mut sources = source_inputs_from_file_arguments(
        workload
            .standard_library_sources
            .iter()
            .map(|source| crate::workspace::root().map(|root| root.join(source)))
            .collect::<Result<Vec<_>, _>>()?,
    )
    .map_err(|error| format!("could not load workload support sources: {error:?}"))?;

    let source_identity = u32::try_from(sources.len())
        .map_err(|_| format!("workload {} has too many source inputs", workload.id))?;

    let source = SourceInput::virtual_text(
        SourceIdentity::new(source_identity),
        format!("{}.bray", workload.id),
        SourceVersion::new(0),
        workload.source,
    );

    sources.push(source);

    let request = CompilationRequest::with_options(
        package,
        sources,
        CompilationOptions::new(WorkerBudget::default(), ProductKind::Executable, selected),
    )
    .with_profile(CompilationProfileConfiguration::new(
        CompilationProfileMode::Summary,
    ))
    .with_profile_product(product.clone());

    let request = if workload.standard_library_sources.is_empty() {
        let standard_library = StandardLibraryRoot::try_new(
            toolchain.join("lib").join("bray").join("standard-library"),
        )
        .ok_or_else(|| "invalid benchmark standard-library root".to_owned())?;

        request.with_standard_library_root(standard_library)
    } else {
        request.with_standard_library_source_authority()
    };

    let compilation = load_llvm_compilation(request)
        .map_err(|error| format!("could not load workload {}: {error:?}", workload.id))?;

    let diagnostics = compilation.check_diagnostics();

    if !diagnostics.is_empty() {
        return Err(format!(
            "workload {} did not pass compilation checks: {diagnostics:?}",
            workload.id,
        ));
    }

    let map = output.join("application.map");

    let map_output = bray_linker::SystemLinkerMapOutput::try_new(map.clone())
        .ok_or_else(|| format!("invalid workload linker-map path: {}", map.display()))?;

    crate::native_product::emit_executable(
        &compilation,
        product.clone(),
        options.target,
        runtime,
        &output,
        [],
        Some(map_output),
    )?;

    audit_retention_contract(workload, &map)?;

    let profile = compilation
        .profile_report()
        .ok_or_else(|| format!("workload {} produced no compiler profile", workload.id))?;

    let executable = resolve_published_artifact(
        &output,
        &product,
        EmittedArtifactKind::Executable,
        0,
    )
    .map_err(|error| format!("could not resolve workload {} executable: {error:?}", workload.id))?;

    let object = resolve_published_artifact(
        &output,
        &product,
        EmittedArtifactKind::RelocatableObject,
        0,
    )
    .ok();

    super::super::observation::require_production_symbols_absent(&map)?;

    let output_digest = expected_output_digest(workload.expected_output)?;

    let process_execution = execute_samples(
        &executable,
        &output,
        options.warmup,
        options.samples,
        workload.scale,
        &output_digest,
    )?;

    let timing_output = output.join("timing");

    let (timed_executable, timing_map) = emit_observed_executable(
        &compilation,
        product.clone(),
        options.target,
        observation_runtime,
        &timing_output,
        bray_compilation::BuildConfiguration::TimedRelease,
        workload.id,
    )?;

    let bray_execution = super::super::observation::measure_timing_samples(
        &timed_executable,
        &timing_map,
        &timing_output,
        &timing_output,
        options.warmup,
        options.samples,
        workload.scale,
        &output_digest,
    )?;

    let mut observations = if let Some(expected) = workload.storage {
        let storage_output = output.join("storage-observation");

        let (storage_executable, storage_map) = emit_observed_executable(
            &compilation,
            product.clone(),
            options.target,
            observation_runtime,
            &storage_output,
            bray_compilation::BuildConfiguration::ObservedRelease,
            workload.id,
        )?;

        super::super::observation::measure_storage(
            &storage_executable,
            &storage_map,
            &storage_output,
            &storage_output,
            &output_digest,
            expected,
        )?
    } else {
        unavailable_storage_observations()
    };

    observations.platform_operations = unavailable_platform_observations(workload);

    let mut artifacts = vec![retention::inspect(
        ArtifactKind::Executable,
        &executable,
        Some(&map),
    )?];

    if let Some(object) = object {
        artifacts.push(retention::inspect(
            ArtifactKind::RelocatableObject,
            &object,
            None,
        )?);
    }

    Ok(WorkloadReport {
        id: workload.id.to_owned(),
        category: workload.category,
        scale: workload.scale,
        units: workload.units.to_owned(),
        expected_output_sha256: output_digest,
        compilation: profile,
        process_execution,
        bray_execution,
        artifacts,
        observations,
    })
}

fn emit_observed_executable(
    compilation: &bray_compilation::Compilation,
    product: ProductIdentity,
    target: bray_target::NativeTarget,
    runtime: &Path,
    output: &Path,
    configuration: bray_compilation::BuildConfiguration,
    workload: &str,
) -> Result<(PathBuf, PathBuf), String> {
    fs::create_dir_all(output)
        .map_err(|error| format!("could not create {}: {error}", output.display()))?;

    let map = output.join("application.map");

    let map_output = bray_linker::SystemLinkerMapOutput::try_new(map.clone())
        .ok_or_else(|| format!("invalid observed workload linker-map path: {}", map.display()))?;

    crate::native_product::emit_executable_with_configuration(
        compilation,
        product.clone(),
        target,
        runtime,
        output,
        [],
        Some(map_output),
        configuration,
    )?;

    let executable = resolve_published_artifact(
        output,
        &product,
        EmittedArtifactKind::Executable,
        0,
    )
    .map_err(|error| {
        format!("could not resolve observed workload {workload} executable: {error:?}")
    })?;

    Ok((executable, map))
}

fn execute_samples(
    executable: &Path,
    working_directory: &Path,
    warmup: u32,
    samples: u32,
    scale: u64,
    expected_output_sha256: &str,
) -> Result<super::super::model::ExecutionStatistics, String> {
    let sample_capacity = usize::try_from(samples)
        .map_err(|_| "sample count cannot be represented by this host".to_owned())?;

    let mut times = Vec::with_capacity(sample_capacity);

    for iteration in 0..warmup.saturating_add(samples) {
        let started = Instant::now();

        let output = Command::new(executable)
            .current_dir(working_directory)
            .output()
            .map_err(|error| format!("could not execute {}: {error}", executable.display()))?;

        let elapsed = u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX);

        if !output.status.success() {
            return Err(format!(
                "{} exited unsuccessfully: {}",
                executable.display(),
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }

        let digest = lowercase_hex(&Sha256::digest(&output.stdout));

        if digest != expected_output_sha256 {
            return Err(format!(
                "{} did not produce the corpus-defined output",
                executable.display()
            ));
        }

        if iteration >= warmup {
            times.push(elapsed);
        }
    }

    statistics::summarize(times, scale, super::super::model::PROCESS_EXECUTION_SCOPE)
        .ok_or_else(|| "at least one execution sample is required".to_owned())
}

fn unavailable_platform_observations(workload: &Workload) -> BTreeMap<String, Observation> {
    let reason = "the selected production runtime does not expose benchmark observation hooks";
    let unavailable = || Observation::Unavailable { reason: reason.to_owned() };

    workload
        .platform_operations
        .iter()
        .map(|name| ((*name).to_owned(), unavailable()))
        .collect()
}

fn unavailable_storage_observations() -> super::super::model::WorkloadObservations {
    let unavailable = || Observation::Unavailable {
        reason: "the workload has no storage observation contract".to_owned(),
    };

    super::super::model::WorkloadObservations {
        allocation_count: unavailable(),
        allocated_bytes: unavailable(),
        copied_bytes: unavailable(),
        platform_operations: BTreeMap::new(),
    }
}

fn audit_retention_contract(workload: &Workload, map: &Path) -> Result<(), String> {
    let contents = fs::read_to_string(map)
        .map_err(|error| format!("could not read workload linker map {}: {error}", map.display()))?;

    for symbol in workload.retention.required_symbols {
        if !crate::link_map::contains_symbol(&contents, symbol) {
            return Err(retention_error(workload, "did not retain required symbol", symbol));
        }
    }

    for symbol in workload.retention.forbidden_symbols {
        if crate::link_map::contains_symbol(&contents, symbol) {
            return Err(retention_error(workload, "retained forbidden symbol", symbol));
        }
    }

    for provenance in workload.retention.required_provenance {
        if !retention::contains_retained_provenance(&contents, provenance) {
            return Err(retention_error(
                workload,
                "did not retain required provenance",
                provenance,
            ));
        }
    }

    for provenance in workload.retention.forbidden_provenance {
        if retention::contains_retained_provenance(&contents, provenance) {
            return Err(retention_error(
                workload,
                "retained forbidden provenance",
                provenance,
            ));
        }
    }

    Ok(())
}

fn retention_error(workload: &Workload, behavior: &str, identity: &str) -> String {
    format!("workload {} {behavior} {identity}", workload.id)
}
