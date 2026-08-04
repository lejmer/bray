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

    /// Renders one canonical invocation result.
    pub fn result(self, outcome: TestReportOutcome, identity: &str) -> String {
        self.catalog.test_report_result(outcome, identity)
    }

    /// Renders aggregate pass and failure counts.
    pub fn summary(self, passed: usize, failed: usize) -> String {
        self.catalog.test_report_summary(passed, failed)
    }

    /// Renders a captured standard-stream heading.
    pub fn captured_stream(self, standard_error: bool) -> &'static str {
        self.catalog.test_report_captured_stream(standard_error)
    }
}
