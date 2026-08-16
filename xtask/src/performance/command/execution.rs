use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};
use std::time::Instant;

use bray_compilation::{
    BuildConfiguration, CompilationOptions, CompilationProfileConfiguration,
    CompilationProfileMode, CompilationRequest, SelectedTarget, WorkerBudget,
};
use bray_emitter::{ArtifactKind as EmittedArtifactKind, resolve_published_artifact};
use bray_source::{SourceIdentity, SourceInput, SourceVersion};
use bray_standard_library::StandardLibraryRoot;
use bray_symbols::{PackageIdentity, ProductIdentity, ProductKind};
use bray_tooling::load_llvm_compilation;

use super::super::comparison::compare;
use super::super::corpus::{
    BatchingPolicy, CALIBRATION_SEED_INNER_ITERATIONS, WORKLOADS, Workload,
};
use super::super::model::{
    ArtifactKind, Observation, PeerLanguage, PeerReport, PerformanceReport, SCHEMA_REVISION,
    WorkloadBatching, WorkloadReport,
};
use super::super::{report, retention, statistics};
use super::identity::{expected_output_digest, report_identity};
use super::measurement::{
    ImplementationExecution, ImplementationKey, ImplementationTarget, calibrate_inner_iterations,
    execute_interleaved,
};
use super::comparison_build;
use super::options::Options;
use super::progress;

const USAGE: &str = "usage: cargo xtask performance \
    --output <directory> [--baseline <report.json>] [--target <triple>] \
    [--warmup <count>] [--samples <count>] [--workload <identity>]...";
const MAX_BASELINE_REPORT_BYTES: usize = 16 * 1024 * 1024;

