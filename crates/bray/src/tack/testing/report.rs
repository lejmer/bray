use std::collections::BTreeMap;

use bray_base::lowercase_hex;
use bray_diagnostics::{
    DiagnosticBag, DiagnosticDocumentParseKind, DiagnosticProjectCommandFailure,
    DiagnosticProjectOperation,
};
use bray_messages::{
    ProgressField, TestReportActivity, TestReportLineKind, TestReportMessageRenderer,
    TestReportOutcome, TestReportSummaryStatus,
};
use bray_test_protocol::{
    CapturedStream, CapturedStreamPolicy, TestCancellationSource, TestCommandReport,
    TestInfrastructureFailureKind, TestInvocationResult, TestOutcome, TestPanicCause,
    TestProductReport, TestSourceAnchor,
};
use bray_tooling::OutputFormat;
use serde::Serialize;

use super::model::LoadedTestHost;
use crate::tack::error::operation_diagnostics;
use crate::tack::progress::{ProgressVisualState, max_column_width, padded, render_plain_line};

use super::identity::identity_text;

pub(super) fn product_reports(
    hosts: &[LoadedTestHost],
    results: Vec<TestInvocationResult>,
) -> Vec<TestProductReport> {
    let mut results = results
        .into_iter()
        .map(|result| (result.identity().product().clone(), result))
        .fold(
            BTreeMap::<_, Vec<_>>::new(),
            |mut results, (product, result)| {
                results.entry(product).or_default().push(result);

                results
            },
        );

    hosts
        .iter()
        .map(|host| {
            TestProductReport::new(
                host.catalog.product().clone(),
                host.digest,
                results.remove(host.catalog.product()).unwrap_or_default(),
            )
        })
        .collect()
}

pub(super) fn render_report(
    report: &TestCommandReport,
    output_format: OutputFormat,
    show_output: bool,
    interactive: bool,
) -> Result<String, DiagnosticBag> {
    match output_format {
        OutputFormat::Text => Ok(render_text_report(report, show_output, interactive)),
        OutputFormat::Json => serialize_json_report(&JsonTestCommandReport::from(report)),
    }
}

pub(super) fn render_batch_report(
    reports: &[(String, TestCommandReport)],
) -> Result<String, DiagnosticBag> {
    serialize_json_report(&JsonTestBatchReport::from(reports))
}

fn serialize_json_report(report: &impl Serialize) -> Result<String, DiagnosticBag> {
    serde_json::to_string_pretty(report)
        .map(|report| format!("{report}\n"))
        .map_err(|_| {
            operation_diagnostics(DiagnosticProjectCommandFailure::Document {
                operation: DiagnosticProjectOperation::TestReportJson,
                path: None,
                problem: DiagnosticDocumentParseKind::Serialization,
            })
        })
}

fn render_text_report(report: &TestCommandReport, show_output: bool, interactive: bool) -> String {
    if interactive {
        return String::new();
    }

    let renderer = TestReportMessageRenderer::english();

    let results = report
        .products()
        .iter()
        .flat_map(TestProductReport::results)
        .collect::<Vec<_>>();

    let mut output = String::new();

    let (operation_width, subject_width) =
        append_result_rows(&mut output, &renderer, report, &results);

    output.push_str(&render_details(report, show_output));

    append_summary(
        &mut output,
        &renderer,
        report,
        operation_width,
        subject_width,
    );

    output
}

pub(super) fn render_details(report: &TestCommandReport, show_output: bool) -> String {
    let renderer = TestReportMessageRenderer::english();
    let mut output = String::new();

    for result in report
        .products()
        .iter()
        .flat_map(TestProductReport::results)
    {
        if show_output || !matches!(result.outcome(), TestOutcome::Passed) {
            append_captured_stream(&mut output, &renderer, result.standard_output(), false);
            append_captured_stream(&mut output, &renderer, result.standard_error(), true);
        }
    }

    output
}

