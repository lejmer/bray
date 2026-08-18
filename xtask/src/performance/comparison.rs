use std::collections::{BTreeMap, BTreeSet};

use super::model::{
    ArtifactComparison, ArtifactKind, ChangeAssessment, ComparisonReport,
    CompilationComparisonReport, CompilationLanguage, MetricComparison, Observation,
    ObservationComparison, ObservationComparisonReport, PeerComparison, PerformanceReport,
    SCHEMA_REVISION, WorkloadComparison,
};

pub(super) fn compare(
    baseline: &PerformanceReport,
    candidate: &PerformanceReport,
) -> Result<ComparisonReport, String> {
    super::validation::validate(baseline)?;
    super::validation::validate(candidate)?;

    validate_identity(baseline, candidate)?;

    let application_compilation = compare_compilation(
        &baseline.application_compilation,
        &candidate.application_compilation,
        "application compilation",
    )?;

    let library_compilation = compare_compilation(
        &baseline.library_compilation,
        &candidate.library_compilation,
        "library compilation",
    )?;

    if baseline.workloads.len() != candidate.workloads.len() {
        return Err("baseline and candidate workload counts differ".to_owned());
    }

    let mut workloads = Vec::with_capacity(candidate.workloads.len());

    for candidate_workload in &candidate.workloads {
        let baseline_workload = baseline
            .workloads
            .iter()
            .find(|workload| workload.id == candidate_workload.id)
            .ok_or_else(|| format!("baseline is missing workload {}", candidate_workload.id))?;

        if baseline_workload.category != candidate_workload.category
            || baseline_workload.peer_contract != candidate_workload.peer_contract
            || baseline_workload.scale != candidate_workload.scale
            || baseline_workload.units != candidate_workload.units
            || baseline_workload.expected_output_sha256 != candidate_workload.expected_output_sha256
            || baseline_workload.process_execution.scope
                != candidate_workload.process_execution.scope
            || baseline_workload.bray_execution.scope != candidate_workload.bray_execution.scope
        {
            return Err(format!(
                "workload {} does not have equivalent inputs and output",
                candidate_workload.id
            ));
        }

        let process_execution = noisy_metric(
            baseline_workload.process_execution.median_picoseconds,
            candidate_workload.process_execution.median_picoseconds,
            baseline_workload
                .process_execution
                .median_absolute_deviation_picoseconds,
            candidate_workload
                .process_execution
                .median_absolute_deviation_picoseconds,
        );

        let bray_execution = noisy_metric(
            baseline_workload.bray_execution.median_picoseconds,
            candidate_workload.bray_execution.median_picoseconds,
            baseline_workload
                .bray_execution
                .median_absolute_deviation_picoseconds,
            candidate_workload
                .bray_execution
                .median_absolute_deviation_picoseconds,
        );

        workloads.push(WorkloadComparison {
            id: candidate_workload.id.clone(),
            process_execution,
            bray_execution,
            compiler_operations: compare_compiler_operations(baseline_workload, candidate_workload),
            compiler_metrics: compare_compiler_metrics(baseline_workload, candidate_workload),
            artifacts: compare_artifacts(baseline_workload, candidate_workload)?,
            observations: compare_observations(baseline_workload, candidate_workload),
            peers: compare_peers(baseline_workload, candidate_workload)?,
        });
    }

    Ok(ComparisonReport {
        schema_revision: SCHEMA_REVISION,
        baseline_identity: baseline.identity.clone(),
        candidate_identity: candidate.identity.clone(),
        application_compilation,
        library_compilation,
        workloads,
    })
}

fn compare_compilation(
    baseline: &CompilationComparisonReport,
    candidate: &CompilationComparisonReport,
    owner: &str,
) -> Result<BTreeMap<CompilationLanguage, MetricComparison>, String> {
    if baseline.kind != candidate.kind
        || baseline.contract != candidate.contract
        || baseline.comparability != candidate.comparability
        || baseline.builds.keys().ne(candidate.builds.keys())
    {
        return Err(format!("baseline and candidate {owner} contracts differ"));
    }

    baseline
        .builds
        .iter()
        .map(|(language, baseline)| {
            let candidate = candidate
                .builds
                .get(language)
                .ok_or_else(|| format!("candidate {owner} omitted {language:?}"))?;

            if baseline.toolchain != candidate.toolchain
                || baseline.source_sha256 != candidate.source_sha256
                || baseline.authority != candidate.authority
            {
                return Err(format!(
                    "baseline and candidate {owner} authority differs for {language:?}"
                ));
            }

            Ok((
                *language,
                observed_metric(baseline.elapsed_nanoseconds, candidate.elapsed_nanoseconds),
            ))
        })
        .collect()
}

