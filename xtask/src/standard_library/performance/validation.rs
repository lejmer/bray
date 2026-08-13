use std::collections::BTreeSet;

use bray_base::is_lowercase_hex;

use super::command::expected_output_digest;
use super::corpus::WORKLOADS;
use super::model::{
    ArtifactReport, BoundedList, MAX_DYNAMIC_LIBRARY_COUNT, MAX_PLATFORM_OPERATION_COUNT,
    MAX_RETAINED_INPUT_COUNT, MAX_SAMPLE_COUNT, MAX_SECTION_COUNT, Observation, PeerLanguage,
    PeerOutcome, PerformanceReport, RetainedInput, SCHEMA_REVISION,
};

pub(super) fn validate(report: &PerformanceReport) -> Result<(), String> {
    if report.schema_revision != SCHEMA_REVISION {
        return Err(format!("unsupported report schema revision: {}", report.schema_revision));
    }

    if report.workloads.is_empty() || report.workloads.len() > WORKLOADS.len() {
        return Err("report workload count is outside the canonical corpus bound".to_owned());
    }

    validate_identity(report)?;

    let mut identities = BTreeSet::new();

    for workload in &report.workloads {
        if !identities.insert(workload.id.as_str()) {
            return Err(format!("report repeats workload {}", workload.id));
        }

        validate_workload(report, workload)?;
    }

    Ok(())
}

fn validate_identity(report: &PerformanceReport) -> Result<(), String> {
    let identity = &report.identity;
    let sha_is_valid = |value: &str| value.len() == 64 && is_lowercase_hex(value);

    if identity.corpus_revision == 0
        || !sha_is_valid(&identity.corpus_sha256)
        || identity.target.is_empty()
        || identity.host.is_empty()
        || identity.build_configuration.is_empty()
        || identity.compiler_version.is_empty()
        || identity.source_revision.is_empty()
        || identity.llvm_version.is_empty()
        || identity.warmup_iterations == 0
        || identity.sample_iterations == 0
        || identity.sample_iterations > MAX_SAMPLE_COUNT
    {
        return Err("report identity is incomplete or outside its bounds".to_owned());
    }

    Ok(())
}

fn validate_workload(
    report: &PerformanceReport,
    workload: &super::model::WorkloadReport,
) -> Result<(), String> {
    let canonical = WORKLOADS
        .iter()
        .find(|candidate| candidate.id == workload.id)
        .ok_or_else(|| format!("report contains unknown workload {}", workload.id))?;

    let expected_output = expected_output_digest(canonical.expected_output)?;
    let expected_peer_contract = super::peer::comparison_contract(&workload.id);

    if workload.category != canonical.category
        || workload.scale != canonical.scale
        || workload.units != canonical.units
        || workload.expected_output_sha256 != expected_output
        || workload.peer_contract.as_deref() != expected_peer_contract
    {
        return Err(format!("workload {} does not match the canonical corpus", workload.id));
    }

    workload
        .compilation
        .validate()
        .map_err(|error| format!("workload {} has an invalid compiler profile: {error:?}", workload.id))?;

    if workload.compilation.context.target != report.identity.target {
        return Err(format!("workload {} compiler target differs from the report", workload.id));
    }

    let expected_samples = usize::try_from(report.identity.sample_iterations)
        .map_err(|_| "sample count cannot be represented by this host".to_owned())?;

    if workload.process_execution.scope != super::model::PROCESS_EXECUTION_SCOPE
        || workload.bray_execution.scope != super::model::BRAY_EXECUTION_SCOPE
        || !execution_is_valid(&workload.process_execution, expected_samples, workload.scale)
        || !execution_is_valid(&workload.bray_execution, expected_samples, workload.scale)
    {
        return Err(format!("workload {} has invalid execution statistics", workload.id));
    }

    if workload.artifacts.is_empty() || workload.artifacts.len() > 2 {
        return Err(format!("workload {} has an invalid artifact count", workload.id));
    }

    let mut artifact_kinds = BTreeSet::new();

    for artifact in &workload.artifacts {
        if !artifact_kinds.insert(artifact.kind) {
            return Err(format!("workload {} repeats an artifact kind", workload.id));
        }

        validate_artifact(artifact)?;
    }

    validate_observation(&workload.observations.allocation_count)?;
    validate_observation(&workload.observations.allocated_bytes)?;
    validate_observation(&workload.observations.copied_bytes)?;

    if let Some(expected) = canonical.storage
        && (
            measured_value(&workload.observations.allocation_count)
                != Some(expected.allocation_count)
                || measured_value(&workload.observations.allocated_bytes)
                    != Some(expected.allocated_bytes)
                || measured_value(&workload.observations.copied_bytes)
                    != Some(expected.copied_bytes)
        )
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
        return Err(format!("workload {} has too many platform observations", workload.id));
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

    validate_peers(workload, expected_samples, &report.identity.target)?;

    Ok(())
}

