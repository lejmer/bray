use std::collections::BTreeSet;
use std::num::NonZeroU64;
use std::path::Path;

use bray_base::is_lowercase_hex;
use bray_target::{NativeTarget, ObjectFormat};

use super::command::expected_output_digest;
use super::corpus::{
    BatchingPolicy, CALIBRATION_SAMPLE_COUNT, CALIBRATION_SEED_INNER_ITERATIONS,
    CALIBRATION_TARGET_NANOSECONDS, WORKLOADS,
};
use super::model::{
    ArtifactReport, BoundedList, MAX_DYNAMIC_LIBRARY_COUNT, MAX_PLATFORM_OPERATION_COUNT,
    MAX_RETAINED_INPUT_COUNT, MAX_SAMPLE_COUNT, MAX_SECTION_COUNT, Observation, PeerLanguage,
    PerformanceReport, RetainedInput, SCHEMA_REVISION, WorkloadBatching,
};

const MINIMUM_BATCH_INTERVAL_NANOSECONDS: u64 = 10_000_000;
const MINIMUM_TIMER_RESOLUTION_MULTIPLE: u64 = 10_000;

pub(super) fn validate(report: &PerformanceReport) -> Result<(), String> {
    if report.schema_revision != SCHEMA_REVISION {
        return Err(format!(
            "unsupported report schema revision: {}",
            report.schema_revision
        ));
    }

    if report.workloads.is_empty() || report.workloads.len() > WORKLOADS.len() {
        return Err("report workload count is outside the canonical corpus bound".to_owned());
    }

    super::compilation::validate(&report.application_compilation)?;
    super::compilation::validate(&report.library_compilation)?;

    if report.application_compilation.kind != super::model::CompilationKind::Application {
        return Err("application compilation report uses the wrong comparison kind".to_owned());
    }

    if report.library_compilation.kind != super::model::CompilationKind::Library {
        return Err("library compilation report uses the wrong comparison kind".to_owned());
    }

    let target = validate_identity(report)?;

    if report.application_compilation.contract
        != super::compilation::MATCHED_APPLICATION_CONTRACT
    {
        return Err("application compilation report uses the wrong source contract".to_owned());
    }

    if report.library_compilation.contract != super::compilation::MATCHED_LIBRARY_CONTRACT {
        return Err("library compilation report uses the wrong source contract".to_owned());
    }

    validate_compiler_profiles(
        &report.application_compilation,
        target,
        "application compilation",
    )?;

    validate_compiler_profiles(&report.library_compilation, target, "library compilation")?;

    let mut identities = BTreeSet::new();

    for workload in &report.workloads {
        if !identities.insert(workload.id.as_str()) {
            return Err(format!("report repeats workload {}", workload.id));
        }

        validate_workload(report, workload, target)?;
    }

    Ok(())
}

fn validate_identity(report: &PerformanceReport) -> Result<NativeTarget, String> {
    let identity = &report.identity;
    let sha_is_valid = |value: &str| value.len() == 64 && is_lowercase_hex(value);

    let target = bray_target::TargetIdentity::try_new(identity.target.as_str())
        .and_then(|identity| bray_target::NativeTarget::for_identity(&identity))
        .ok_or_else(|| "report target is not a supported native target".to_owned())?;

    let expected_runtime_linkage = super::peer::runtime_linkage(target)?;

    if identity.corpus_revision == 0
        || !sha_is_valid(&identity.corpus_sha256)
        || identity.target.is_empty()
        || identity.host.is_empty()
        || identity.build_configuration.is_empty()
        || identity.compiler_version.is_empty()
        || identity.source_revision.is_empty()
        || identity.llvm_version.is_empty()
        || identity.runtime_linkage != expected_runtime_linkage
        || identity.warmup_iterations == 0
        || identity.sample_iterations == 0
        || identity.sample_iterations > MAX_SAMPLE_COUNT
        || identity.timer_resolution_nanoseconds == 0
    {
        return Err("report identity is incomplete or outside its bounds".to_owned());
    }

    Ok(target)
}