fn validate_identity(
    baseline: &PerformanceReport,
    candidate: &PerformanceReport,
) -> Result<(), String> {
    let left = &baseline.identity;
    let right = &candidate.identity;

    if baseline.schema_revision != candidate.schema_revision
        || left.corpus_revision != right.corpus_revision
        || left.corpus_sha256 != right.corpus_sha256
        || left.target != right.target
        || left.host != right.host
        || left.build_configuration != right.build_configuration
        || left.compiler_version != right.compiler_version
        || left.llvm_version != right.llvm_version
        || left.runtime_linkage != right.runtime_linkage
        || left.warmup_iterations != right.warmup_iterations
        || left.sample_iterations != right.sample_iterations
    {
        return Err("baseline and candidate identities are not equivalent".to_owned());
    }

    Ok(())
}

fn compare_peers(
    baseline: &super::model::WorkloadReport,
    candidate: &super::model::WorkloadReport,
) -> Result<BTreeMap<super::model::PeerLanguage, PeerComparison>, String> {
    if baseline.peers.keys().ne(candidate.peers.keys()) {
        return Err(format!(
            "workload {} peer language sets differ",
            candidate.id
        ));
    }

    let workload_id = &candidate.id;

    baseline
        .peers
        .iter()
        .map(|(language, baseline)| {
            let candidate = candidate
                .peers
                .get(language)
                .ok_or_else(|| "candidate peer disappeared during comparison".to_owned())?;

            if baseline.toolchain != candidate.toolchain
                || baseline.source_sha256 != candidate.source_sha256
                || baseline.build_configuration.target != candidate.build_configuration.target
                || baseline.build_configuration.compiler != candidate.build_configuration.compiler
                || baseline.build_configuration.production
                    != candidate.build_configuration.production
                || baseline.build_configuration.linker != candidate.build_configuration.linker
                || baseline.build_configuration.runtime_linkage
                    != candidate.build_configuration.runtime_linkage
                || baseline.build_configuration.post_link_actions
                    != candidate.build_configuration.post_link_actions
                || baseline.process_execution.scope != candidate.process_execution.scope
                || baseline.controlled_execution.scope != candidate.controlled_execution.scope
            {
                return Err(format!(
                    "workload {} {language:?} peer configurations differ",
                    workload_id
                ));
            }

            let comparison = PeerComparison {
                process_execution: noisy_metric(
                    baseline.process_execution.median_picoseconds,
                    candidate.process_execution.median_picoseconds,
                    baseline
                        .process_execution
                        .median_absolute_deviation_picoseconds,
                    candidate
                        .process_execution
                        .median_absolute_deviation_picoseconds,
                ),
                controlled_execution: noisy_metric(
                    baseline.controlled_execution.median_picoseconds,
                    candidate.controlled_execution.median_picoseconds,
                    baseline
                        .controlled_execution
                        .median_absolute_deviation_picoseconds,
                    candidate
                        .controlled_execution
                        .median_absolute_deviation_picoseconds,
                ),
                artifacts: compare_artifact_sets(
                    &baseline.artifacts,
                    &candidate.artifacts,
                    &format!("{:?} peer", language),
                )?,
            };

            Ok((*language, comparison))
        })
        .collect()
}

fn compare_compiler_operations(
    baseline: &super::model::WorkloadReport,
    candidate: &super::model::WorkloadReport,
) -> BTreeMap<String, MetricComparison> {
    let baseline = compiler_operations(baseline);
    let candidate = compiler_operations(candidate);

    compare_metric_maps_with(&baseline, &candidate, observed_metric)
}

fn compiler_operations(workload: &super::model::WorkloadReport) -> BTreeMap<String, u64> {
    let Some(profile) = bray_compiler_profile(workload) else {
        return BTreeMap::new();
    };

    profile
        .operations
        .iter()
        .filter_map(|statistics| {
            profile
                .operation_descriptor(statistics.id)
                .map(|descriptor| (descriptor.name.clone(), statistics.self_nanoseconds))
        })
        .collect()
}

fn compare_compiler_metrics(
    baseline: &super::model::WorkloadReport,
    candidate: &super::model::WorkloadReport,
) -> BTreeMap<String, MetricComparison> {
    let baseline = compiler_metrics(baseline);
    let candidate = compiler_metrics(candidate);

    compare_metric_maps_with(&baseline, &candidate, observed_metric)
}

fn compiler_metrics(workload: &super::model::WorkloadReport) -> BTreeMap<String, u64> {
    let Some(profile) = bray_compiler_profile(workload) else {
        return BTreeMap::new();
    };

    profile
        .metrics
        .iter()
        .filter_map(|metric| {
            profile
                .metric_descriptor(metric.id)
                .map(|descriptor| (descriptor.name.clone(), metric.value))
        })
        .collect()
}

