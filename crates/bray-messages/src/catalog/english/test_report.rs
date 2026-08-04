use crate::{
    ProgressField, TestReportActivity, TestReportLineKind, TestReportOutcome,
    TestReportSummaryStatus,
};

const WAITING_RESULT_FIELDS: &[ProgressField] = &[
    ProgressField::Operation,
    ProgressField::Subject,
];

const RESULT_FIELDS: &[ProgressField] = &[
    ProgressField::Operation,
    ProgressField::Subject,
    ProgressField::Duration,
];

const SUMMARY_FIELDS: &[ProgressField] = &[
    ProgressField::Operation,
    ProgressField::Detail,
    ProgressField::Duration,
];

pub(crate) const fn fields(kind: TestReportLineKind) -> &'static [ProgressField] {
    match kind {
        TestReportLineKind::WaitingResult => WAITING_RESULT_FIELDS,
        TestReportLineKind::Result => RESULT_FIELDS,
        TestReportLineKind::Summary => SUMMARY_FIELDS,
    }
}

pub(crate) const fn activity_operation(activity: TestReportActivity) -> &'static str {
    match activity {
        TestReportActivity::Waiting => "Waiting",
        TestReportActivity::Running => "Running",
    }
}

pub(crate) fn heading(count: usize) -> String {
    let noun = if count == 1 { "test" } else { "tests" };

    format!("Running {count} {noun}")
}

pub(crate) const fn result_operation(outcome: TestReportOutcome) -> &'static str {
    match outcome {
        TestReportOutcome::Passed => "Passed",
        TestReportOutcome::ReturnedError => "Returned error",
        TestReportOutcome::ExplicitFailure => "Failed",
        TestReportOutcome::AssertionFailure => "Assertion failed",
        TestReportOutcome::Panicked => "Panicked",
        TestReportOutcome::TimedOut => "Timed out",
        TestReportOutcome::Cancelled => "Cancelled",
        TestReportOutcome::InfrastructureFailed => "Infrastructure failed",
    }
}

pub(crate) const fn summary_operation(status: TestReportSummaryStatus) -> &'static str {
    match status {
        TestReportSummaryStatus::Finished => "Finished",
        TestReportSummaryStatus::Failed => "Failed",
    }
}

pub(crate) fn summary_counts(passed: usize, failed: usize, filtered: usize) -> String {
    let mut counts = format!("{passed} passed, {failed} failed");

    if filtered > 0 {
        counts.push_str(&format!(", {filtered} filtered out"));
    }

    counts
}

pub(crate) fn duration(milliseconds: u128) -> String {
    super::build_progress::duration(milliseconds)
}

pub(crate) const fn captured_stream(standard_error: bool) -> &'static str {
    if standard_error {
        "captured stderr"
    } else {
        "captured stdout"
    }
}
