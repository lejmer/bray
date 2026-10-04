use std::fs;
use std::io::Read;
use std::path::Path;

use super::super::comparison::compare;
use super::super::model::PerformanceReport;
use super::super::{report, validation};
use super::progress;

const MAX_BASELINE_REPORT_BYTES: usize = 16 * 1024 * 1024;

pub(super) fn publish(
    output: &Path,
    baseline_path: Option<&Path>,
    candidate: &mut PerformanceReport,
) -> Result<(), String> {
    validate_candidate(output, candidate)?;
    candidate.conformance_failures = validation::conformance_failures(candidate)?;

    let candidate_path = output.join("candidate.json");
    let candidate_html = output.join("candidate.html");

    crate::json::write_pretty(&candidate_path, candidate)?;
    report::write_candidate(&candidate_html, candidate)?;
    progress::report("Candidate HTML", &candidate_html);

    let mut failures = candidate.conformance_failures.clone();

    if let Some(path) = baseline_path {
        progress::phase("Comparing performance reports");

        let bytes = read_baseline(path)?;

        let baseline: PerformanceReport = serde_json::from_slice(&bytes)
            .map_err(|error| format!("baseline {} is invalid: {error}", path.display()))?;

        let comparison = compare(&baseline, candidate)?;
        let comparison_html = output.join("comparison.html");

        crate::json::write_pretty(&output.join("comparison.json"), &comparison)?;
        report::write_comparison(&comparison_html, &comparison)?;
        progress::report("Comparison HTML", &comparison_html);

        failures.extend(
            comparison
                .baseline_conformance_failures
                .iter()
                .map(|failure| format!("baseline: {failure}")),
        );
    }

    println!("{}", candidate_path.display());

    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "performance conformance failed after measurement:\n- {}",
            failures.join("\n- ")
        ))
    }
}