fn append_result_rows(
    output: &mut String,
    renderer: &TestReportMessageRenderer,
    report: &TestCommandReport,
    results: &[&TestInvocationResult],
) -> (usize, usize) {
    output.push('\n');
    output.push_str(&renderer.heading(report.selection().selected()));
    output.push('\n');

    let operation_width = operation_width(*renderer);

    let identities = results
        .iter()
        .map(|result| identity_text(result.identity()))
        .collect::<Vec<_>>();

    let subject_width = max_column_width(identities.iter().map(String::as_str));

    for (result, identity) in results.iter().zip(identities) {
        let outcome = report_outcome(result.outcome());

        let duration = result
            .duration()
            .map(|duration| renderer.duration(duration.duration().as_millis()));

        let line = render_plain_line(
            renderer.fields(TestReportLineKind::Result),
            outcome_state(outcome),
            |field| match field {
                ProgressField::Operation => {
                    Some(padded(renderer.result_operation(outcome), operation_width))
                }
                ProgressField::Subject => Some(padded(&identity, subject_width)),
                ProgressField::Duration => duration.clone(),
                ProgressField::Path
                | ProgressField::Bar
                | ProgressField::Percentage
                | ProgressField::Count
                | ProgressField::Detail => None,
            },
        );

        output.push_str(&line);
        output.push('\n');
    }

    (operation_width, subject_width)
}

fn append_summary(
    output: &mut String,
    renderer: &TestReportMessageRenderer,
    report: &TestCommandReport,
    operation_width: usize,
    subject_width: usize,
) {
    let status = summary_status(report.succeeded());
    let counts = report.counts();

    let detail = renderer.summary_counts(
        counts.passed(),
        counts.failed(),
        report.selection().filtered_out(),
    );

    let duration = report
        .duration()
        .map(|duration| renderer.duration(duration.duration().as_millis()));

    let line = render_plain_line(
        renderer.fields(TestReportLineKind::Summary),
        summary_state(status),
        |field| match field {
            ProgressField::Operation => {
                Some(padded(renderer.summary_operation(status), operation_width))
            }
            ProgressField::Detail => Some(padded(&detail, subject_width)),
            ProgressField::Duration => duration.clone(),
            ProgressField::Subject
            | ProgressField::Path
            | ProgressField::Bar
            | ProgressField::Percentage
            | ProgressField::Count => None,
        },
    );

    output.push_str(&line);
    output.push('\n');
}

fn append_captured_stream(
    output: &mut String,
    renderer: &TestReportMessageRenderer,
    stream: &CapturedStream,
    standard_error: bool,
) {
    if stream.bytes().is_empty() {
        return;
    }

    output.push_str("--- ");
    output.push_str(renderer.captured_stream(standard_error));
    output.push_str(" ---\n");
    output.push_str(&String::from_utf8_lossy(stream.bytes()));

    if !output.ends_with('\n') {
        output.push('\n');
    }
}

pub(super) const fn report_outcome(outcome: &TestOutcome) -> TestReportOutcome {
    match outcome {
        TestOutcome::Passed => TestReportOutcome::Passed,
        TestOutcome::ReturnedError { .. } => TestReportOutcome::ReturnedError,
        TestOutcome::ExplicitFailure(_) => TestReportOutcome::ExplicitFailure,
        TestOutcome::AssertionFailure(_) => TestReportOutcome::AssertionFailure,
        TestOutcome::Panicked(_) => TestReportOutcome::Panicked,
        TestOutcome::TimedOut(_) => TestReportOutcome::TimedOut,
        TestOutcome::Cancelled(_) => TestReportOutcome::Cancelled,
        TestOutcome::InfrastructureFailed(_) => TestReportOutcome::InfrastructureFailed,
    }
}

#[derive(Serialize)]
struct JsonTestCommandReport<'report> {
    format: u32,
    selection: JsonTestSelectionSummary,
    products: Vec<JsonTestProductReport<'report>>,
    summary: JsonTestOutcomeCounts,
    duration_nanoseconds: Option<u64>,
}