fn validate_workload(
    report: &PerformanceReport,
    workload: &super::model::WorkloadReport,
    target: NativeTarget,
) -> Result<(), String> {
    let canonical = WORKLOADS
        .iter()
        .find(|candidate| candidate.id == workload.id)
        .ok_or_else(|| format!("report contains unknown workload {}", workload.id))?;

    let expected_output = expected_output_digest(canonical.expected_output)?;

    let expected_peer_contract = super::peer::comparison_contract(&workload.id)
        .ok_or_else(|| format!("workload {} has no peer contract", workload.id))?;

    if workload.category != canonical.category
        || workload.scale != canonical.scale
        || workload.units != canonical.units
        || workload.expected_output_sha256 != expected_output
        || workload.peer_contract != expected_peer_contract
    {
        return Err(format!(
            "workload {} does not match the canonical corpus",
            workload.id
        ));
    }

    let profile = &workload.compiler_profile;

    profile.validate().map_err(|error| {
        format!(
            "workload {} has an invalid compiler profile: {error:?}",
            workload.id
        )
    })?;

    if profile.context.target != report.identity.target {
        return Err(format!(
            "workload {} compiler target differs from the report",
            workload.id
        ));
    }

    let expected_samples = usize::try_from(report.identity.sample_iterations)
        .map_err(|_| "sample count cannot be represented by this host".to_owned())?;

    let controlled_inner_iterations = NonZeroU64::new(workload.bray_execution.inner_iterations)
        .ok_or_else(|| format!("workload {} has a zero repetition count", workload.id))?;

    validate_batching(
        canonical.batching,
        &workload.batching,
        controlled_inner_iterations,
    )?;

    if workload.process_execution.scope != super::model::PROCESS_EXECUTION_SCOPE
        || workload.bray_execution.scope != super::model::BRAY_EXECUTION_SCOPE
        || !execution_is_valid(
            &workload.process_execution,
            expected_samples,
            workload.scale,
            1,
            report.identity.timer_resolution_nanoseconds,
        )
        || !execution_is_valid(
            &workload.bray_execution,
            expected_samples,
            workload.scale,
            controlled_inner_iterations.get(),
            report.identity.timer_resolution_nanoseconds,
        )
    {
        return Err(format!(
            "workload {} has invalid execution statistics",
            workload.id
        ));
    }

    if workload.artifacts.is_empty() || workload.artifacts.len() > 2 {
        return Err(format!(
            "workload {} has an invalid artifact count",
            workload.id
        ));
    }

    let mut artifact_kinds = BTreeSet::new();

    for artifact in &workload.artifacts {
        if !artifact_kinds.insert(artifact.kind) {
            return Err(format!("workload {} repeats an artifact kind", workload.id));
        }

        validate_artifact(artifact)?;
        validate_runtime_dependencies(artifact, target.object_format())?;
    }

    validate_observation(&workload.observations.allocation_count)?;
    validate_observation(&workload.observations.allocated_bytes)?;
    validate_observation(&workload.observations.copied_bytes)?;

    if let Some(expected) = canonical.storage
        && (measured_value(&workload.observations.allocation_count)
            != Some(expected.allocation_count)
            || measured_value(&workload.observations.allocated_bytes)
                != Some(expected.allocated_bytes)
            || measured_value(&workload.observations.copied_bytes) != Some(expected.copied_bytes))
    {
        return Err(format!(
            "workload {} does not contain its required storage observations",
            workload.id
        ));
    }

    if canonical.storage.is_some()
        && [
            &workload.observations.allocation_count,
            &workload.observations.allocated_bytes,
            &workload.observations.copied_bytes,
        ]
        .into_iter()
        .any(|observation| {
            !matches!(
                observation,
                Observation::Measured { scope, .. }
                    if scope == super::model::STORAGE_OBSERVATION_SCOPE
            )
        })
    {
        return Err(format!(
            "workload {} has invalid storage observation scope",
            workload.id
        ));
    }

    if workload.observations.platform_operations.len() > MAX_PLATFORM_OPERATION_COUNT {
        return Err(format!(
            "workload {} has too many platform observations",
            workload.id
        ));
    }

    let expected_operations = canonical
        .platform_operations
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();

    let actual_operations = workload
        .observations
        .platform_operations
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();

    if actual_operations != expected_operations {
        return Err(format!(
            "workload {} platform observations do not match the canonical corpus",
            workload.id
        ));
    }

    for observation in workload.observations.platform_operations.values() {
        validate_observation(observation)?;
    }

    validate_peers(
        workload,
        expected_samples,
        target,
        controlled_inner_iterations,
        report.identity.timer_resolution_nanoseconds,
    )?;

    Ok(())
}