fn validate_candidate(output: &Path, candidate: &PerformanceReport) -> Result<(), String> {
    if let Err(error) = validation::validate_measurements(candidate) {
        if let Err(write_error) =
            crate::json::write_pretty(&output.join("candidate-unvalidated.json"), candidate)
        {
            return Err(format!("{error}\n{write_error}"));
        }

        return Err(error);
    }

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

#[cfg(test)]
mod tests {
    use super::{publish, validate_candidate};

    #[test]
    fn invalid_candidates_preserve_measurements_without_publishing_an_accepted_report() {
        let directory = tempfile::tempdir().expect("diagnostic output directory");
        let mut candidate = super::super::super::tests::report("corpus", 100, 1);

        candidate.schema_revision = 0;

        let error = validate_candidate(directory.path(), &candidate)
            .expect_err("invalid candidate remains rejected");

        assert!(error.contains("unsupported report schema revision"));

        let bytes = std::fs::read(directory.path().join("candidate-unvalidated.json"))
            .expect("unvalidated measurements");

        let retained: super::super::super::model::PerformanceReport =
            serde_json::from_slice(&bytes).expect("existing report format");

        assert_eq!(retained.schema_revision, 0);
        assert_eq!(retained.workloads.len(), candidate.workloads.len());

        assert_eq!(
            retained.workloads[0].bray_execution,
            candidate.workloads[0].bray_execution
        );

        assert!(!directory.path().join("candidate.json").exists());
        assert!(!directory.path().join("candidate.html").exists());
    }

    #[test]
    fn diagnostic_write_failure_preserves_the_validation_rejection() {
        let directory = tempfile::tempdir().expect("diagnostic output directory");
        let mut candidate = super::super::super::tests::report("corpus", 100, 1);

        candidate.schema_revision = 0;

        let error = validate_candidate(&directory.path().join("missing"), &candidate)
            .expect_err("both failures remain visible");

        assert!(error.contains("unsupported report schema revision"));
        assert!(error.contains("could not write"));
    }

    #[test]
    fn completed_runtime_mismatches_publish_failed_reports_and_qualified_comparisons() {
        let directory = tempfile::tempdir().expect("report directory");
        let baseline = crate::performance::tests::report("corpus", 100, 1);
        let baseline_path = directory.path().join("baseline.json");

        crate::json::write_pretty(&baseline_path, &baseline).expect("baseline writes");

        let mut candidate = baseline.clone();

        candidate.workloads[0].artifacts[0]
            .dependencies
            .static_archives
            .entries
            .insert(0, "libcmt.lib".to_owned());

        candidate
            .conformance_failures
            .push("retained forbidden provider <callback>".to_owned());

        let error = publish(directory.path(), Some(&baseline_path), &mut candidate)
            .expect_err("conformance remains unsuccessful");

        assert!(error.contains("retains static Windows CRT input libcmt.lib"));
        assert!(error.contains("retained forbidden provider <callback>"));

        for file in [
            "candidate.json",
            "candidate.html",
            "comparison.json",
            "comparison.html",
        ] {
            assert!(directory.path().join(file).is_file(), "{file}");
        }

        let html = std::fs::read_to_string(directory.path().join("candidate.html"))
            .expect("candidate HTML");

        assert!(html.contains("Candidate conformance: FAILED"));
        assert!(html.contains("retained forbidden provider &lt;callback&gt;"));
        assert!(html.contains("Actual dynamic libraries: "));
        assert!(html.contains("Controlled median"));
        assert!(html.contains("Compiler breakdown"));
        assert!(!html.contains("class=\"metric-best\""));

        let comparison: crate::performance::model::ComparisonReport = serde_json::from_slice(
            &std::fs::read(directory.path().join("comparison.json")).expect("comparison bytes"),
        )
        .expect("comparison model");

        assert!(comparison.baseline_conformance_failures.is_empty());

        assert_eq!(
            comparison.candidate_conformance_failures,
            candidate.conformance_failures
        );

        assert_eq!(comparison.workloads.len(), candidate.workloads.len());
    }

    #[test]
    fn incompatible_or_malformed_baselines_cannot_publish_comparisons() {
        for malformed in [false, true] {
            let directory = tempfile::tempdir().expect("report directory");
            let mut baseline = crate::performance::tests::report("other corpus", 100, 1);

            if malformed {
                baseline.workloads[0].artifacts[0]
                    .linker_map
                    .as_mut()
                    .expect("linker map")
                    .sha256 = "invalid".to_owned();
            }

            let baseline_path = directory.path().join("baseline.json");

            crate::json::write_pretty(&baseline_path, &baseline).expect("baseline writes");

            let mut candidate = crate::performance::tests::report("corpus", 100, 1);

            assert!(publish(directory.path(), Some(&baseline_path), &mut candidate).is_err());
            assert!(!directory.path().join("comparison.json").exists());
            assert!(!directory.path().join("comparison.html").exists());
        }
    }

    #[test]
    fn missing_dependency_evidence_is_a_hard_error_even_with_quality_failures() {
        let directory = tempfile::tempdir().expect("report directory");
        let mut candidate = crate::performance::tests::report("corpus", 100, 1);

        candidate
            .conformance_failures
            .push("retained forbidden runtime".to_owned());

        candidate.workloads[0]
            .peers
            .values_mut()
            .next()
            .expect("peer")
            .artifacts[0]
            .dependencies
            .dynamic_libraries
            .omitted_count = 1;

        let error =
            publish(directory.path(), None, &mut candidate).expect_err("incomplete measurement");

        assert!(error.contains("omits dependencies required to prove"));

        assert!(
            directory
                .path()
                .join("candidate-unvalidated.json")
                .is_file()
        );

        assert!(!directory.path().join("candidate.html").exists());
        assert!(!directory.path().join("comparison.json").exists());
    }

    #[test]
    fn failing_baseline_keeps_comparison_unaccepted_and_command_unsuccessful() {
        let directory = tempfile::tempdir().expect("report directory");
        let mut baseline = crate::performance::tests::report("corpus", 100, 1);

        baseline
            .conformance_failures
            .push("retained forbidden runtime".to_owned());

        let path = directory.path().join("baseline.json");

        crate::json::write_pretty(&path, &baseline).expect("baseline writes");

        let mut candidate = crate::performance::tests::report("corpus", 100, 1);

        assert!(
            publish(directory.path(), Some(&path), &mut candidate)
                .expect_err("baseline failure")
                .contains("baseline: retained forbidden runtime")
        );

        assert!(directory.path().join("comparison.html").is_file());
    }

    #[test]
    fn successful_reports_and_comparisons_remain_successful() {
        let directory = tempfile::tempdir().expect("report directory");
        let mut candidate = crate::performance::tests::report("corpus", 100, 1);
        let baseline_path = directory.path().join("baseline.json");

        crate::json::write_pretty(&baseline_path, &candidate).expect("baseline writes");

        publish(directory.path(), Some(&baseline_path), &mut candidate)
            .expect("matching successful reports");

        let html = std::fs::read_to_string(directory.path().join("comparison.html"))
            .expect("comparison HTML");

        assert!(html.contains("Candidate conformance: Passed"));
        assert!(html.contains("Baseline conformance: Passed"));
    }
}
