use std::fs;
use std::path::Path;

use super::model::{ChangeAssessment, ComparisonReport, PerformanceReport};

pub(super) fn write_summary(path: &Path, report: &PerformanceReport) -> Result<(), String> {
    let mut text = format!(
        "Standard-library performance report\nTarget: {}\nCorpus: {}\n\n",
        report.identity.target, report.identity.corpus_sha256
    );

    for workload in &report.workloads {
        let executable = workload
            .artifacts
            .iter()
            .find(|artifact| artifact.kind == super::model::ArtifactKind::Executable);

        text.push_str(&format!(
            "{}: median {} ns, MAD {} ns, {} {}/s, executable {} bytes\n",
            workload.id,
            workload.execution.median_nanoseconds,
            workload.execution.median_absolute_deviation_nanoseconds,
            workload.execution.median_units_per_second,
            workload.units,
            executable.map_or(0, |artifact| artifact.bytes),
        ));
    }

    fs::write(path, text).map_err(|error| format!("could not write {}: {error}", path.display()))
}

pub(super) fn write_comparison_summary(
    path: &Path,
    comparison: &ComparisonReport,
) -> Result<(), String> {
    let mut text = String::from("Standard-library performance comparison\n\n");

    for workload in &comparison.workloads {
        let executable = workload
            .artifacts
            .iter()
            .find(|artifact| artifact.kind == super::model::ArtifactKind::Executable);

        text.push_str(&format!(
            "{}: runtime {} ({}), executable {} ({} bytes)\n",
            workload.id,
            assessment(workload.execution.assessment),
            basis_points(workload.execution.delta_basis_points),
            executable.map_or("unavailable", |artifact| assessment(artifact.bytes.assessment)),
            executable.map_or(0, |artifact| artifact.bytes.delta),
        ));
    }

    fs::write(path, text).map_err(|error| format!("could not write {}: {error}", path.display()))
}

fn basis_points(value: Option<i128>) -> String {
    value.map_or_else(|| "no baseline ratio".to_owned(), |value| format!("{value} bp"))
}

const fn assessment(value: ChangeAssessment) -> &'static str {
    match value {
        ChangeAssessment::Improved => "improved",
        ChangeAssessment::Regressed => "regressed",
        ChangeAssessment::Indeterminate => "indeterminate",
    }
}