fn validate_compiler_profiles(
    comparison: &super::model::CompilationComparisonReport,
    target: NativeTarget,
    owner: &str,
) -> Result<(), String> {
    for language in [
        super::model::CompilationLanguage::Bray,
        super::model::CompilationLanguage::Rust,
        super::model::CompilationLanguage::Cpp,
    ] {
        let build = comparison
            .builds
            .get(&language)
            .ok_or_else(|| format!("{owner} is missing its {language:?} build"))?;

        match (language, build.profile.as_ref()) {
            (super::model::CompilationLanguage::Bray, Some(profile))
                if profile.context.target == target.as_str() => {}
            (super::model::CompilationLanguage::Bray, _) => {
                return Err(format!("{owner} has an invalid Bray compiler profile"));
            }
            (_, None) => {}
            (_, Some(_)) => {
                return Err(format!("{owner} attaches a Bray profile to a peer build"));
            }
        }
    }

    Ok(())
}

fn validate_batching(
    policy: BatchingPolicy,
    batching: &WorkloadBatching,
    selected_inner_iterations: NonZeroU64,
) -> Result<(), String> {
    match (policy, batching) {
        (BatchingPolicy::SingleExecution, WorkloadBatching::SingleExecution)
            if selected_inner_iterations == NonZeroU64::MIN =>
        {
            Ok(())
        }
        (
            BatchingPolicy::Calibrated,
            WorkloadBatching::Calibrated {
                seed_inner_iterations,
                target_interval_nanoseconds,
                bray_samples_nanoseconds,
                rust_samples_nanoseconds,
                cpp_samples_nanoseconds,
                selected_inner_iterations: recorded_inner_iterations,
            },
        ) => {
            let expected_sample_count =
                usize::try_from(CALIBRATION_SAMPLE_COUNT).map_err(|_| {
                    "calibration sample count cannot be represented by this host".to_owned()
                })?;

            let samples_are_valid = [
                bray_samples_nanoseconds,
                rust_samples_nanoseconds,
                cpp_samples_nanoseconds,
            ]
            .into_iter()
            .all(|samples| {
                samples.len() == expected_sample_count && samples.iter().all(|sample| *sample > 0)
            });

            let seed = NonZeroU64::new(*seed_inner_iterations)
                .ok_or_else(|| "batch calibration seed must be nonzero".to_owned())?;

            let expected = super::statistics::calibrated_inner_iterations(
                seed,
                *target_interval_nanoseconds,
                [
                    &bray_samples_nanoseconds[..],
                    &rust_samples_nanoseconds[..],
                    &cpp_samples_nanoseconds[..],
                ],
            )?;

            if *seed_inner_iterations != CALIBRATION_SEED_INNER_ITERATIONS
                || *target_interval_nanoseconds != CALIBRATION_TARGET_NANOSECONDS
                || *target_interval_nanoseconds
                    < MINIMUM_BATCH_INTERVAL_NANOSECONDS.saturating_mul(10)
                || !samples_are_valid
                || *recorded_inner_iterations != expected.get()
                || selected_inner_iterations != expected
            {
                return Err(
                    "workload batch calibration does not match its measured contract".to_owned(),
                );
            }

            Ok(())
        }
        _ => Err("workload batching mode does not match the corpus contract".to_owned()),
    }
}