#[derive(Serialize)]
struct JsonTestBatchReport<'report> {
    format: u32,
    plans: Vec<JsonTestBatchPlanReport<'report>>,
}

impl<'report> From<&'report [(String, TestCommandReport)]> for JsonTestBatchReport<'report> {
    fn from(reports: &'report [(String, TestCommandReport)]) -> Self {
        Self {
            format: 1,
            plans: reports
                .iter()
                .map(|(identity, report)| JsonTestBatchPlanReport {
                    identity,
                    succeeded: report.succeeded(),
                    report: JsonTestCommandReport::from(report),
                })
                .collect(),
        }
    }
}

#[derive(Serialize)]
struct JsonTestBatchPlanReport<'report> {
    identity: &'report str,
    succeeded: bool,
    report: JsonTestCommandReport<'report>,
}

impl<'report> From<&'report TestCommandReport> for JsonTestCommandReport<'report> {
    fn from(report: &'report TestCommandReport) -> Self {
        let selection = report.selection();
        let counts = report.counts();

        Self {
            format: 1,
            selection: JsonTestSelectionSummary {
                discovered: selection.discovered(),
                selected: selection.selected(),
                filtered_out: selection.filtered_out(),
            },
            products: report.products().iter().map(Into::into).collect(),
            summary: JsonTestOutcomeCounts {
                passed: counts.passed(),
                failed: counts.failed(),
            },
            duration_nanoseconds: report.duration().map(|duration| duration.nanoseconds()),
        }
    }
}

#[derive(Serialize)]
struct JsonTestSelectionSummary {
    discovered: usize,
    selected: usize,
    filtered_out: usize,
}

#[derive(Serialize)]
struct JsonTestOutcomeCounts {
    passed: usize,
    failed: usize,
}

#[derive(Serialize)]
struct JsonTestProductReport<'report> {
    package: &'report str,
    product: &'report str,
    catalog_digest: String,
    tests: Vec<JsonTestResult<'report>>,
}

impl<'report> From<&'report TestProductReport> for JsonTestProductReport<'report> {
    fn from(report: &'report TestProductReport) -> Self {
        Self {
            package: report.product().package().as_str(),
            product: report.product().name(),
            catalog_digest: lowercase_hex(&report.catalog_digest().bytes()),
            tests: report.results().iter().map(Into::into).collect(),
        }
    }
}

#[derive(Serialize)]
struct JsonTestResult<'report> {
    identity: String,
    outcome: JsonTestOutcome<'report>,
    stdout: JsonCapturedStream<'report>,
    stderr: JsonCapturedStream<'report>,
    duration_nanoseconds: Option<u64>,
}

impl<'report> From<&'report TestInvocationResult> for JsonTestResult<'report> {
    fn from(result: &'report TestInvocationResult) -> Self {
        Self {
            identity: identity_text(result.identity()),
            outcome: result.outcome().into(),
            stdout: result.standard_output().into(),
            stderr: result.standard_error().into(),
            duration_nanoseconds: result.duration().map(|duration| duration.nanoseconds()),
        }
    }
}

pub(super) fn operation_width(renderer: TestReportMessageRenderer) -> usize {
    let activities = [TestReportActivity::Waiting, TestReportActivity::Running]
        .map(|activity| renderer.activity_operation(activity));

    let outcomes = [
        TestReportOutcome::Passed,
        TestReportOutcome::ReturnedError,
        TestReportOutcome::ExplicitFailure,
        TestReportOutcome::AssertionFailure,
        TestReportOutcome::Panicked,
        TestReportOutcome::TimedOut,
        TestReportOutcome::Cancelled,
        TestReportOutcome::InfrastructureFailed,
    ]
    .map(|outcome| renderer.result_operation(outcome));

    activities
        .into_iter()
        .chain(outcomes)
        .map(|operation| operation.chars().count())
        .max()
        .unwrap_or(0)
}

