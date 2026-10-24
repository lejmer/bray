use std::fmt::Write as _;
use std::path::Path;

use super::super::html::{document_start, escape, finish};
use super::model::{GateReport, State};

pub(super) fn write(path: &Path, report: &GateReport) -> Result<(), String> {
    super::super::html::write(path, render(report)?)
}

pub(super) fn render(report: &GateReport) -> Result<String, String> {
    let mut html = document_start("Merge-base performance regression gate");

    let _ = write!(
        html,
        "<p>State: <strong>{}</strong>. Merge gate: <strong>{}</strong>.</p><p>Merge base: <code>{}</code>. Candidate: <code>{}</code>.</p>",
        state(report.state),
        if report.blocking {
            "FAILED"
        } else {
            "no unwaived confirmed regression"
        },
        escape(&report.merge_base),
        escape(&report.candidate_revision)
    );

    html.push_str("<p>Review-required timings need human assessment. A single timing miss cannot block automatically. Waivers preserve the observed regression state.</p><section><h2>Absolute health and competitive targets</h2><p>This gate assesses merge-base deltas only. It does not grant acceptance for BRA-540, BRA-541, BRA-542, or BRA-428. Keep their matched peer comparisons and resource evidence visible in the retained candidate report. A regression waiver cannot turn a failed absolute gate into a pass.</p><p><a href=\"candidate.html\">Absolute candidate and peer measurements</a>. <a href=\"baseline.html\">Absolute merge-base measurements</a>. <a href=\"comparison.html\">Cost attribution and peer deltas</a>.</p></section>");

    super::super::presentation::conformance(
        &mut html,
        "Baseline",
        &report.comparison.baseline_conformance_failures,
    );

    super::super::presentation::conformance(
        &mut html,
        "Candidate",
        &report.comparison.candidate_conformance_failures,
    );

    html.push_str("<section><h2>Workload decisions</h2><table><thead><tr><th>Workload</th><th>Metric</th><th>Baseline</th><th>Candidate</th><th>State</th><th>Evidence</th><th>Waiver</th></tr></thead><tbody>");

    for decision in &report.decisions {
        let waiver = decision.waiver.as_ref().map_or_else(
            || "None".to_owned(),
            |waiver| format!("{} until {}", waiver.issue, waiver.expires_on),
        );

        let _ = write!(
            html,
            "<tr><th>{}</th><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}<br>Initial: {:?}<br>Confirmation: {:?}</td><td>{}</td></tr>",
            escape(&decision.workload),
            escape(&decision.metric),
            decision.baseline,
            decision.candidate,
            state(decision.state),
            escape(&decision.evidence),
            decision.timing,
            decision.confirmation,
            escape(&waiver)
        );
    }

    html.push_str("</tbody></table></section><section><h2>Active waivers</h2><ul>");

    for waiver in &report.active_waivers {
        let _ = write!(
            html,
            "<li>{} / {} / {}: maximum value {}, maximum increase {}, owner {}, rationale {}, approved by {} on {} at {}, tracking {}, expires {}.</li>",
            escape(&waiver.workload),
            escape(&waiver.metric),
            escape(&waiver.target),
            waiver.maximum_value,
            waiver.maximum_increase,
            escape(&waiver.owner),
            escape(&waiver.rationale),
            escape(&waiver.approved_by),
            escape(&waiver.approved_on),
            escape(&waiver.approval),
            escape(&waiver.issue),
            escape(&waiver.expires_on)
        );
    }

    html.push_str("</ul></section>");

    if let Some(confirmation) = &report.confirmation_identity {
        super::super::presentation::identity(
            &mut html,
            "Confirmation baseline",
            &confirmation.baseline,
        );

        super::super::presentation::identity(
            &mut html,
            "Confirmation candidate",
            &confirmation.candidate,
        );
    }

    if let Some(checkpoint) = &report.checkpoint {
        let json = serde_json::to_string_pretty(checkpoint)
            .map_err(|error| format!("could not render consumer checkpoint: {error}"))?;

        let _ = write!(
            html,
            "<section><h2>Early complete-consumer checkpoint</h2><pre>{}</pre></section>",
            escape(&json)
        );
    }

    finish(html)
}

const fn state(state: State) -> &'static str {
    match state {
        State::Pass => "pass",
        State::ReviewRequired => "review-required",
        State::ConfirmedRegression => "confirmed-regression",
    }
}
