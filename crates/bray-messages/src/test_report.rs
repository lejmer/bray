use crate::DiagnosticLocale;
use crate::catalog::MessageCatalog;

/// Stable outcome category rendered in a human test report.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TestReportOutcome {
    /// The invocation passed.
    Passed,
    /// The invocation returned an error.
    ReturnedError,
    /// An explicit test failure occurred.
    ExplicitFailure,
    /// An assertion failed.
    AssertionFailure,
    /// The invocation panicked.
    Panicked,
    /// The invocation exceeded its timeout.
    TimedOut,
    /// The invocation was cancelled.
    Cancelled,
    /// Test infrastructure failed.
    InfrastructureFailed,
}

/// Stable terminal state for a complete test command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TestReportSummaryStatus {
    /// Every selected test passed.
    Finished,
    /// At least one selected test did not pass.
    Failed,
}

/// Stable activity state for an invocation that has not completed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TestReportActivity {
    /// The invocation is waiting for admission.
    Waiting,
    /// The invocation is executing.
    Running,
}

/// Stable category of one test-report progress line.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TestReportLineKind {
    /// One selected invocation waiting for admission.
    WaitingResult,
    /// One selected test invocation.
    Result,
    /// The aggregate command result.
    Summary,
}

/// Locale-aware renderer for human test reports.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TestReportMessageRenderer {
    catalog: MessageCatalog,
}

impl TestReportMessageRenderer {
    /// Creates a renderer for the selected locale.
    pub const fn new(locale: DiagnosticLocale) -> Self {
        Self {
            catalog: MessageCatalog::new(locale),
        }
    }

    /// Creates an English renderer.
    pub const fn english() -> Self {
        Self::new(DiagnosticLocale::English)
    }

    /// Renders the command heading.
    pub fn heading(self, count: usize) -> String {
        self.catalog.test_report_heading(count)
    }

    /// Returns the locale-defined field order for one test-report line.
    pub fn fields(self, kind: TestReportLineKind) -> &'static [crate::ProgressField] {
        self.catalog.test_report_fields(kind)
    }

    /// Renders the operation describing an invocation activity state.
    pub fn activity_operation(self, activity: TestReportActivity) -> &'static str {
        self.catalog.test_report_activity_operation(activity)
    }

    /// Renders the operation describing one invocation result.
    pub fn result_operation(self, outcome: TestReportOutcome) -> &'static str {
        self.catalog.test_report_result_operation(outcome)
    }

    /// Renders the operation describing the complete command result.
    pub fn summary_operation(self, status: TestReportSummaryStatus) -> &'static str {
        self.catalog.test_report_summary_operation(status)
    }

    /// Renders aggregate pass, failure, and filtering counts.
    pub fn summary_counts(self, passed: usize, failed: usize, filtered: usize) -> String {
        self.catalog
            .test_report_summary_counts(passed, failed, filtered)
    }

    /// Renders an elapsed duration represented in milliseconds.
    pub fn duration(self, milliseconds: u128) -> String {
        self.catalog.test_report_duration(milliseconds)
    }

    /// Renders a captured standard-stream heading.
    pub fn captured_stream(self, standard_error: bool) -> &'static str {
        self.catalog.test_report_captured_stream(standard_error)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        TestReportMessageRenderer, TestReportOutcome, TestReportSummaryStatus,
    };

    #[test]
    fn test_workflow_text_renders_through_the_selected_catalog() {
        let renderer = TestReportMessageRenderer::english();

        assert_eq!(renderer.heading(1), "Running 1 test");
        assert_eq!(renderer.heading(2), "Running 2 tests");
        assert_eq!(renderer.result_operation(TestReportOutcome::Passed), "Passed");

        assert_eq!(
            renderer.summary_operation(TestReportSummaryStatus::Finished),
            "Finished test run"
        );

        assert_eq!(
            renderer.summary_counts(2, 0, 4),
            "2 passed, 0 failed, 4 filtered out"
        );
    }
}
