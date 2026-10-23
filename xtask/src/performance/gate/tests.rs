use super::super::model::PerformanceReport;
use super::evaluation::evaluate;
use super::model::{GateReport, Policy, State, Waiver};

const BASE: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const CANDIDATE: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn policy() -> Policy {
    serde_json::from_str(include_str!("../../../performance-policy.json"))
        .expect("repository-owned policy")
}

fn report(revision: &str, median: u64, mad: u64) -> PerformanceReport {
    let mut report = super::super::tests::report("corpus", median, mad);

    report.identity.source_revision = revision.to_owned();
    report.identity.sample_iterations = 7;

    for workload in &mut report.workloads {
        let mut measurements = vec![
            &mut workload.process_execution,
            &mut workload.bray_execution,
        ];

        for peer in workload.peers.values_mut() {
            measurements.extend([&mut peer.process_execution, &mut peer.controlled_execution]);
        }

        for measurement in measurements {
            let samples = vec![
                median - mad,
                median - mad,
                median,
                median,
                median,
                median + mad,
                median + mad,
            ]
            .into_iter()
            .map(|value| value * measurement.inner_iterations)
            .collect();

            *measurement = super::super::statistics::summarize(
                samples,
                workload.scale,
                &measurement.scope,
                measurement.inner_iterations,
                measurement.timer_resolution_nanoseconds,
            )
            .expect("synthetic observations");
        }
    }

    report
}

fn gate(
    baseline: &PerformanceReport,
    candidate: &PerformanceReport,
    confirmation: Option<(&PerformanceReport, &PerformanceReport)>,
) -> GateReport {
    evaluate(
        baseline,
        candidate,
        confirmation,
        policy(),
        BASE,
        CANDIDATE,
        None,
    )
    .expect("valid matched gate")
}

#[test]
fn equal_or_improving_measurements_pass() {
    let baseline = report(BASE, 10_000_000, 10);
    let candidate = report(CANDIDATE, 9_000_000, 10);
    let gate = gate(&baseline, &candidate, None);

    assert_eq!(gate.state, State::Pass);
    assert!(!gate.blocking);
}

#[test]
fn two_percent_miss_needs_review_and_never_blocks_initial_evidence() {
    let baseline = report(BASE, 10_000_000, 10);
    let candidate = report(CANDIDATE, 10_200_000, 10);
    let gate = gate(&baseline, &candidate, None);

    assert_eq!(gate.state, State::ReviewRequired);
    assert!(!gate.blocking);

    assert!(
        gate.decisions
            .iter()
            .filter(|decision| decision.timing.is_some())
            .all(|decision| decision.state == State::ReviewRequired)
    );
}

#[test]
fn independent_confirmed_workload_regression_blocks() {
    let baseline = report(BASE, 10_000_000, 10);
    let candidate = report(CANDIDATE, 12_000_000, 10);
    let mut confirm_base = baseline.clone();
    let mut confirm_candidate = candidate.clone();

    confirm_base.identity.measured_at_unix_nanoseconds = 2;
    confirm_candidate.identity.measured_at_unix_nanoseconds = 3;

    let gate = gate(
        &baseline,
        &candidate,
        Some((&confirm_base, &confirm_candidate)),
    );

    assert_eq!(gate.state, State::ConfirmedRegression);
    assert!(gate.blocking);

    assert!(
        gate.decisions
            .iter()
            .any(|decision| decision.workload == "small_output"
                && decision.metric == "controlled_execution.picoseconds"
                && decision.state == State::ConfirmedRegression)
    );
}

#[test]
fn noise_unresolved_intervals_and_insufficient_samples_cannot_confirm_a_regression() {
    for (median, mad, minimum_samples, resolution_multiple) in [
        (10_200_000, 1_000_000, 7, 10_000),
        (12_000_000, 10, 8, 10_000),
        (12_000_000, 10, 7, u64::MAX),
    ] {
        let baseline = report(BASE, 10_000_000, mad);
        let candidate = report(CANDIDATE, median, mad);
        let mut confirm_base = baseline.clone();
        let mut confirm_candidate = candidate.clone();
        let mut policy = policy();

        confirm_base.identity.measured_at_unix_nanoseconds = 2;
        confirm_candidate.identity.measured_at_unix_nanoseconds = 3;
        policy.minimum_samples = minimum_samples;
        policy.resolution_multiple = resolution_multiple;

        let gate = evaluate(
            &baseline,
            &candidate,
            Some((&confirm_base, &confirm_candidate)),
            policy,
            BASE,
            CANDIDATE,
            None,
        )
        .expect("valid uncertain evidence");

        assert_eq!(gate.state, State::ReviewRequired);
        assert!(!gate.blocking);
    }
}