fn validate_peers(
    workload: &super::model::WorkloadReport,
    expected_samples: usize,
    target: NativeTarget,
    controlled_inner_iterations: NonZeroU64,
    timer_resolution_nanoseconds: u64,
) -> Result<(), String> {
    let languages = workload.peers.keys().copied().collect::<BTreeSet<_>>();

    if languages
        != [PeerLanguage::Rust, PeerLanguage::Cpp]
            .into_iter()
            .collect()
    {
        return Err(format!(
            "workload {} does not contain both peer languages",
            workload.id
        ));
    }

    for (language, report) in &workload.peers {
        if report.artifacts.len() != 1 {
            return Err(format!(
                "workload {} {language:?} peer report has an invalid artifact count",
                workload.id
            ));
        }

        if report.toolchain.is_empty()
            || report.build_configuration.target != target.as_str()
            || report.build_configuration.production.arguments.is_empty()
            || report.build_configuration.timed.arguments.is_empty()
            || report.build_configuration.linker.is_empty()
            || report.build_configuration.runtime_linkage
                != super::peer::runtime_linkage(target)?
            || !super::peer::build_configuration_matches(
                *language,
                &workload.id,
                target,
                Path::new(&report.artifacts[0].path),
                &report.build_configuration,
                controlled_inner_iterations,
            )
            || report.build_configuration.post_link_actions.is_empty()
            || report.source_sha256.len() != 64
            || !is_lowercase_hex(&report.source_sha256)
            || report.process_execution.scope != super::model::PROCESS_EXECUTION_SCOPE
            || report.controlled_execution.scope != super::model::BRAY_EXECUTION_SCOPE
            || !execution_is_valid(
                &report.process_execution,
                expected_samples,
                workload.scale,
                1,
                timer_resolution_nanoseconds,
            )
            || !execution_is_valid(
                &report.controlled_execution,
                expected_samples,
                workload.scale,
                controlled_inner_iterations.get(),
                timer_resolution_nanoseconds,
            )
        {
            return Err(format!(
                "workload {} {language:?} peer report is invalid",
                workload.id
            ));
        }

        validate_artifact(&report.artifacts[0])?;
        validate_runtime_dependencies(&report.artifacts[0], target.object_format())?;
        validate_unavailable_peer_observations(&report.observations)?;
    }

    Ok(())
}

fn validate_runtime_dependencies(
    artifact: &ArtifactReport,
    object_format: ObjectFormat,
) -> Result<(), String> {
    let has_dynamic_runtime =
        artifact
            .dependencies
            .dynamic_libraries
            .entries
            .iter()
            .any(|library| {
                let library = library.to_ascii_lowercase();

                match object_format {
                    ObjectFormat::Coff => {
                        library.starts_with("api-ms-win-crt-")
                            || library.starts_with("msvcp")
                            || library.starts_with("msvcr")
                            || library.starts_with("vcruntime")
                            || library == "ucrtbase.dll"
                    }
                    ObjectFormat::Elf => {
                        library.starts_with("libstdc++.") || library.starts_with("libgcc_s.")
                    }
                    ObjectFormat::MachO | ObjectFormat::WebAssembly | ObjectFormat::Xcoff => true,
                }
            });

    if has_dynamic_runtime {
        return Err(format!(
            "{:?} artifact loads a dynamic application runtime",
            artifact.kind
        ));
    }

    Ok(())
}