pub(super) const fn outcome_state(outcome: TestReportOutcome) -> ProgressVisualState {
    match outcome {
        TestReportOutcome::Passed => ProgressVisualState::Complete,
        TestReportOutcome::ReturnedError
        | TestReportOutcome::ExplicitFailure
        | TestReportOutcome::AssertionFailure
        | TestReportOutcome::Panicked
        | TestReportOutcome::TimedOut
        | TestReportOutcome::Cancelled
        | TestReportOutcome::InfrastructureFailed => ProgressVisualState::Failed,
    }
}

pub(super) const fn summary_status(succeeded: bool) -> TestReportSummaryStatus {
    if succeeded {
        TestReportSummaryStatus::Finished
    } else {
        TestReportSummaryStatus::Failed
    }
}

pub(super) const fn summary_state(status: TestReportSummaryStatus) -> ProgressVisualState {
    match status {
        TestReportSummaryStatus::Finished => ProgressVisualState::Complete,
        TestReportSummaryStatus::Failed => ProgressVisualState::Failed,
    }
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum JsonTestOutcome<'report> {
    Passed,
    ReturnedError {
        error_type: &'report str,
        formatted_value: Option<&'report str>,
    },
    ExplicitFailure {
        source: JsonSourceAnchor,
        message: &'report str,
    },
    AssertionFailure {
        source: JsonSourceAnchor,
        message: Option<&'report str>,
    },
    Panicked {
        cause: &'static str,
        source: Option<JsonSourceAnchor>,
        message: &'report str,
    },
    TimedOut {
        nanoseconds: u64,
    },
    Cancelled {
        source: &'static str,
    },
    InfrastructureFailed {
        failure: &'static str,
        detail_code: Option<u64>,
    },
}

impl<'report> From<&'report TestOutcome> for JsonTestOutcome<'report> {
    fn from(outcome: &'report TestOutcome) -> Self {
        match outcome {
            TestOutcome::Passed => Self::Passed,
            TestOutcome::ReturnedError {
                error_type,
                formatted_value,
            } => Self::ReturnedError {
                error_type: error_type.as_str(),
                formatted_value: formatted_value.as_deref(),
            },
            TestOutcome::ExplicitFailure(failure) => Self::ExplicitFailure {
                source: failure.source().into(),
                message: failure.message(),
            },
            TestOutcome::AssertionFailure(failure) => Self::AssertionFailure {
                source: failure.source().into(),
                message: failure.message(),
            },
            TestOutcome::Panicked(report) => Self::Panicked {
                cause: panic_cause(report.cause()),
                source: report.source().map(Into::into),
                message: report.message(),
            },
            TestOutcome::TimedOut(limit) => Self::TimedOut {
                nanoseconds: limit.nanoseconds(),
            },
            TestOutcome::Cancelled(source) => Self::Cancelled {
                source: cancellation_source(*source),
            },
            TestOutcome::InfrastructureFailed(failure) => Self::InfrastructureFailed {
                failure: infrastructure_failure(failure.kind()),
                detail_code: failure.detail_code(),
            },
        }
    }
}

#[derive(Serialize)]
struct JsonCapturedStream<'report> {
    policy: &'static str,
    bytes: &'report [u8],
    truncated: bool,
    discarded_byte_count: u64,
    failure: Option<JsonStreamFailure>,
}

impl<'report> From<&'report CapturedStream> for JsonCapturedStream<'report> {
    fn from(stream: &'report CapturedStream) -> Self {
        Self {
            policy: stream_policy(stream.policy()),
            bytes: stream.bytes(),
            truncated: stream.truncated(),
            discarded_byte_count: stream.discarded_byte_count(),
            failure: stream.failure().map(|failure| JsonStreamFailure {
                kind: stream_failure(failure.kind()),
                platform_code: failure.platform_code(),
            }),
        }
    }
}

#[derive(Serialize)]
struct JsonStreamFailure {
    kind: &'static str,
    platform_code: Option<i64>,
}

