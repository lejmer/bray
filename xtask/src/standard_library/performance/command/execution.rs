use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use bray_compilation::{
    CompilationOptions, CompilationProfileConfiguration, CompilationProfileMode,
    CompilationRequest, SelectedTarget, WorkerBudget,
};
use bray_emitter::{ArtifactKind as EmittedArtifactKind, resolve_published_artifact};
use bray_source::{SourceIdentity, SourceInput, SourceVersion};
use bray_standard_library::StandardLibraryRoot;
use bray_symbols::{PackageIdentity, ProductIdentity, ProductKind};
use bray_tooling::{load_llvm_compilation, source_inputs_from_file_arguments};

use super::super::comparison::compare;
use super::super::corpus::{ExpectedSideEffects, WORKLOADS, Workload};
use super::super::model::{
    ArtifactKind, Observation, PeerLanguage, PeerOutcome, PeerReport, PerformanceReport,
    SCHEMA_REVISION, WorkloadReport,
};
use super::super::{report, retention, statistics};
use super::identity::{expected_output_digest, report_identity};
use super::options::Options;
use super::progress;

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
    let started = Instant::now();
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

    progress::phase("Preparing performance runtime");

    let runtime_directory = options.output.join("runtime");
    let runtime = crate::runtime_artifact::build_for_readiness(options.target, &runtime_directory)?;
    let toolchain = options.output.join("toolchain");

    progress::phase("Assembling performance toolchain");

    crate::native_toolchain::assemble(&root, options.target, &runtime, &toolchain)?;

    let selected = WORKLOADS
        .iter()
        .filter(|workload| {
            options.workloads.is_empty() || options.workloads.contains(workload.id)
        })
        .collect::<Vec<_>>();

    progress::phase("Preparing performance observations");

    let observation_runtime = crate::runtime_artifact::build_for_performance_observation(
        options.target,
        &options.output.join("observation-runtime"),
    )?;

    let identity = report_identity(&root, &options, &selected)?;
    let mut workloads = Vec::with_capacity(selected.len());

    progress::plan(selected.len(), options.warmup, options.samples);

    for (index, workload) in selected.iter().enumerate() {
        progress::workload(index.saturating_add(1), selected.len(), workload.id);

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

    super::super::validation::validate(&candidate)?;

    let candidate_path = options.output.join("candidate.json");
    let candidate_html = options.output.join("candidate.html");

    crate::json::write_pretty(&candidate_path, &candidate)?;
    report::write_candidate(&candidate_html, &candidate)?;

    progress::report("Candidate HTML", &candidate_html);

    if let Some(path) = options.baseline {
        progress::phase("Comparing performance reports");

        let bytes = read_baseline(&path)?;

        let baseline: PerformanceReport = serde_json::from_slice(&bytes)
            .map_err(|error| format!("baseline {} is invalid: {error}", path.display()))?;

        let comparison = compare(&baseline, &candidate)?;
        let comparison_html = options.output.join("comparison.html");

        crate::json::write_pretty(&options.output.join("comparison.json"), &comparison)?;
        report::write_comparison(&comparison_html, &comparison)?;

        progress::report("Comparison HTML", &comparison_html);
    }

    progress::finished(started.elapsed());

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

    progress::workload_phase("Compiling Bray artifacts");

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
    let timing_output = output.join("timing");

    progress::workload_phase("Building timing artifact");

    let (timed_executable, timing_map) = emit_observed_executable(
        &compilation,
        product.clone(),
        options.target,
        observation_runtime,
        &timing_output,
        bray_compilation::BuildConfiguration::TimedRelease,
        workload.id,
    )?;

    progress::workload_phase("Preparing Rust and C++ peers");

    let peer_build = super::super::peer::build(
        &crate::workspace::root()?,
        &output.join("peers"),
        options.target,
        workload.id,
    )?;

    let mut implementations = vec![ImplementationTarget {
        key: ImplementationKey::Bray,
        executable: &executable,
        timed_executable: &timed_executable,
        timing_map: Some(&timing_map),
    }];

    if let Ok(peers) = &peer_build {
        implementations.extend(peers.iter().map(|peer| ImplementationTarget {
            key: ImplementationKey::Peer(peer.language),
            executable: &peer.executable,
            timed_executable: &peer.timed_executable,
            timing_map: None,
        }));
    }

    progress::workload_phase("Running warmups and measured samples");

    let mut execution = execute_interleaved(
        &implementations,
        &output,
        options.warmup,
        options.samples,
        workload,
        &output_digest,
    )?;

    let bray = execution
        .remove(&ImplementationKey::Bray)
        .ok_or_else(|| "interleaved execution omitted Bray".to_owned())?;

    let mut observations = if let Some(expected) = workload.storage {
        progress::workload_phase("Measuring storage work");

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

    let peers = peer_reports(peer_build, execution)?;

    progress::workload_phase("Inspecting artifacts");

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

    let report = WorkloadReport {
        id: workload.id.to_owned(),
        peer_contract: super::super::peer::comparison_contract(workload.id).map(str::to_owned),
        category: workload.category,
        scale: workload.scale,
        units: workload.units.to_owned(),
        expected_output_sha256: output_digest,
        compilation: profile,
        process_execution: bray.process,
        bray_execution: bray.controlled,
        artifacts,
        observations,
        peers,
    };

    progress::workload_phase("Complete");

    Ok(report)
}

fn peer_reports(
    built: Result<Vec<super::super::peer::BuiltPeer>, String>,
    mut execution: BTreeMap<ImplementationKey, ImplementationExecution>,
) -> Result<BTreeMap<PeerLanguage, PeerOutcome>, String> {
    let built = match built {
        Ok(built) => built,
        Err(reason) => {
            return Ok([PeerLanguage::Rust, PeerLanguage::Cpp]
                .into_iter()
                .map(|language| {
                    (
                        language,
                        PeerOutcome::Unsupported {
                            reason: reason.clone(),
                        },
                    )
                })
                .collect());
        }
    };

    let mut peers = BTreeMap::new();

    for built in built {
        let measured = execution
            .remove(&ImplementationKey::Peer(built.language))
            .ok_or_else(|| "interleaved execution omitted a measured peer".to_owned())?;

        let artifacts = vec![retention::inspect(
            ArtifactKind::Executable,
            &built.executable,
            Some(&built.linker_map),
        )?];

        peers.insert(
            built.language,
            PeerOutcome::Measured {
                report: PeerReport {
                    toolchain: built.toolchain,
                    build_configuration: built.build_configuration,
                    source_sha256: built.source_sha256,
                    production_compile_link_nanoseconds: built
                        .production_compile_link_nanoseconds,
                    process_execution: measured.process,
                    controlled_execution: measured.controlled,
                    artifacts,
                    observations: unavailable_peer_observations(),
                },
            },
        );
    }

    Ok(peers)
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

#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
enum ImplementationKey {
    Bray,
    Peer(PeerLanguage),
}

struct ImplementationTarget<'a> {
    key: ImplementationKey,
    executable: &'a Path,
    timed_executable: &'a Path,
    timing_map: Option<&'a Path>,
}

struct ImplementationSamples {
    process: Vec<u64>,
    controlled: Vec<u64>,
}

struct ImplementationExecution {
    process: super::super::model::ExecutionStatistics,
    controlled: super::super::model::ExecutionStatistics,
}

#[expect(
    clippy::too_many_arguments,
    reason = "the interleaved run keeps its sample policy and validation contract explicit"
)]
fn execute_interleaved(
    implementations: &[ImplementationTarget<'_>],
    working_directory: &Path,
    warmup: u32,
    samples: u32,
    workload: &Workload,
    expected_output_sha256: &str,
) -> Result<BTreeMap<ImplementationKey, ImplementationExecution>, String> {
    let sample_capacity = usize::try_from(samples)
        .map_err(|_| "sample count cannot be represented by this host".to_owned())?;

    if implementations.is_empty() {
        return Err("performance execution requires at least one implementation".to_owned());
    }

    for implementation in implementations {
        if let Some(map) = implementation.timing_map {
            super::super::observation::validate_timing_artifact(map)?;
        }
    }

    let mut measured = implementations
        .iter()
        .map(|implementation| {
            (
                implementation.key,
                ImplementationSamples {
                    process: Vec::with_capacity(sample_capacity),
                    controlled: Vec::with_capacity(sample_capacity),
                },
            )
        })
        .collect::<BTreeMap<_, _>>();

    for iteration in 0..warmup.saturating_add(samples) {
        let start = rotation_start(iteration, implementations.len(), 0);

        for offset in 0..implementations.len() {
            let implementation = &implementations[(start + offset) % implementations.len()];

            let elapsed = execute_process_sample(
                implementation.executable,
                working_directory,
                expected_output_sha256,
                workload.expected_side_effects,
            )?;

            if iteration >= warmup {
                measured
                    .get_mut(&implementation.key)
                    .ok_or_else(|| "interleaved process sample lost its implementation".to_owned())?
                    .process
                    .push(elapsed);
            }
        }

        let timing_start = rotation_start(iteration, implementations.len(), 1);

        for offset in 0..implementations.len() {
            let implementation =
                &implementations[(timing_start + offset) % implementations.len()];

            let elapsed = super::super::observation::execute_timing_sample(
                implementation.timed_executable,
                working_directory,
                working_directory,
                iteration,
                expected_output_sha256,
                workload.expected_side_effects,
            )?;

            if iteration >= warmup {
                measured
                    .get_mut(&implementation.key)
                    .ok_or_else(|| "interleaved timing sample lost its implementation".to_owned())?
                    .controlled
                    .push(elapsed);
            }
        }
    }

    measured
        .into_iter()
        .map(|(key, samples)| {
            let process = statistics::summarize(
                samples.process,
                workload.scale,
                super::super::model::PROCESS_EXECUTION_SCOPE,
            )
            .ok_or_else(|| "at least one process execution sample is required".to_owned())?;

            let controlled = statistics::summarize(
                samples.controlled,
                workload.scale,
                super::super::model::BRAY_EXECUTION_SCOPE,
            )
            .ok_or_else(|| "at least one controlled execution sample is required".to_owned())?;

            Ok((
                key,
                ImplementationExecution {
                    process,
                    controlled,
                },
            ))
        })
        .collect()
}

fn rotation_start(iteration: u32, implementations: usize, phase_offset: usize) -> usize {
    usize::try_from(iteration)
        .unwrap_or(usize::MAX)
        .wrapping_add(phase_offset)
        .wrapping_rem(implementations)
}

fn execute_process_sample(
    executable: &Path,
    working_directory: &Path,
    expected_output_sha256: &str,
    expected_side_effects: ExpectedSideEffects,
) -> Result<u64, String> {
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

    super::super::validate_output(
        &output,
        expected_output_sha256,
        expected_side_effects,
        working_directory,
    )?;

    Ok(elapsed)
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

fn unavailable_peer_observations() -> super::super::model::WorkloadObservations {
    let unavailable = || Observation::Unavailable {
        reason: "the selected language runtime does not expose matching observation hooks".to_owned(),
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::super::options::Options;
    use super::{execute, rotation_start};

    #[test]
    fn process_and_controlled_rounds_rotate_language_priority() {
        assert_eq!(
            (0..6)
                .map(|iteration| rotation_start(iteration, 3, 0))
                .collect::<Vec<_>>(),
            [0, 1, 2, 0, 1, 2]
        );

        assert_eq!(
            (0..6)
                .map(|iteration| rotation_start(iteration, 3, 1))
                .collect::<Vec<_>>(),
            [1, 2, 0, 1, 2, 0]
        );
    }

    #[test]
    #[ignore = "requires the pinned LLVM toolchain and native process execution"]
    fn small_cross_language_comparison_runs_end_to_end() {
        let directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("comparison directory must exist: {error}"));

        let target = bray_target::NativeTarget::current()
            .unwrap_or_else(|| panic!("comparison test requires a supported native host"));

        let workloads = BTreeSet::from(["small_output".to_owned()]);
        let baseline_output = directory.path().join("baseline");

        execute(Options {
            output: baseline_output.clone(),
            baseline: None,
            target,
            warmup: 1,
            samples: 1,
            workloads,
        })
        .unwrap_or_else(|error| panic!("baseline comparison run must pass: {error}"));

        let report = std::fs::read(baseline_output.join("candidate.json"))
            .unwrap_or_else(|error| panic!("candidate report must be readable: {error}"));

        let report: super::super::super::model::PerformanceReport = serde_json::from_slice(&report)
            .unwrap_or_else(|error| panic!("candidate report must decode: {error}"));

        let comparison = super::super::super::comparison::compare(&report, &report)
            .unwrap_or_else(|error| panic!("candidate must compare with itself: {error}"));

        let comparison_path = baseline_output.join("comparison.json");

        crate::json::write_pretty(&comparison_path, &comparison)
            .unwrap_or_else(|error| panic!("comparison JSON must write: {error}"));

        super::super::super::report::write_comparison(
            &baseline_output.join("comparison.html"),
            &comparison,
        )
        .unwrap_or_else(|error| panic!("comparison HTML must write: {error}"));

        assert_eq!(comparison.workloads.len(), 1);
        assert_eq!(comparison.workloads[0].peers.len(), 2);
        assert!(baseline_output.join("candidate.html").is_file());
        assert!(comparison_path.is_file());
        assert!(baseline_output.join("comparison.html").is_file());
    }
}
