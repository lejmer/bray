use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::Path;
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
    ArtifactKind, Observation, PerformanceReport, SCHEMA_REVISION, WorkloadObservations,
    WorkloadReport,
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
            format!("{detail}; {USAGE}"),
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

    let identity = report_identity(&root, &options, &selected)?;
    let mut workloads = Vec::with_capacity(selected.len());

    for workload in selected {
        workloads.push(run_workload(&options, workload, &toolchain, &runtime)?);
    }

    let candidate = PerformanceReport {
        schema_revision: SCHEMA_REVISION,
        identity,
        workloads,
    };

    let candidate_path = options.output.join("candidate.json");

    crate::json::write_pretty(&candidate_path, &candidate)?;
    report::write_summary(&options.output.join("candidate.txt"), &candidate)?;

    if let Some(path) = options.baseline {
        let bytes = read_baseline(&path)?;

        let baseline: PerformanceReport = serde_json::from_slice(&bytes)
            .map_err(|error| format!("baseline {} is invalid: {error}", path.display()))?;

        let comparison = compare(&baseline, &candidate)?;

        crate::json::write_pretty(&options.output.join("comparison.json"), &comparison)?;
        report::write_comparison_summary(&options.output.join("comparison.txt"), &comparison)?;
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

    let output_digest = expected_output_digest(workload.expected_output)?;

    let execution = execute_samples(
        &executable,
        &output,
        options.warmup,
        options.samples,
        workload.scale,
        &output_digest,
    )?;

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
        execution,
        artifacts,
        observations: unavailable_observations(workload),
    })
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

    statistics::summarize(times, scale)
        .ok_or_else(|| "at least one execution sample is required".to_owned())
}

fn unavailable_observations(workload: &Workload) -> WorkloadObservations {
    let reason = "the selected production runtime does not expose benchmark observation hooks";
    let unavailable = || Observation::Unavailable { reason: reason.to_owned() };

    let operation_names = match workload.category {
        super::super::model::WorkloadCategory::Streaming => &["stream_writes"][..],
        super::super::model::WorkloadCategory::Concurrent => {
            &["stream_writes", "task_suspensions"][..]
        }
        super::super::model::WorkloadCategory::Filesystem => &["filesystem_metadata_queries"][..],
        super::super::model::WorkloadCategory::Process => &["process_identity_queries"][..],
        super::super::model::WorkloadCategory::Time => &["monotonic_clock_queries"][..],
        _ => &[],
    };

    let platform_operations = operation_names
        .iter()
        .map(|name| ((*name).to_owned(), unavailable()))
        .collect::<BTreeMap<_, _>>();

    WorkloadObservations {
        allocation_count: unavailable(),
        allocated_bytes: unavailable(),
        copied_bytes: unavailable(),
        platform_operations,
    }
}