#[derive(Clone, Copy, Serialize)]
struct JsonSourceAnchor {
    source: u32,
    start: u32,
    end: u32,
    version: u64,
}

impl From<TestSourceAnchor> for JsonSourceAnchor {
    fn from(source: TestSourceAnchor) -> Self {
        let span = source.span();

        Self {
            source: span.source_id().raw(),
            start: span.start().bytes(),
            end: span.end().bytes(),
            version: source.version().raw(),
        }
    }
}

const fn panic_cause(cause: TestPanicCause) -> &'static str {
    match cause {
        TestPanicCause::Message => "message",
        TestPanicCause::Assertion => "assertion",
        TestPanicCause::ExplicitFailure => "explicit_failure",
    }
}

const fn cancellation_source(source: TestCancellationSource) -> &'static str {
    match source {
        TestCancellationSource::Command => "command",
        TestCancellationSource::Invocation => "invocation",
    }
}

const fn infrastructure_failure(kind: TestInfrastructureFailureKind) -> &'static str {
    match kind {
        TestInfrastructureFailureKind::Protocol => "protocol",
        TestInfrastructureFailureKind::Host => "host",
        TestInfrastructureFailureKind::Capture => "capture",
        TestInfrastructureFailureKind::ResourceExhausted => "resource_exhausted",
        TestInfrastructureFailureKind::MissingOutcome => "missing_outcome",
        TestInfrastructureFailureKind::Cleanup => "cleanup",
        TestInfrastructureFailureKind::ForcedTermination => "forced_termination",
    }
}

const fn stream_policy(policy: CapturedStreamPolicy) -> &'static str {
    match policy {
        CapturedStreamPolicy::Captured => "captured",
        CapturedStreamPolicy::Inherited => "inherited",
        CapturedStreamPolicy::Discarded => "discarded",
    }
}

const fn stream_failure(kind: bray_test_protocol::TestStreamFailureKind) -> &'static str {
    match kind {
        bray_test_protocol::TestStreamFailureKind::ResourceExhausted => "resource_exhausted",
        bray_test_protocol::TestStreamFailureKind::SinkUnavailable => "sink_unavailable",
        bray_test_protocol::TestStreamFailureKind::Platform => "platform",
    }
}

#[cfg(test)]
mod tests {
    use bray_source::{SourceId, SourceSpan, SourceVersion, TextRange, TextSize};
    use bray_symbols::{ModulePathKey, ProductIdentity, SymbolName};
    use bray_test_protocol::{
        AssertionFailure, CapturedStream, TestCatalog, TestCatalogDigest, TestCommandReport,
        TestDeclarationPath, TestDuration, TestIdentity, TestInvocationResult, TestOutcome,
        TestProductReport, TestSelectionSummary, TestSourceAnchor, encode_test_catalog,
    };
    use bray_tooling::OutputFormat;

    use super::super::test_support::product;
    use super::render_report;

    #[test]
    fn text_reports_keep_result_order_and_show_failed_output() {
        let product = product();
        let digest = catalog_digest(&product);

        let results = [
            result(product.clone(), "first", TestOutcome::Passed, b"hidden"),
            result(
                product.clone(),
                "second",
                TestOutcome::AssertionFailure(AssertionFailure::with_message(
                    source(),
                    "expected equality",
                )),
                b"visible",
            ),
        ];

        let report = TestCommandReport::new(
            TestSelectionSummary::new(2, 2),
            [TestProductReport::new(product, digest, results)],
        )
        .with_duration(TestDuration::from_nanoseconds(9_000_000));

        let rendered = render_report(&report, OutputFormat::Text, false, false)
            .unwrap_or_else(|diagnostics| panic!("report must render: {diagnostics:?}"));

        let first = rendered
            .find("first")
            .unwrap_or_else(|| panic!("first result must be rendered"));

        let second = rendered
            .find("second")
            .unwrap_or_else(|| panic!("second result must be rendered"));

        assert!(first < second);
        assert!(rendered.starts_with("\nRunning 2 tests\n"));
        assert!(rendered.contains("✓ Passed"));
        assert!(rendered.contains("× Assertion failed"));
        assert!(!rendered.contains("hidden"));
        assert!(rendered.contains("visible"));

        let result_line = rendered
            .lines()
            .find(|line| line.contains("second"))
            .unwrap_or_else(|| panic!("test result line must be rendered"));

        let summary_line = rendered
            .lines()
            .last()
            .unwrap_or_else(|| panic!("test summary line must be rendered"));

        assert!(summary_line.starts_with("   × Failed"));
        assert!(summary_line.contains("1 passed, 1 failed"));
        assert_eq!(result_line.find("6 ms"), summary_line.find("9 ms"));
    }

