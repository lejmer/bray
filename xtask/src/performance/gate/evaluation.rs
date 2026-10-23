use super::super::comparison::{compare, validate_identity};
use super::super::model::PerformanceReport;
use super::metrics::{single_timings, structural, timing};
use super::model::{
    Checkpoint, ConfirmationIdentity, Decision, GateReport, Policy, State, TimingEvidence,
};

#[expect(
    clippy::too_many_arguments,
    reason = "gate evaluation binds measured evidence to verified Git revisions and repository policy"
)]
pub(super) fn evaluate(
    baseline: &PerformanceReport,
    candidate: &PerformanceReport,
    confirmation: Option<(&PerformanceReport, &PerformanceReport)>,
    policy: Policy,
    merge_base: &str,
    candidate_revision: &str,
    checkpoint: Option<Checkpoint>,
) -> Result<GateReport, String> {
    validate_revisions(baseline, candidate, merge_base, candidate_revision)?;

    let comparison = compare(baseline, candidate)?;

    if let Some((confirm_base, confirm_candidate)) = confirmation {
        validate_revisions(
            confirm_base,
            confirm_candidate,
            merge_base,
            candidate_revision,
        )?;

        compare(confirm_base, confirm_candidate)?;
        validate_identity(baseline, confirm_base)?;
        validate_identity(candidate, confirm_candidate)?;

        let initial_workloads = baseline
            .workloads
            .iter()
            .map(|workload| workload.id.as_str())
            .collect::<std::collections::BTreeSet<_>>();

        let confirmation_workloads = confirm_base
            .workloads
            .iter()
            .map(|workload| workload.id.as_str())
            .collect::<std::collections::BTreeSet<_>>();

        if initial_workloads != confirmation_workloads {
            return Err(
                "confirmation must measure the same workload set as the initial pair".to_owned(),
            );
        }

        if baseline.identity.measured_at_unix_nanoseconds
            == confirm_base.identity.measured_at_unix_nanoseconds
            || candidate.identity.measured_at_unix_nanoseconds
                == confirm_candidate.identity.measured_at_unix_nanoseconds
        {
            return Err(
                "confirmation must use independently measured baseline and candidate reports"
                    .to_owned(),
            );
        }
    }

    validate_policy(&policy)?;

    if let Some(checkpoint) = &checkpoint {
        validate_checkpoint(checkpoint, candidate)?;
    }

    let mut decisions = Vec::new();

    for row in &comparison.workloads {
        let baseline_row = baseline
            .workloads
            .iter()
            .find(|workload| workload.id == row.id)
            .expect("comparison establishes baseline workload identity");

        let candidate_row = candidate
            .workloads
            .iter()
            .find(|workload| workload.id == row.id)
            .expect("comparison establishes candidate workload identity");

        let values = structural(row, baseline_row, candidate_row);

        if let Some(limits) = policy.structural.get(&row.id) {
            for (metric, limit) in limits {
                let (baseline, candidate) = values.get(metric).copied().ok_or_else(|| {
                    format!(
                        "configured structural metric {} / {metric} has no comparable measurement",
                        row.id
                    )
                })?;

                let exceeded = candidate.saturating_sub(baseline) > limit.maximum_increase
                    || limit
                        .maximum_value
                        .is_some_and(|maximum| candidate > maximum);

                decisions.push(Decision {
                    workload: row.id.clone(),
                    metric: metric.clone(),
                    baseline,
                    candidate,
                    state: if exceeded {
                        State::ConfirmedRegression
                    } else {
                        State::Pass
                    },
                    evidence: format!(
                        "deterministic structural limit: maximum increase {}, maximum value {:?}",
                        limit.maximum_increase, limit.maximum_value
                    ),
                    confirmation: None,
                    timing: None,
                    waiver: None,
                });
            }
        }

        let confirmation_rows = confirmation.map(|(baseline, candidate)| {
            (
                baseline
                    .workloads
                    .iter()
                    .find(|workload| workload.id == row.id)
                    .expect("matched confirmation baseline workload"),
                candidate
                    .workloads
                    .iter()
                    .find(|workload| workload.id == row.id)
                    .expect("matched confirmation candidate workload"),
            )
        });

        for (metric, baseline, candidate, confirmation) in [
            (
                "controlled_execution.picoseconds",
                &baseline_row.bray_execution,
                &candidate_row.bray_execution,
                confirmation_rows.map(|(baseline, candidate)| {
                    (&baseline.bray_execution, &candidate.bray_execution)
                }),
            ),
            (
                "process_execution.picoseconds",
                &baseline_row.process_execution,
                &candidate_row.process_execution,
                confirmation_rows.map(|(baseline, candidate)| {
                    (&baseline.process_execution, &candidate.process_execution)
                }),
            ),
        ] {
            let observed = timing(baseline, candidate, &policy);

            let confirmation =
                confirmation.map(|(baseline, candidate)| timing(baseline, candidate, &policy));

            decisions.push(timing_decision(&row.id, metric, observed, confirmation));
        }
    }

    let baseline_timings = single_timings(baseline);

    for ((workload, metric), candidate) in single_timings(candidate) {
        let baseline = baseline_timings[&(workload.clone(), metric.clone())];

        decisions.push(Decision {
            workload, metric, baseline, candidate,
            state: if candidate > baseline { State::ReviewRequired } else { State::Pass },
            evidence: "single compiler observation, repeated execution samples do not confirm compiler timing".to_owned(),
            confirmation: None, timing: None, waiver: None,
        });
    }

    let state = if decisions
        .iter()
        .any(|decision| decision.state == State::ConfirmedRegression)
    {
        State::ConfirmedRegression
    } else if decisions
        .iter()
        .any(|decision| decision.state == State::ReviewRequired)
    {
        State::ReviewRequired
    } else {
        State::Pass
    };

    let blocking = decisions
        .iter()
        .any(|decision| decision.state == State::ConfirmedRegression)
        || !comparison.baseline_conformance_failures.is_empty()
        || !comparison.candidate_conformance_failures.is_empty();

    Ok(GateReport {
        merge_base: merge_base.to_owned(),
        candidate_revision: candidate_revision.to_owned(),
        baseline_identity: baseline.identity.clone(),
        candidate_identity: candidate.identity.clone(),
        confirmation_identity: confirmation.map(|(baseline, candidate)| ConfirmationIdentity {
            baseline: baseline.identity.clone(),
            candidate: candidate.identity.clone(),
        }),
        state,
        blocking,
        policy,
        checkpoint,
        decisions,
        active_waivers: Vec::new(),
        comparison,
    })
}