fn validate_peers(
    workload: &super::model::WorkloadReport,
    expected_samples: usize,
    expected_target: &str,
) -> Result<(), String> {
    let languages = workload.peers.keys().copied().collect::<BTreeSet<_>>();

    if languages != [PeerLanguage::Rust, PeerLanguage::Cpp].into_iter().collect() {
        return Err(format!(
            "workload {} does not contain both peer languages",
            workload.id
        ));
    }

    let expected_support_reason = super::peer::support_reason(&workload.id);

    for (language, outcome) in &workload.peers {
        match (expected_support_reason, outcome) {
            (Some(expected), PeerOutcome::Unsupported { reason }) if reason == expected => {}
            (None, PeerOutcome::Measured { report }) => {
                if report.toolchain.is_empty()
                    || report.build_configuration.target != expected_target
                    || report.build_configuration.target.is_empty()
                    || report.build_configuration.production_arguments.is_empty()
                    || report.build_configuration.timed_arguments.is_empty()
                    || report.build_configuration.linker.is_empty()
                    || report.build_configuration.runtime_linkage.is_empty()
                    || report.build_configuration.post_link_actions.is_empty()
                    || report.source_sha256.len() != 64
                    || !is_lowercase_hex(&report.source_sha256)
                    || report.production_compile_link_nanoseconds == 0
                    || report.process_execution.scope != super::model::PROCESS_EXECUTION_SCOPE
                    || report.controlled_execution.scope != super::model::BRAY_EXECUTION_SCOPE
                    || !execution_is_valid(
                        &report.process_execution,
                        expected_samples,
                        workload.scale,
                    )
                    || !execution_is_valid(
                        &report.controlled_execution,
                        expected_samples,
                        workload.scale,
                    )
                    || report.artifacts.len() != 1
                {
                    return Err(format!(
                        "workload {} {language:?} peer report is invalid",
                        workload.id
                    ));
                }

                validate_artifact(&report.artifacts[0])?;
                validate_unavailable_peer_observations(&report.observations)?;
            }
            _ => {
                return Err(format!(
                    "workload {} {language:?} peer support does not match its corpus contract",
                    workload.id
                ));
            }
        }
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
) -> bool {
    !execution.scope.is_empty()
        && execution.samples_nanoseconds.len() == expected_samples
        && super::statistics::summarize(
            execution.samples_nanoseconds.clone(),
            scale,
            &execution.scope,
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
        return Err(format!("{:?} artifact is incomplete or outside its bounds", artifact.kind));
    }

    if !artifact
        .sections
        .entries
        .windows(2)
        .all(|pair| pair[0].name < pair[1].name)
        || artifact.sections.entries.iter().any(|section| section.name.is_empty())
        || !strictly_sorted(&artifact.dependencies.static_inputs.entries)
        || !strictly_sorted(&artifact.dependencies.dynamic_libraries.entries)
    {
        return Err(format!("{:?} artifact collections are not canonical", artifact.kind));
    }

    if let Some(map) = &artifact.linker_map
        && (map.sha256.len() != 64 || !is_lowercase_hex(&map.sha256))
    {
        return Err(format!("{:?} artifact linker-map digest is invalid", artifact.kind));
    }

    validate_retained_inputs(&artifact.dependencies.static_inputs)
}

fn validate_retained_inputs(inputs: &BoundedList<RetainedInput>) -> Result<(), String> {
    if inputs
        .entries
        .iter()
        .any(|input| input.artifact.is_empty() || input.member.as_ref().is_some_and(String::is_empty))
    {
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
