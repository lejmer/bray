use std::collections::BTreeSet;

use bray_base::is_lowercase_hex;

use super::command::expected_output_digest;
use super::corpus::WORKLOADS;
use super::model::{
    ArtifactReport, BoundedList, Observation, PerformanceReport, RetainedInput, SCHEMA_REVISION,
    MAX_DYNAMIC_LIBRARY_COUNT, MAX_PLATFORM_OPERATION_COUNT, MAX_RETAINED_INPUT_COUNT,
    MAX_SAMPLE_COUNT, MAX_SECTION_COUNT,
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

    if workload.category != canonical.category
        || workload.scale != canonical.scale
        || workload.units != canonical.units
        || workload.expected_output_sha256 != expected_output
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

    if workload.execution.samples_nanoseconds.len() != expected_samples
        || super::statistics::summarize(
            workload.execution.samples_nanoseconds.clone(),
            workload.scale,
        )
        .as_ref()
            != Some(&workload.execution)
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

    Ok(())
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