pub(super) fn validate_revisions(
    baseline: &PerformanceReport,
    candidate: &PerformanceReport,
    merge_base: &str,
    candidate_revision: &str,
) -> Result<(), String> {
    if baseline.identity.source_revision != merge_base
        || candidate.identity.source_revision != candidate_revision
    {
        return Err(
            "gate reports must identify the exact clean merge base and candidate commits"
                .to_owned(),
        );
    }

    if baseline.identity.host_class.is_none() || candidate.identity.host_class.is_none() {
        return Err("regression gating requires reports measured with --host-class".to_owned());
    }

    Ok(())
}

pub(super) fn validate_policy(policy: &Policy) -> Result<(), String> {
    if policy.minimum_samples < 3
        || policy.minimum_samples > super::super::model::MAX_SAMPLE_COUNT as usize
        || policy.mad_multiplier < 3
        || policy.resolution_multiple < 1_000
        || policy.maximum_waiver_days == 0
    {
        return Err(
            "performance policy has invalid sampling, resolution, or waiver bounds".to_owned(),
        );
    }

    for (workload, limits) in &policy.structural {
        if !super::super::corpus::WORKLOADS
            .iter()
            .any(|entry| entry.id == workload)
            || limits.is_empty()
        {
            return Err(format!(
                "performance policy has unknown or empty workload {workload}"
            ));
        }
    }

    Ok(())
}

fn timing_decision(
    workload: &str,
    metric: &str,
    observed: TimingEvidence,
    confirmation: Option<TimingEvidence>,
) -> Decision {
    let state = if observed.candidate <= observed.baseline && observed.resolved {
        State::Pass
    } else if confident_regression(observed) && confirmation.is_some_and(confident_regression) {
        State::ConfirmedRegression
    } else {
        State::ReviewRequired
    };

    Decision {
        workload: workload.to_owned(), metric: metric.to_owned(), baseline: observed.baseline,
        candidate: observed.candidate, state,
        evidence: "median and MAD with sample-count and timer-resolution checks, independent confirmation required for a timing block".to_owned(),
        timing: Some(observed), confirmation, waiver: None,
    }
}

fn confident_regression(evidence: TimingEvidence) -> bool {
    evidence.resolved
        && evidence.candidate.saturating_sub(evidence.baseline)
            > evidence.noise_bound.max(evidence.resolution_bound)
}

fn validate_checkpoint(
    checkpoint: &Checkpoint,
    candidate: &PerformanceReport,
) -> Result<(), String> {
    if !candidate
        .workloads
        .iter()
        .any(|workload| workload.id == checkpoint.workload)
        || [
            &checkpoint.caller_and_callee,
            &checkpoint.cleanup_and_report_release,
            &checkpoint.ownership_or_failure_case,
            &checkpoint.correctness_evidence,
            &checkpoint.hypothesis,
            &checkpoint.expected_improvement,
            &checkpoint.stopping_condition,
            &checkpoint.attribution_evidence,
        ]
        .iter()
        .any(|text| text.trim().is_empty())
    {
        return Err("consumer checkpoint must identify a measured complete consumer, ownership/failure evidence, and a bounded attributed experiment".to_owned());
    }

    Ok(())
}