pub(crate) fn run(
    arguments: impl Iterator<Item = String>,
) -> std::process::ExitCode {
    let result = Options::parse(arguments)
        .map_err(|detail| format!("{detail}. {USAGE}"))
        .and_then(execute);

    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");

            std::process::ExitCode::FAILURE
        }
    }
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

    let prepared = super::toolchain::prepare(&root, options.target)?;
    crate::native_toolchain::build_compiler(&root)?;

    let compiler = crate::native_toolchain::compiler_executable(&root, "brayc");

    fs::create_dir_all(&options.output)
        .map_err(|error| format!("could not create {}: {error}", options.output.display()))?;

    let selected = WORKLOADS
        .iter()
        .filter(|workload| options.workloads.is_empty() || options.workloads.contains(workload.id))
        .collect::<Vec<_>>();

    let timer_resolution_nanoseconds = statistics::timer_resolution_nanoseconds()?;
    let identity = report_identity(&root, &options, &selected, timer_resolution_nanoseconds)?;
    progress::phase("Building matched application peers");

    let application_compilation = comparison_build::build(
        crate::performance::model::CompilationKind::Application,
        &root,
        &compiler,
        &options.output,
        options.target,
        prepared.toolchain(),
        prepared.runtime(),
    )?;

    progress::phase("Building matched source library peers");

    let library_compilation = comparison_build::build(
        crate::performance::model::CompilationKind::Library,
        &root,
        &compiler,
        &options.output,
        options.target,
        prepared.toolchain(),
        prepared.runtime(),
    )?;

    let mut workloads = Vec::with_capacity(selected.len());

    progress::plan(selected.len(), options.warmup, options.samples);

    for (index, workload) in selected.iter().enumerate() {
        progress::workload(index.saturating_add(1), selected.len(), workload.id);

        workloads.push(run_workload(
            &options,
            workload,
            prepared.toolchain(),
            prepared.runtime(),
            prepared.observation_runtime(),
            timer_resolution_nanoseconds,
        )?);
    }

    let candidate = PerformanceReport {
        schema_revision: SCHEMA_REVISION,
        identity,
        application_compilation,
        library_compilation,
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
    timer_resolution_nanoseconds: u64,
) -> Result<WorkloadReport, String> {
    let output = options.output.join("workloads").join(workload.id);

    fs::create_dir_all(&output)
        .map_err(|error| format!("could not create {}: {error}", output.display()))?;

    let package_name = format!("bray.performance.{}", workload.id);

    let product_name = "application";

    let package = PackageIdentity::try_new(package_name.as_str())
        .ok_or_else(|| format!("invalid workload package identity: {package_name}"))?;

    let product = ProductIdentity::try_new(package.clone(), product_name)
        .ok_or_else(|| format!("invalid workload product identity: {}", workload.id))?;

    let selected = SelectedTarget::for_native(options.target);

    let source = SourceInput::virtual_text(
        SourceIdentity::new(0),
        format!("{}.bray", workload.id),
        SourceVersion::new(0),
        workload.source,
    );

    let request = CompilationRequest::with_options(
        package.clone(),
        vec![source],
        CompilationOptions::new(WorkerBudget::default(), ProductKind::Executable, selected),
    )
    .with_profile(CompilationProfileConfiguration::new(
        CompilationProfileMode::Summary,
    ))
    .with_profile_product(product.clone());

    let standard_library_path = toolchain.join("lib").join("bray").join("standard-library");

    let standard_library = StandardLibraryRoot::try_new(standard_library_path.clone())
        .ok_or_else(|| "invalid benchmark standard-library root".to_owned())?;

    let request = request.with_standard_library_root(standard_library);

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

    crate::native_product::emit_executable_with_configuration(
        &compilation,
        product.clone(),
        options.target,
        runtime,
        &output,
        [],
        Some(map_output),
        BuildConfiguration::Release,
    )?;

    audit_retention_contract(workload, &map)?;

    let compiler_profile = compilation
        .profile_report()
        .ok_or_else(|| format!("workload {} produced no compiler profile", workload.id))?;

    let executable =
        resolve_published_artifact(&output, &product, EmittedArtifactKind::Executable, 0).map_err(
            |error| {
                format!(
                    "could not resolve workload {} executable: {error:?}",
                    workload.id
                )
            },
        )?;

    let object =
        resolve_published_artifact(&output, &product, EmittedArtifactKind::RelocatableObject, 0)
            .ok();

    super::super::observation::require_production_symbols_absent(&map)?;

    let output_digest = expected_output_digest(workload.expected_output)?;

    let controlled = prepare_controlled_artifacts(
        &compilation,
        product.clone(),
        options.target,
        observation_runtime,
        &output,
        workload,
        &executable,
        &output_digest,
    )?;

    let mut implementations = vec![ImplementationTarget {
        key: ImplementationKey::Bray,
        executable: &executable,
        timed_executable: &controlled.timed_executable,
        timing_map: Some(&controlled.timing_map),
    }];

    implementations.extend(controlled.peers.iter().map(|peer| ImplementationTarget {
        key: ImplementationKey::Peer(peer.language),
        executable: &peer.executable,
        timed_executable: &peer.timed_executable,
        timing_map: None,
    }));

    progress::workload_phase("Running warmups and measured samples");

    let mut execution = execute_interleaved(
        &implementations,
        &output,
        options.warmup,
        options.samples,
        workload,
        &output_digest,
        timer_resolution_nanoseconds,
        controlled.inner_iterations,
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

    let peers = peer_reports(controlled.peers, execution)?;

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
        peer_contract: super::super::peer::comparison_contract(workload.id)
            .ok_or_else(|| format!("workload {} has no peer contract", workload.id))?
            .to_owned(),
        category: workload.category,
        scale: workload.scale,
        units: workload.units.to_owned(),
        expected_output_sha256: output_digest,
        batching: controlled.batching,
        compiler_profile,
        process_execution: bray.process,
        bray_execution: bray.controlled,
        artifacts,
        observations,
        peers,
    };

    progress::workload_phase("Complete");

    Ok(report)
}

struct ControlledArtifacts {
    inner_iterations: NonZeroU64,
    batching: WorkloadBatching,
    timed_executable: PathBuf,
    timing_map: PathBuf,
    peers: Vec<super::super::peer::BuiltPeer>,
}