#[test]
fn deterministic_structure_blocks_without_timing_confirmation() {
    let baseline = report(BASE, 10_000_000, 10);
    let mut candidate = report(CANDIDATE, 9_000_000, 10);

    candidate.workloads[0].artifacts[0].bytes += 1;

    let gate = gate(&baseline, &candidate, None);

    assert!(gate.blocking);
    assert_eq!(gate.state, State::ConfirmedRegression);
    assert_eq!(gate.decisions[0].metric, "executable.bytes");
}

#[test]
fn stale_merge_bases_modified_candidates_and_duplicate_confirmations_fail() {
    let baseline = report(BASE, 10_000_000, 10);
    let candidate = report(CANDIDATE, 12_000_000, 10);

    for (base, revision) in [(CANDIDATE, CANDIDATE), (BASE, BASE), (BASE, "modified")] {
        assert!(evaluate(&baseline, &candidate, None, policy(), base, revision, None).is_err());
    }

    assert!(
        evaluate(
            &baseline,
            &candidate,
            Some((&baseline, &candidate)),
            policy(),
            BASE,
            CANDIDATE,
            None
        )
        .is_err()
    );
}

#[test]
fn mismatched_environments_and_incomplete_reports_fail() {
    let baseline = report(BASE, 10_000_000, 10);

    for field in [
        "host_class",
        "worker_budget",
        "rustc_version",
        "runtime_sha256",
        "standard_library_sha256",
        "corpus_sha256",
        "llvm_version",
    ] {
        let candidate = report(CANDIDATE, 10_000_000, 10);
        let mut value = serde_json::to_value(candidate).expect("fixture JSON");

        value["identity"][field] = if field == "worker_budget" {
            serde_json::json!(2)
        } else {
            serde_json::json!("different")
        };

        let candidate = serde_json::from_value(value).expect("modified identity");

        assert!(
            evaluate(&baseline, &candidate, None, policy(), BASE, CANDIDATE, None).is_err(),
            "{field}"
        );
    }

    let mut candidate = report(CANDIDATE, 10_000_000, 10);

    candidate.identity.host_class = None;

    assert!(evaluate(&baseline, &candidate, None, policy(), BASE, CANDIDATE, None).is_err());
}

fn waiver() -> Waiver {
    Waiver {
        workload: "small_output".to_owned(),
        metric: "executable.bytes".to_owned(),
        target: "x86_64-pc-windows-msvc".to_owned(),
        maximum_value: 110,
        maximum_increase: 10,
        rationale: "retain a measured runtime capability".to_owned(),
        owner: "owner".to_owned(),
        issue: "https://linear.app/bray-lang/issue/BRA-456".to_owned(),
        approved_by: "reviewer".to_owned(),
        approval: "https://github.com/lejmer/bray/pull/1#issuecomment-1".to_owned(),
        approved_on: "2026-10-20".to_owned(),
        expires_on: "2026-11-01".to_owned(),
    }
}

fn waived_report() -> GateReport {
    let baseline = report(BASE, 10_000_000, 10);
    let mut candidate = report(CANDIDATE, 9_000_000, 10);

    candidate.workloads[0].artifacts[0].bytes = 110;

    let mut gate = gate(&baseline, &candidate, None);

    gate.policy.waivers.push(waiver());

    gate
}

#[test]
fn exact_bounded_active_waivers_preserve_failed_state_and_show_approval() {
    let mut gate = waived_report();
    let today = super::waivers::date("2026-10-23").expect("today");

    super::waivers::apply(&mut gate, today, |_| Ok(true)).expect("approved waiver");

    assert!(!gate.blocking);
    assert_eq!(gate.state, State::ConfirmedRegression);
    assert_eq!(gate.active_waivers.len(), 1);
    assert!(gate.decisions[0].waiver.is_some());

    let html = super::presentation::render(&gate).expect("gate HTML");

    assert!(html.contains("confirmed-regression"));
    assert!(html.contains("BRA-456"));
    assert!(html.contains("2026-11-01"));
    assert!(html.contains("owner owner"));
    assert!(html.contains("BRA-540"));
}