    #[test]
    fn json_reports_preserve_typed_failure_and_capture_facts() {
        let product = product();

        let outcome = TestOutcome::AssertionFailure(AssertionFailure::with_message(
            source(),
            "expected equality",
        ));

        let report = TestCommandReport::new(
            TestSelectionSummary::new(3, 1),
            [TestProductReport::new(
                product.clone(),
                catalog_digest(&product),
                [result(product, "fails", outcome, b"prefix")],
            )],
        )
        .with_duration(TestDuration::from_nanoseconds(12_000_000));

        let rendered = render_report(&report, OutputFormat::Json, false, false)
            .unwrap_or_else(|diagnostics| panic!("report must render: {diagnostics:?}"));

        let report: serde_json::Value = serde_json::from_str(&rendered)
            .unwrap_or_else(|error| panic!("report must be valid JSON: {error}"));

        assert_eq!(report["format"], 1);
        assert_eq!(report["selection"]["filtered_out"], 2);

        assert_eq!(
            report["products"][0]["tests"][0]["outcome"]["kind"],
            "assertion_failure"
        );

        assert_eq!(
            report["products"][0]["tests"][0]["outcome"]["message"],
            "expected equality"
        );

        assert_eq!(
            report["products"][0]["tests"][0]["stdout"]["policy"],
            "captured"
        );

        assert_eq!(
            report["products"][0]["tests"][0]["stdout"]["bytes"],
            serde_json::json!([112, 114, 101, 102, 105, 120])
        );

        assert_eq!(report["summary"]["failed"], 1);
        assert_eq!(report["duration_nanoseconds"], 12_000_000);

        assert_eq!(
            report["products"][0]["tests"][0]["duration_nanoseconds"],
            6_000_000
        );
    }

    fn result(
        product: ProductIdentity,
        name: &str,
        outcome: TestOutcome,
        output: &[u8],
    ) -> TestInvocationResult {
        let module = ModulePathKey::try_new(["example", "tests"])
            .unwrap_or_else(|| panic!("test module path must be valid"));

        let name = SymbolName::try_new(name).unwrap_or_else(|| panic!("test name must be valid"));

        let identity = TestIdentity::new(product, TestDeclarationPath::new(module, name));

        TestInvocationResult::after_cleanup(
            identity,
            outcome,
            CapturedStream::captured(output.iter().copied(), 0, None),
            CapturedStream::captured([], 0, None),
        )
        .with_duration(TestDuration::from_nanoseconds(6_000_000))
    }

    fn source() -> TestSourceAnchor {
        TestSourceAnchor::new(
            SourceSpan::new(
                SourceId::new(4),
                TextRange::new(TextSize::new(8), TextSize::new(13)),
            ),
            SourceVersion::new(2),
        )
    }

    fn catalog_digest(product: &ProductIdentity) -> TestCatalogDigest {
        let catalog = TestCatalog::try_new(product.clone(), [])
            .unwrap_or_else(|error| panic!("empty test catalog must be valid: {error:?}"));

        encode_test_catalog(&catalog)
            .unwrap_or_else(|error| panic!("test catalog must encode: {error:?}"))
            .1
    }
}