#[expect(
    clippy::too_many_arguments,
    reason = "controlled artifacts require the compilation and shared measurement contract"
)]
fn prepare_controlled_artifacts(
    compilation: &bray_compilation::Compilation,
    product: ProductIdentity,
    target: bray_target::NativeTarget,
    observation_runtime: &Path,
    output: &Path,
    workload: &Workload,
    production_executable: &Path,
    output_digest: &str,
) -> Result<ControlledArtifacts, String> {
    let seed_inner_iterations = match workload.batching {
        BatchingPolicy::SingleExecution => NonZeroU64::MIN,
        BatchingPolicy::Calibrated => NonZeroU64::new(CALIBRATION_SEED_INNER_ITERATIONS)
            .ok_or_else(|| "calibration seed must be nonzero".to_owned())?,
    };

    let initial_output = match workload.batching {
        BatchingPolicy::SingleExecution => output.join("timing"),
        BatchingPolicy::Calibrated => output.join("calibration"),
    };

    progress::workload_phase("Building timing artifact");

    let (initial_executable, initial_map) = emit_observed_executable(
        compilation,
        product.clone(),
        target,
        observation_runtime,
        &initial_output,
        bray_compilation::BuildConfiguration::TimedRelease {
            inner_iterations: seed_inner_iterations,
        },
        workload.id,
    )?;

    progress::workload_phase("Preparing Rust and C++ peers");

    let root = crate::workspace::root()?;

    let mut peers = super::super::peer::build(
        &root,
        &output.join("peers"),
        target,
        workload.id,
        seed_inner_iterations,
    )?;

    if workload.batching == BatchingPolicy::SingleExecution {
        return Ok(ControlledArtifacts {
            inner_iterations: NonZeroU64::MIN,
            batching: WorkloadBatching::SingleExecution,
            timed_executable: initial_executable,
            timing_map: initial_map,
            peers,
        });
    }

    progress::workload_phase("Calibrating controlled workload");

    let mut calibration_targets = vec![ImplementationTarget {
        key: ImplementationKey::Bray,
        executable: production_executable,
        timed_executable: &initial_executable,
        timing_map: Some(&initial_map),
    }];

    calibration_targets.extend(peers.iter().map(|peer| ImplementationTarget {
        key: ImplementationKey::Peer(peer.language),
        executable: &peer.executable,
        timed_executable: &peer.timed_executable,
        timing_map: None,
    }));

    let (inner_iterations, batching) = calibrate_inner_iterations(
        &calibration_targets,
        output,
        workload,
        output_digest,
        seed_inner_iterations,
    )?;

    progress::workload_phase("Building calibrated timing artifacts");

    let (timed_executable, timing_map) = emit_observed_executable(
        compilation,
        product,
        target,
        observation_runtime,
        &output.join("timing"),
        bray_compilation::BuildConfiguration::TimedRelease { inner_iterations },
        workload.id,
    )?;

    super::super::peer::rebuild_timed(&root, target, workload.id, inner_iterations, &mut peers)?;

    Ok(ControlledArtifacts {
        inner_iterations,
        batching,
        timed_executable,
        timing_map,
        peers,
    })
}

fn peer_reports(
    built: Vec<super::super::peer::BuiltPeer>,
    mut execution: BTreeMap<ImplementationKey, ImplementationExecution>,
) -> Result<BTreeMap<PeerLanguage, PeerReport>, String> {
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
            PeerReport {
                toolchain: built.toolchain,
                build_configuration: built.build_configuration,
                source_sha256: built.source_sha256,
                process_execution: measured.process,
                controlled_execution: measured.controlled,
                artifacts,
                observations: unavailable_peer_observations(),
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

    let map_output = bray_linker::SystemLinkerMapOutput::try_new(map.clone()).ok_or_else(|| {
        format!(
            "invalid observed workload linker-map path: {}",
            map.display()
        )
    })?;

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

    let executable =
        resolve_published_artifact(output, &product, EmittedArtifactKind::Executable, 0).map_err(
            |error| format!("could not resolve observed workload {workload} executable: {error:?}"),
        )?;

    Ok((executable, map))
}

fn unavailable_platform_observations(workload: &Workload) -> BTreeMap<String, Observation> {
    let reason = "the selected production runtime does not expose benchmark observation hooks";

    let unavailable = || Observation::Unavailable {
        reason: reason.to_owned(),
    };

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
        reason: "the selected language runtime does not expose matching observation hooks"
            .to_owned(),
    };

    super::super::model::WorkloadObservations {
        allocation_count: unavailable(),
        allocated_bytes: unavailable(),
        copied_bytes: unavailable(),
        platform_operations: BTreeMap::new(),
    }
}

fn audit_retention_contract(workload: &Workload, map: &Path) -> Result<(), String> {
    let contents = fs::read_to_string(map).map_err(|error| {
        format!(
            "could not read workload linker map {}: {error}",
            map.display()
        )
    })?;

    for symbol in workload.retention.required_symbols {
        if !crate::link_map::contains_symbol(&contents, symbol) {
            return Err(retention_error(
                workload,
                "did not retain required symbol",
                symbol,
            ));
        }
    }

    for symbol in workload.retention.forbidden_symbols {
        if crate::link_map::contains_symbol(&contents, symbol) {
            return Err(retention_error(
                workload,
                "retained forbidden symbol",
                symbol,
            ));
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
    use super::execute;

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
