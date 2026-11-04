use crate::{
    ProgressField, TestReportActivity, TestReportLineKind, TestReportOutcome,
    TestReportSummaryStatus,
};

const WAITING_RESULT_FIELDS: &[ProgressField] = &[ProgressField::Operation, ProgressField::Subject];

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

pub(crate) const fn captured_stream(standard_error: bool) -> &'static str {
    if standard_error {
        "captured stderr"
    } else {
        "captured stdout"
    }
}

pub(crate) fn host_exit_code(identity: &str, code: i32) -> String {
    format!("Test {identity}: host exited with code {code}")
}

pub(crate) fn host_signal(identity: &str, signal: i64) -> String {
    format!("Test {identity}: host terminated by signal {signal}")
}

pub(crate) fn worker_failure(
    identity: &str,
    report: &bray_test_protocol::TestPanicReport,
) -> Option<String> {
    let bray_test_protocol::TestPanicCause::WorkerFormationFailure(failure) = report.cause() else {
        return None;
    };

    let stage = match failure.stage {
        1 => "allocation",
        2 => "native thread creation",
        3 => "thread attachment",
        4 => "closed admission",
        stage => panic!("worker formation reports retain a defined stage: {stage}"),
    };

    Some(format!(
        "Test {identity}: worker {stage} failed (category {}, native code {})",
        failure.category, failure.native_code
    ))
}

pub(crate) fn timeout_admission_failure(
    identity: &str,
    failure: bray_test_protocol::TestInfrastructureFailure,
) -> Option<String> {
    use bray_test_protocol::TestInfrastructureFailureKind;

    let description = match failure.kind() {
        TestInfrastructureFailureKind::TimeoutRuntime => "execution services are unavailable",
        TestInfrastructureFailureKind::TimeoutCallbackResources => {
            "cancellation callback storage is unavailable"
        }
        TestInfrastructureFailureKind::TimeoutDeadlineOverflow => {
            "deadline is outside the target clock range"
        }
        TestInfrastructureFailureKind::TimeoutCapacity => "pending timeout capacity is exhausted",
        TestInfrastructureFailureKind::TimeoutStorage => "pending timeout storage is unavailable",
        TestInfrastructureFailureKind::TimeoutIdentityExhausted => {
            "pending timeout identities are exhausted"
        }
        TestInfrastructureFailureKind::TimeoutClosed => "timeout admission is closed",
        TestInfrastructureFailureKind::TimeoutClock(status) => {
            return Some(format!(
                "Test {identity}: timeout clock failed (category {}, native code {})",
                status.category(),
                status.native_code(),
            ));
        }
        TestInfrastructureFailureKind::Protocol
        | TestInfrastructureFailureKind::Host
        | TestInfrastructureFailureKind::Capture
        | TestInfrastructureFailureKind::ResourceExhausted
        | TestInfrastructureFailureKind::MissingOutcome
        | TestInfrastructureFailureKind::Cleanup
        | TestInfrastructureFailureKind::ForcedTermination => return None,
    };

    Some(match failure.detail_code() {
        Some(code) => format!("Test {identity}: {description} (status {code})"),
        None => format!("Test {identity}: {description}"),
    })
}