fn validate_unavailable_peer_observations(
    observations: &super::model::WorkloadObservations,
) -> Result<(), String> {
    if !observations.platform_operations.is_empty()
        || [
            &observations.allocation_count,
            &observations.allocated_bytes,
            &observations.copied_bytes,
        ]
        .into_iter()
        .any(|observation| !matches!(observation, Observation::Unavailable { reason } if !reason.is_empty()))
    {
        return Err("peer observations must explain unavailable runtime measurements".to_owned());
    }

    Ok(())
}

fn execution_is_valid(
    execution: &super::model::ExecutionStatistics,
    expected_samples: usize,
    scale: u64,
    inner_iterations: u64,
    timer_resolution_nanoseconds: u64,
) -> bool {
    !execution.scope.is_empty()
        && execution.raw_samples_nanoseconds.len() == expected_samples
        && execution.samples_picoseconds.len() == expected_samples
        && execution.inner_iterations == inner_iterations
        && execution.timer_resolution_nanoseconds == timer_resolution_nanoseconds
        && execution.minimum_picoseconds > 0
        && execution.raw_samples_nanoseconds.iter().all(|sample| {
            inner_iterations == 1
                || (*sample >= MINIMUM_BATCH_INTERVAL_NANOSECONDS
                    && u128::from(*sample)
                        >= u128::from(timer_resolution_nanoseconds)
                            .saturating_mul(MINIMUM_TIMER_RESOLUTION_MULTIPLE.into()))
        })
        && super::statistics::summarize(
            execution.raw_samples_nanoseconds.clone(),
            scale,
            &execution.scope,
            inner_iterations,
            timer_resolution_nanoseconds,
        )
        .as_ref()
            == Some(execution)
}

fn validate_artifact(artifact: &ArtifactReport) -> Result<(), String> {
    if artifact.path.is_empty()
        || artifact.sections.entries.len() > MAX_SECTION_COUNT
        || artifact.dependencies.static_inputs.entries.len() > MAX_RETAINED_INPUT_COUNT
        || artifact.dependencies.dynamic_libraries.entries.len() > MAX_DYNAMIC_LIBRARY_COUNT
    {
        return Err(format!(
            "{:?} artifact is incomplete or outside its bounds",
            artifact.kind
        ));
    }

    if !artifact
        .sections
        .entries
        .windows(2)
        .all(|pair| pair[0].name < pair[1].name)
        || artifact
            .sections
            .entries
            .iter()
            .any(|section| section.name.is_empty())
        || !strictly_sorted(&artifact.dependencies.static_inputs.entries)
        || !strictly_sorted(&artifact.dependencies.dynamic_libraries.entries)
    {
        return Err(format!(
            "{:?} artifact collections are not canonical",
            artifact.kind
        ));
    }

    if let Some(map) = &artifact.linker_map
        && (map.sha256.len() != 64 || !is_lowercase_hex(&map.sha256))
    {
        return Err(format!(
            "{:?} artifact linker-map digest is invalid",
            artifact.kind
        ));
    }

    validate_retained_inputs(&artifact.dependencies.static_inputs)
}

fn validate_retained_inputs(inputs: &BoundedList<RetainedInput>) -> Result<(), String> {
    if inputs.entries.iter().any(|input| {
        input.artifact.is_empty() || input.member.as_ref().is_some_and(String::is_empty)
    }) {
        return Err("retained linker inputs must have nonempty identities".to_owned());
    }

    Ok(())
}

fn validate_observation(observation: &Observation) -> Result<(), String> {
    match observation {
        Observation::Measured { scope, .. } if scope.is_empty() => {
            Err("measured observations require a scope".to_owned())
        }
        Observation::Unavailable { reason } if reason.is_empty() => {
            Err("unavailable observations require a reason".to_owned())
        }
        _ => Ok(()),
    }
}

const fn measured_value(observation: &Observation) -> Option<u64> {
    match observation {
        Observation::Measured { value, .. } => Some(*value),
        Observation::Unavailable { .. } => None,
    }
}

fn strictly_sorted<T: Ord>(entries: &[T]) -> bool {
    entries.windows(2).all(|pair| pair[0] < pair[1])
}