#[test]
fn expired_closed_missing_broadened_and_over_bound_waivers_fail() {
    let today = super::waivers::date("2026-10-23").expect("today");

    for invalid in [
        "expired",
        "closed",
        "issue-less",
        "broad",
        "bound",
        "approval",
        "too-long",
        "malformed-approval",
    ] {
        let mut gate = waived_report();
        let waiver = &mut gate.policy.waivers[0];

        match invalid {
            "expired" => waiver.expires_on = "2026-10-23".to_owned(),
            "issue-less" => waiver.issue.clear(),
            "broad" => waiver.metric = "*".to_owned(),
            "bound" => waiver.maximum_value = 109,
            "approval" => waiver.approval.clear(),
            "malformed-approval" => {
                waiver.approval = "https://github.com/lejmer/bray/pull/invalid".to_owned()
            }
            "too-long" => waiver.expires_on = "2026-12-31".to_owned(),
            _ => {}
        }

        assert!(
            super::waivers::apply(&mut gate, today, |_| Ok(invalid != "closed")).is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn waivers_never_override_conformance_failures_or_other_metrics() {
    let today = super::waivers::date("2026-10-23").expect("today");
    let mut gate = waived_report();

    gate.comparison
        .candidate_conformance_failures
        .push("absolute retention failure".to_owned());

    super::waivers::apply(&mut gate, today, |_| Ok(true)).expect("valid scoped waiver");

    assert!(gate.blocking);

    let mut gate = waived_report();

    gate.policy.waivers[0].metric = "controlled_execution.picoseconds".to_owned();
    gate.policy.waivers[0].maximum_value = u64::MAX;
    gate.policy.waivers[0].maximum_increase = u64::MAX;

    super::waivers::apply(&mut gate, today, |_| Ok(true)).expect("waiver for a different metric");

    assert!(gate.blocking);
    assert!(gate.decisions[0].waiver.is_none());
}

#[test]
fn policy_scope_and_strict_waiver_schema_reject_silent_extensions() {
    let mut policy = policy();

    policy
        .structural
        .insert("invented_workload".to_owned(), Default::default());

    assert!(super::evaluation::validate_policy(&policy).is_err());

    let mut json = serde_json::to_value(waiver()).expect("waiver JSON");

    json["all_metrics"] = serde_json::json!(true);

    assert!(serde_json::from_value::<Waiver>(json).is_err());
}

#[test]
fn timing_metrics_cannot_bypass_confirmation_as_structural_rules() {
    let mut baseline = report(BASE, 10_000_000, 10);
    let mut candidate = report(CANDIDATE, 12_000_000, 10);
    let mut policy = policy();

    baseline.workloads[0].compiler_profile.descriptors.metrics[0].unit =
        bray_compilation::CompilationProfileUnit::Nanoseconds;

    candidate.workloads[0].compiler_profile.descriptors.metrics[0].unit =
        bray_compilation::CompilationProfileUnit::Nanoseconds;

    policy
        .structural
        .get_mut("small_output")
        .expect("workload policy")
        .insert(
            "compiler.metric.source_units".to_owned(),
            super::model::StructuralLimit {
                maximum_increase: 0,
                maximum_value: None,
            },
        );

    assert!(
        evaluate(&baseline, &candidate, None, policy, BASE, CANDIDATE, None)
            .expect_err("timing is not a structural count")
            .contains("no comparable measurement")
    );
}

#[test]
fn calendar_dates_and_command_arguments_are_validated() {
    assert_eq!(super::waivers::date("1970-01-01").expect("epoch"), 0);
    assert!(super::waivers::date("2024-02-29").is_ok());
    assert!(super::waivers::date("2026-02-29").is_err());
    assert!(super::waivers::date("2026-13-01").is_err());

    for arguments in [
        vec!["--candidate"],
        vec!["--candidate", "--baseline"],
        vec!["--baseline", "a", "--baseline", "b"],
        vec!["--unknown", "a"],
    ] {
        assert!(super::command::parse(arguments.into_iter().map(str::to_owned)).is_err());
    }
}

#[test]
fn live_issue_responses_fail_closed_on_terminal_missing_and_invalid_states() {
    for (state, expected) in [
        ("backlog", true),
        ("started", true),
        ("completed", false),
        ("canceled", false),
        ("unknown", false),
    ] {
        let response = serde_json::json!({"data": {"issue": {"identifier": "BRA-456", "archivedAt": null, "state": {"type": state}}}});

        assert_eq!(
            super::waivers::issue_response_is_open(
                "BRA-456",
                &serde_json::to_vec(&response).expect("issue JSON")
            )
            .expect("valid response"),
            expected
        );
    }

    for response in [
        serde_json::json!({"data": {"issue": {"identifier": "BRA-456", "state": {"type": "started"}}}}),
        serde_json::json!({"data": {"issue": null}}),
        serde_json::json!({"errors": [{"message": "unauthorized"}]}),
        serde_json::json!({"data": {}}),
    ] {
        assert!(
            super::waivers::issue_response_is_open(
                "BRA-456",
                &serde_json::to_vec(&response).expect("failed issue JSON")
            )
            .is_err()
        );
    }

    let archived = serde_json::json!({"data": {"issue": {"identifier": "BRA-456", "archivedAt": "2026-10-23", "state": {"type": "started"}}}});

    assert!(
        !super::waivers::issue_response_is_open(
            "BRA-456",
            &serde_json::to_vec(&archived).expect("archived issue")
        )
        .expect("archive status")
    );
}

#[test]
fn gate_command_uses_the_real_merge_base_and_publishes_failed_decisions() {
    let directory = tempfile::tempdir().expect("gate repository");
    let root = directory.path();

    let git =
        |arguments: &[&str]| super::command::git(root, arguments).expect("fixture Git command");

    git(&["init", "--initial-branch=main"]);
    std::fs::create_dir(root.join("xtask")).expect("policy directory");
    std::fs::create_dir(root.join("reports")).expect("report directory");
    std::fs::write(root.join(".gitignore"), "/reports/\n").expect("ignore measurements");

    std::fs::write(
        root.join("xtask/performance-policy.json"),
        include_str!("../../../performance-policy.json"),
    )
    .expect("policy");

    git(&["add", "."]);

    git(&[
        "-c",
        "user.name=Gate Test",
        "-c",
        "user.email=gate@example.invalid",
        "commit",
        "-m",
        "base",
    ]);

    let base = git(&["rev-parse", "HEAD"]);

    git(&["checkout", "-b", "candidate"]);

    git(&[
        "-c",
        "user.name=Gate Test",
        "-c",
        "user.email=gate@example.invalid",
        "commit",
        "--allow-empty",
        "-m",
        "candidate",
    ]);

    let revision = git(&["rev-parse", "HEAD"]);
    let baseline = report(&base, 10_000_000, 10);
    let mut candidate = report(&revision, 9_000_000, 10);
    let baseline_path = root.join("reports/base.json");
    let candidate_path = root.join("reports/candidate.json");
    let output = root.join("reports/gate");

    candidate.workloads[0].artifacts[0].bytes += 1;

    crate::json::write_pretty(&baseline_path, &baseline).expect("baseline");
    crate::json::write_pretty(&candidate_path, &candidate).expect("candidate");

    let arguments = [
        "--base-ref".to_owned(),
        "main".to_owned(),
        "--baseline".to_owned(),
        baseline_path.display().to_string(),
        "--candidate".to_owned(),
        candidate_path.display().to_string(),
        "--output".to_owned(),
        output.display().to_string(),
    ];

    let failure =
        super::command::execute(root, arguments.into_iter()).expect_err("structural gate failure");

    assert!(failure.contains("regression gate failed"));

    let retained: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output.join("gate.json")).expect("failed report"))
            .expect("gate JSON");

    assert_eq!(retained["merge_base"], base);
    assert_eq!(retained["candidate_revision"], revision);
    assert_eq!(retained["blocking"], true);
    assert_eq!(retained["state"], "confirmed_regression");

    for name in [
        "gate.html",
        "comparison.json",
        "comparison.html",
        "baseline.json",
        "candidate.json",
        "baseline.html",
        "candidate.html",
    ] {
        assert!(output.join(name).is_file(), "{name}");
    }
}

#[test]
fn workload_regression_is_not_hidden_by_faster_compiler_or_peer_aggregates() {
    let mut baseline = report(BASE, 10_000_000, 10);
    let mut candidate = report(CANDIDATE, 12_000_000, 10);

    append_borrowed_text(&mut baseline, 100_000_000);
    append_borrowed_text(&mut candidate, 1_000_000);

    assert!(
        candidate
            .workloads
            .iter()
            .map(|row| row.bray_execution.median_picoseconds)
            .sum::<u64>()
            < baseline
                .workloads
                .iter()
                .map(|row| row.bray_execution.median_picoseconds)
                .sum::<u64>()
    );

    candidate.workloads[0]
        .compilation
        .process_elapsed_nanoseconds = 1;

    candidate.workloads[0]
        .compilation
        .compiler_elapsed_nanoseconds = 1;

    candidate.workloads[0]
        .compilation
        .compiler_components_nanoseconds
        .values_mut()
        .for_each(|value| *value = 1);

    let mut confirm_base = baseline.clone();
    let mut confirm_candidate = candidate.clone();

    confirm_base.identity.measured_at_unix_nanoseconds = 2;
    confirm_candidate.identity.measured_at_unix_nanoseconds = 3;

    let gate = gate(
        &baseline,
        &candidate,
        Some((&confirm_base, &confirm_candidate)),
    );

    assert!(gate.blocking);

    assert!(
        gate.decisions
            .iter()
            .any(|decision| decision.metric == "compiler_work.nanoseconds"
                && decision.state == State::Pass)
    );

    assert!(gate.decisions.iter().any(|decision| decision.metric
        == "controlled_execution.picoseconds"
        && decision.state == State::ConfirmedRegression));
}

fn append_borrowed_text(report: &mut PerformanceReport, median: u64) {
    let canonical = super::super::corpus::WORKLOADS
        .iter()
        .find(|workload| workload.id == "borrowed_text")
        .expect("existing complete consumer");

    let mut row = report.workloads[0].clone();

    row.id = canonical.id.to_owned();
    row.category = canonical.category;
    row.scale = canonical.scale;
    row.units = canonical.units.to_owned();

    row.expected_output_sha256 =
        super::super::command::expected_output_digest(canonical.expected_output)
            .expect("corpus output");

    row.peer_contract = super::super::peer::comparison_contract(canonical.id)
        .expect("peer contract")
        .to_owned();

    row.batching = super::super::model::WorkloadBatching::SingleExecution;

    for (language, peer) in &mut row.peers {
        peer.build_configuration = super::super::peer::fixture_build_configuration(
            *language,
            canonical.id,
            bray_target::NativeTarget::X86_64WindowsMsvc,
            std::path::Path::new(&peer.artifacts[0].path),
            std::num::NonZeroU64::MIN,
        );
    }

    let mut measurements = vec![&mut row.process_execution, &mut row.bray_execution];

    for peer in row.peers.values_mut() {
        measurements.extend([&mut peer.process_execution, &mut peer.controlled_execution]);
    }

    for measurement in measurements {
        *measurement = super::super::statistics::summarize(
            vec![median; 7],
            row.scale,
            &measurement.scope,
            1,
            report.identity.timer_resolution_nanoseconds,
        )
        .expect("second-workload observations");
    }

    report.workloads.push(row);
}

#[test]
fn checkpoint_requires_measured_complete_consumer_and_attributed_bounded_experiment() {
    let baseline = report(BASE, 10_000_000, 10);
    let candidate = report(CANDIDATE, 9_000_000, 10);

    let checkpoint = serde_json::json!({
        "workload": "small_output", "caller_and_callee": "application and library entry",
        "cleanup_and_report_release": "terminal cleanup consumes the report", "ownership_or_failure_case": "denied allocation",
        "correctness_evidence": "retained native ownership evidence", "hypothesis": "eliminate redundant report work",
        "expected_improvement": "fewer generated operations", "stopping_condition": "one complete consumer measurement",
        "attribution": "whole_change_only", "attribution_evidence": "retain compiler operations and linked sections"
    });

    for invalid in [false, true] {
        let mut value = checkpoint.clone();

        if invalid {
            value["workload"] = serde_json::json!("unused_abi_foundation");
        }

        let result = evaluate(
            &baseline,
            &candidate,
            None,
            policy(),
            BASE,
            CANDIDATE,
            Some(serde_json::from_value(value).expect("checkpoint schema")),
        );

        assert_eq!(result.is_err(), invalid);
    }
}