fn bray_compiler_profile(
    workload: &super::model::WorkloadReport,
) -> Option<&bray_compilation::CompilationProfileReport> {
    Some(&workload.compiler_profile)
}

fn compare_metric_maps(
    baseline: &BTreeMap<String, u64>,
    candidate: &BTreeMap<String, u64>,
) -> BTreeMap<String, MetricComparison> {
    compare_metric_maps_with(baseline, candidate, exact_metric)
}

fn compare_metric_maps_with(
    baseline: &BTreeMap<String, u64>,
    candidate: &BTreeMap<String, u64>,
    compare: fn(u64, u64) -> MetricComparison,
) -> BTreeMap<String, MetricComparison> {
    baseline
        .keys()
        .chain(candidate.keys())
        .map(|name| {
            (
                name.clone(),
                compare(
                    baseline.get(name).copied().unwrap_or(0),
                    candidate.get(name).copied().unwrap_or(0),
                ),
            )
        })
        .collect()
}

fn observed_metric(baseline: u64, candidate: u64) -> MetricComparison {
    metric(baseline, candidate, ChangeAssessment::Indeterminate)
}

fn compare_artifacts(
    baseline: &super::model::WorkloadReport,
    candidate: &super::model::WorkloadReport,
) -> Result<Vec<ArtifactComparison>, String> {
    compare_artifact_sets(
        &baseline.artifacts,
        &candidate.artifacts,
        &format!("workload {}", baseline.id),
    )
}

fn compare_artifact_sets(
    baseline: &[super::model::ArtifactReport],
    candidate: &[super::model::ArtifactReport],
    owner: &str,
) -> Result<Vec<ArtifactComparison>, String> {
    let kinds: BTreeSet<_> = baseline
        .iter()
        .chain(candidate)
        .map(|artifact| artifact.kind)
        .collect();

    kinds
        .into_iter()
        .map(|kind| compare_artifact(kind, baseline, candidate, owner))
        .collect()
}

fn compare_artifact(
    kind: ArtifactKind,
    baseline: &[super::model::ArtifactReport],
    candidate: &[super::model::ArtifactReport],
    owner: &str,
) -> Result<ArtifactComparison, String> {
    let baseline = artifact(baseline, kind, owner)?;
    let candidate = artifact(candidate, kind, owner)?;
    let baseline_sections = section_map(baseline);
    let candidate_sections = section_map(candidate);

    let baseline_static: BTreeSet<_> = baseline
        .dependencies
        .static_inputs
        .entries
        .iter()
        .cloned()
        .collect();

    let candidate_static: BTreeSet<_> = candidate
        .dependencies
        .static_inputs
        .entries
        .iter()
        .cloned()
        .collect();

    let baseline_dynamic: BTreeSet<_> = baseline
        .dependencies
        .dynamic_libraries
        .entries
        .iter()
        .cloned()
        .collect();

    let candidate_dynamic: BTreeSet<_> = candidate
        .dependencies
        .dynamic_libraries
        .entries
        .iter()
        .cloned()
        .collect();

    Ok(ArtifactComparison {
        kind,
        bytes: exact_metric(baseline.bytes, candidate.bytes),
        sections: compare_metric_maps(&baseline_sections, &candidate_sections),
        omitted_sections: exact_metric(
            baseline.sections.omitted_count,
            candidate.sections.omitted_count,
        ),
        added_static_inputs: difference(&candidate_static, &baseline_static),
        removed_static_inputs: difference(&baseline_static, &candidate_static),
        omitted_static_inputs: exact_metric(
            baseline.dependencies.static_inputs.omitted_count,
            candidate.dependencies.static_inputs.omitted_count,
        ),
        added_dynamic_libraries: difference(&candidate_dynamic, &baseline_dynamic),
        removed_dynamic_libraries: difference(&baseline_dynamic, &candidate_dynamic),
        omitted_dynamic_libraries: exact_metric(
            baseline.dependencies.dynamic_libraries.omitted_count,
            candidate.dependencies.dynamic_libraries.omitted_count,
        ),
        linker_map_bytes: compare_linker_map_bytes(baseline, candidate)?,
    })
}

fn compare_linker_map_bytes(
    baseline: &super::model::ArtifactReport,
    candidate: &super::model::ArtifactReport,
) -> Result<Option<MetricComparison>, String> {
    match (&baseline.linker_map, &candidate.linker_map) {
        (Some(baseline), Some(candidate)) => {
            Ok(Some(exact_metric(baseline.bytes, candidate.bytes)))
        }
        (None, None) => Ok(None),
        _ => Err(format!(
            "{:?} artifact linker-map availability differs between reports",
            baseline.kind
        )),
    }
}

