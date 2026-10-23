use std::collections::BTreeMap;

use super::super::model::{
    ArtifactKind, CompilationLanguage, ExecutionStatistics, ObservationComparison,
    PerformanceReport,
};
use super::model::{Policy, TimingEvidence};

pub(super) fn structural(
    comparison: &super::super::model::WorkloadComparison,
    baseline: &super::super::model::WorkloadReport,
    candidate: &super::super::model::WorkloadReport,
) -> BTreeMap<String, (u64, u64)> {
    let mut metrics = BTreeMap::new();

    for artifact in &comparison.artifacts {
        let kind = match artifact.kind {
            ArtifactKind::Executable => "executable",
            ArtifactKind::RelocatableObject => "object",
        };

        metrics.insert(
            format!("{kind}.bytes"),
            (artifact.bytes.baseline, artifact.bytes.candidate),
        );

        for (name, section) in &artifact.sections {
            metrics.insert(
                format!("{kind}.section.{name}.bytes"),
                (section.baseline, section.candidate),
            );
        }
    }

    for (name, metric) in &comparison.compiler_operations {
        metrics.insert(
            format!("compiler.operation.{name}"),
            (metric.baseline, metric.candidate),
        );
    }

    for (name, metric) in &comparison.compiler_metrics {
        let descriptor = |workload: &super::super::model::WorkloadReport| {
            workload
                .compiler_profile
                .descriptors
                .metrics
                .iter()
                .find(|descriptor| descriptor.name == *name)
                .map(|descriptor| descriptor.unit)
        };

        let unit = descriptor(baseline);

        if unit != descriptor(candidate)
            || !matches!(
                unit,
                Some(
                    bray_compilation::CompilationProfileUnit::Count
                        | bray_compilation::CompilationProfileUnit::Bytes
                )
            )
        {
            continue;
        }

        metrics.insert(
            format!("compiler.metric.{name}"),
            (metric.baseline, metric.candidate),
        );
    }

    let observations = &comparison.observations;

    for (name, observation) in [
        ("allocation_count", &observations.allocation_count),
        ("allocated_bytes", &observations.allocated_bytes),
        ("copied_bytes", &observations.copied_bytes),
    ]
    .into_iter()
    .chain(
        observations
            .platform_operations
            .iter()
            .map(|(name, value)| (name.as_str(), value)),
    ) {
        if let ObservationComparison::Measured { comparison, .. } = observation {
            metrics.insert(
                format!("observation.{name}"),
                (comparison.baseline, comparison.candidate),
            );
        }
    }

    metrics
}

pub(super) fn timing(
    baseline: &ExecutionStatistics,
    candidate: &ExecutionStatistics,
    policy: &Policy,
) -> TimingEvidence {
    let resolution = |statistics: &ExecutionStatistics| {
        u64::try_from(
            (u128::from(statistics.timer_resolution_nanoseconds) * 1_000)
                .div_ceil(u128::from(statistics.inner_iterations)),
        )
        .unwrap_or(u64::MAX)
        .max(1)
    };

    let resolution_bound = resolution(baseline).saturating_add(resolution(candidate));

    let intervals_resolved = [baseline, candidate].iter().all(|statistics| {
        statistics.raw_samples_nanoseconds.iter().all(|sample| {
            u128::from(*sample)
                >= u128::from(statistics.timer_resolution_nanoseconds)
                    * u128::from(policy.resolution_multiple)
        })
    });

    TimingEvidence {
        baseline: baseline.median_picoseconds,
        candidate: candidate.median_picoseconds,
        baseline_samples: baseline.samples_picoseconds.len(),
        candidate_samples: candidate.samples_picoseconds.len(),
        noise_bound: baseline
            .median_absolute_deviation_picoseconds
            .saturating_add(candidate.median_absolute_deviation_picoseconds)
            .saturating_mul(policy.mad_multiplier),
        resolution_bound,
        resolved: intervals_resolved
            && baseline.samples_picoseconds.len() >= policy.minimum_samples
            && candidate.samples_picoseconds.len() >= policy.minimum_samples,
    }
}

pub(super) fn single_timings(report: &PerformanceReport) -> BTreeMap<(String, String), u64> {
    let mut metrics = BTreeMap::new();

    for workload in &report.workloads {
        metrics.insert(
            (
                workload.id.clone(),
                "compilation_process.nanoseconds".to_owned(),
            ),
            workload.compilation.process_elapsed_nanoseconds,
        );

        metrics.insert(
            (workload.id.clone(), "compiler_work.nanoseconds".to_owned()),
            workload.compilation.compiler_elapsed_nanoseconds,
        );
    }

    for (name, compilation) in [
        ("application_compilation", &report.application_compilation),
        ("library_compilation", &report.library_compilation),
    ] {
        let bray = compilation
            .builds
            .get(&CompilationLanguage::Bray)
            .expect("validated comparison contains Bray compilation");

        metrics.insert(
            (
                name.to_owned(),
                "compilation_process.nanoseconds".to_owned(),
            ),
            bray.elapsed_nanoseconds,
        );
    }

    metrics
}
