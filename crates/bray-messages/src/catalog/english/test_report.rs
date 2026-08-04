use crate::TestReportOutcome;

pub(crate) fn heading(count: usize) -> String {
    format!("Running {count} tests")
}

pub(crate) fn result(outcome: TestReportOutcome, identity: &str) -> String {
    let outcome = match outcome {
        TestReportOutcome::Passed => "passed",
        TestReportOutcome::ReturnedError => "returned an error",
        TestReportOutcome::ExplicitFailure => "failed",
        TestReportOutcome::AssertionFailure => "assertion failed",
        TestReportOutcome::Panicked => "panicked",
        TestReportOutcome::TimedOut => "timed out",
        TestReportOutcome::Cancelled => "cancelled",
        TestReportOutcome::InfrastructureFailed => "infrastructure failed",
    };

    format!("{outcome:>21}  {identity}")
}

pub(crate) fn summary(passed: usize, failed: usize) -> String {
    format!("Test result: {passed} passed, {failed} failed")
}

pub(crate) const fn captured_stream(standard_error: bool) -> &'static str {
    if standard_error {
        "captured stderr"
    } else {
        "captured stdout"
    }
}