fn artifact<'a>(
    artifacts: &'a [super::model::ArtifactReport],
    kind: ArtifactKind,
    owner: &str,
) -> Result<&'a super::model::ArtifactReport, String> {
    let mut matching = artifacts.iter().filter(|artifact| artifact.kind == kind);

    let artifact = matching
        .next()
        .ok_or_else(|| format!("{owner} has no {kind:?} artifact"))?;

    if matching.next().is_some() {
        return Err(format!("{owner} has duplicate {kind:?} artifacts"));
    }

    Ok(artifact)
}

fn difference<T: Clone + Ord>(left: &BTreeSet<T>, right: &BTreeSet<T>) -> Vec<T> {
    left.difference(right).cloned().collect()
}

fn section_map(artifact: &super::model::ArtifactReport) -> BTreeMap<String, u64> {
    artifact
        .sections
        .entries
        .iter()
        .map(|section| (section.name.clone(), section.bytes))
        .collect()
}

fn compare_observations(
    baseline: &super::model::WorkloadReport,
    candidate: &super::model::WorkloadReport,
) -> ObservationComparisonReport {
    let baseline_observations = &baseline.observations;
    let candidate_observations = &candidate.observations;

    let names: BTreeSet<_> = baseline_observations
        .platform_operations
        .keys()
        .chain(candidate_observations.platform_operations.keys())
        .cloned()
        .collect();

    let platform_operations = names
        .into_iter()
        .map(|name| {
            let baseline = baseline_observations
                .platform_operations
                .get(&name)
                .cloned()
                .unwrap_or_else(missing_observation);

            let candidate = candidate_observations
                .platform_operations
                .get(&name)
                .cloned()
                .unwrap_or_else(missing_observation);

            (name, compare_observation(&baseline, &candidate))
        })
        .collect();

    ObservationComparisonReport {
        allocation_count: compare_observation(
            &baseline_observations.allocation_count,
            &candidate_observations.allocation_count,
        ),
        allocated_bytes: compare_observation(
            &baseline_observations.allocated_bytes,
            &candidate_observations.allocated_bytes,
        ),
        copied_bytes: compare_observation(
            &baseline_observations.copied_bytes,
            &candidate_observations.copied_bytes,
        ),
        platform_operations,
    }
}

fn compare_observation(baseline: &Observation, candidate: &Observation) -> ObservationComparison {
    match (baseline, candidate) {
        (
            Observation::Measured {
                value: baseline,
                scope: baseline_scope,
            },
            Observation::Measured {
                value: candidate,
                scope: candidate_scope,
            },
        ) if baseline_scope == candidate_scope => ObservationComparison::Measured {
            comparison: exact_metric(*baseline, *candidate),
            scope: baseline_scope.clone(),
        },
        _ => ObservationComparison::Incomparable {
            baseline: baseline.clone(),
            candidate: candidate.clone(),
        },
    }
}

fn missing_observation() -> Observation {
    Observation::Unavailable {
        reason: "observation is absent from this report".to_owned(),
    }
}

fn noisy_metric(
    baseline: u64,
    candidate: u64,
    baseline_mad: u64,
    candidate_mad: u64,
) -> MetricComparison {
    let noise = baseline_mad.saturating_add(candidate_mad).saturating_mul(3);

    let assessment = if baseline.abs_diff(candidate) <= noise {
        ChangeAssessment::Indeterminate
    } else if candidate < baseline {
        ChangeAssessment::Improved
    } else {
        ChangeAssessment::Regressed
    };

    metric(baseline, candidate, assessment)
}

fn exact_metric(baseline: u64, candidate: u64) -> MetricComparison {
    let assessment = match candidate.cmp(&baseline) {
        std::cmp::Ordering::Less => ChangeAssessment::Improved,
        std::cmp::Ordering::Equal => ChangeAssessment::Indeterminate,
        std::cmp::Ordering::Greater => ChangeAssessment::Regressed,
    };

    metric(baseline, candidate, assessment)
}

fn metric(baseline: u64, candidate: u64, assessment: ChangeAssessment) -> MetricComparison {
    let delta = i128::from(candidate) - i128::from(baseline);

    let delta_basis_points = if baseline == 0 {
        None
    } else {
        Some(delta.saturating_mul(10_000) / i128::from(baseline))
    };

    MetricComparison {
        baseline,
        candidate,
        delta,
        delta_basis_points,
        assessment,
    }
}
