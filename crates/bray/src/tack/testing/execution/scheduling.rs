use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::path::Path;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Instant;

use bray_diagnostics::{
    DiagnosticBag, DiagnosticIoErrorKind, DiagnosticProjectCommandFailure,
    DiagnosticProjectOperation, DiagnosticProjectSelectionProblem,
    DiagnosticTestExecutionPlanProblem, DiagnosticTestSchedulingProblem,
};
use bray_test_protocol::{
    TestAdmissionSchedule, TestBatchRequest, TestCaptureLimits, TestCapturePolicy,
    TestCatalogEntryId, TestCommandReport, TestDuration, TestExecutionMode, TestExecutionPlan,
    TestExecutionPlanBuildError, TestIdentity, TestInvocationPlan, TestInvocationResult,
    TestSchedulingError, TestSelection, TestSelectionQuery, TestSelectionSummary, TestStopReason,
};
use bray_tooling::OutputFormat;

use super::host::{HOST_POLL_INTERVAL, command_cancellation_result, run_host};
use super::super::model::{BuiltTestHost, HostLocation, LoadedTestHost};
use super::super::progress::TestProgress;
use super::super::report::{product_reports, render_batch_report, render_report};
use crate::tack::error::{operation_diagnostics, selection_diagnostics};
use crate::tack::model::TackTestOptions;

pub(super) static COMMAND_CANCELLED: AtomicBool = AtomicBool::new(false);
static CANCELLATION_HANDLER: OnceLock<Result<(), ctrlc::Error>> = OnceLock::new();

fn test_execution_plan_problem(
    error: TestExecutionPlanBuildError,
) -> DiagnosticTestExecutionPlanProblem {
    match error {
        TestExecutionPlanBuildError::InvocationCountMismatch {
            entries,
            invocations,
        } => DiagnosticTestExecutionPlanProblem::InvocationCountMismatch {
            entries: u32::try_from(entries).unwrap_or(u32::MAX),
            invocations: u32::try_from(invocations).unwrap_or(u32::MAX),
        },
        TestExecutionPlanBuildError::InvocationIdentityMismatch(test) => {
            DiagnosticTestExecutionPlanProblem::InvocationIdentityMismatch(test_identity(&test))
        }
        TestExecutionPlanBuildError::DuplicateIdentity(test) => {
            DiagnosticTestExecutionPlanProblem::DuplicateIdentity(test_identity(&test))
        }
        TestExecutionPlanBuildError::CaptureBudgetExceeded {
            test,
            required,
            maximum,
        } => DiagnosticTestExecutionPlanProblem::CaptureBudgetExceeded {
            test: test_identity(&test),
            required_bytes: required,
            maximum_bytes: maximum,
        },
    }
}

fn test_identity(identity: &TestIdentity) -> String {
    let declaration = identity.declaration();

    let module = declaration
        .module()
        .segments()
        .collect::<Vec<_>>()
        .join("::");

    format!(
        "{}::{module}::{}",
        identity.product(),
        declaration.name().as_str()
    )
}

pub(crate) fn execute(
    workspace_root: &Path,
    hosts: Vec<BuiltTestHost>,
    options: &TackTestOptions,
    worker_count: usize,
    output_format: OutputFormat,
    interactive: bool,
    build: &super::super::report::TestBuildProvenance,
) -> Result<(TestCommandReport, String), DiagnosticBag> {
    prepare_command_cancellation()?;

    let hosts = load_hosts(hosts)?;

    let report = execute_loaded(
        workspace_root,
        &hosts,
        options,
        worker_count,
        interactive,
        false,
    )?;

    let rendered = render_report(
        &report,
        build,
        output_format,
        options.show_output,
        interactive,
    )?;

    Ok((report, rendered))
}

pub(crate) fn execute_batch(
    workspace_root: &Path,
    hosts: Vec<BuiltTestHost>,
    request: &TestBatchRequest,
    worker_count: usize,
    interactive: bool,
    build: &super::super::report::TestBuildProvenance,
) -> Result<(Vec<(String, TestCommandReport)>, String), DiagnosticBag> {
    prepare_command_cancellation()?;

    let hosts = load_hosts(hosts)?;
    let mut reports = Vec::with_capacity(request.plans().len());

    for plan in request.plans() {
        let options = TackTestOptions::new(
            plan.filters().to_vec(),
            plan.maximum_concurrency(),
            plan.timeout_milliseconds(),
            TestCapturePolicy::Captured(TestCaptureLimits::new(1_048_576, 2_097_152)),
            false,
        );

        let report = execute_loaded(
            workspace_root,
            &hosts,
            &options,
            worker_count,
            interactive,
            true,
        )?;

        reports.push((plan.identity().to_owned(), report));
    }

    let rendered = render_batch_report(&reports, build)?;

    Ok((reports, rendered))
}

fn execute_loaded(
    workspace_root: &Path,
    hosts: &[LoadedTestHost],
    options: &TackTestOptions,
    worker_count: usize,
    interactive: bool,
    live_results: bool,
) -> Result<TestCommandReport, DiagnosticBag> {
    let query = selection_query(options)?;

    let selections = hosts
        .iter()
        .map(|host| TestSelection::from_catalog(&host.catalog, &query))
        .collect::<Vec<_>>();

    let discovered = selections.iter().map(TestSelection::discovered_count).sum();

    let selected = selections
        .iter()
        .map(|selection| selection.entries().len())
        .sum();

    let maximum_concurrency =
        NonZeroUsize::new(options.maximum_concurrency.min(worker_count).max(1))
            .unwrap_or(NonZeroUsize::MIN);

    let mode = if maximum_concurrency.get() == 1 {
        TestExecutionMode::Sequential
    } else {
        TestExecutionMode::Parallel {
            maximum_concurrency,
        }
    };

    let invocations = selections.iter().flat_map(|selection| {
        selection.entries().iter().map(|entry| {
            TestInvocationPlan::new(entry.identity().clone(), options.timeout, options.capture)
        })
    });

    let capture_budget = capture_reservation(options.capture)
        .saturating_mul(u64::try_from(maximum_concurrency.get()).unwrap_or(u64::MAX));

    let plan = TestExecutionPlan::try_new(&selections, invocations, mode, capture_budget).map_err(
        |error| {
            operation_diagnostics(DiagnosticProjectCommandFailure::TestExecutionPlan(
                test_execution_plan_problem(error),
            ))
        },
    )?;

    let locations = host_locations(hosts);

    let mut progress = TestProgress::new(&plan, interactive, live_results);
    let started_at = Instant::now();

    let results = match run_schedule(workspace_root, plan, &locations, &mut progress) {
        Ok(results) => results,
        Err(diagnostics) => {
            progress.fail();

            return Err(diagnostics);
        }
    };

    let products = product_reports(hosts, results);

    let mut report =
        TestCommandReport::new(TestSelectionSummary::new(discovered, selected), products);

    if let Some(duration) = TestDuration::try_from_duration(started_at.elapsed()) {
        report = report.with_duration(duration);
    }

    progress.finish(&report, options.show_output);

    Ok(report)
}

fn load_hosts(hosts: Vec<BuiltTestHost>) -> Result<Vec<LoadedTestHost>, DiagnosticBag> {
    let mut loaded = hosts
        .into_iter()
        .map(BuiltTestHost::load)
        .collect::<Result<Vec<_>, _>>()
        .map_err(operation_diagnostics)?;

    loaded.sort_by(|left, right| left.catalog.product().cmp(right.catalog.product()));

    Ok(loaded)
}

fn selection_query(options: &TackTestOptions) -> Result<TestSelectionQuery, DiagnosticBag> {
    let filters = options
        .filters
        .iter()
        .map(|filter| {
            bray_test_protocol::TestFilter::name_contains(filter.as_str()).ok_or_else(|| {
                selection_diagnostics(DiagnosticProjectSelectionProblem::InvalidTestFilter(
                    filter.clone(),
                ))
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(TestSelectionQuery::new(filters, None))
}

fn host_locations(
    hosts: &[LoadedTestHost],
) -> BTreeMap<bray_test_protocol::TestIdentity, HostLocation> {
    let mut locations = BTreeMap::new();

    for host in hosts {
        for (index, entry) in host.catalog.entries().iter().enumerate() {
            let Ok(index) = u32::try_from(index) else {
                continue;
            };

            locations.insert(
                entry.identity().clone(),
                HostLocation {
                    executable: host.executable.clone(),
                    entry: TestCatalogEntryId::new(index),
                    catalog_digest: host.digest,
                },
            );
        }
    }

    locations
}

fn run_schedule(
    workspace_root: &Path,
    plan: TestExecutionPlan,
    locations: &BTreeMap<bray_test_protocol::TestIdentity, HostLocation>,
    progress: &mut TestProgress,
) -> Result<Vec<TestInvocationResult>, DiagnosticBag> {
    let invocations = plan.invocations().to_vec();
    let mut schedule = TestAdmissionSchedule::new(plan);

    let (sender, receiver) = mpsc::channel();

    let mut active = 0;
    let mut cancelled = false;

    while !schedule.completed_all() && !(cancelled && schedule.settled()) {
        if !cancelled && COMMAND_CANCELLED.load(Ordering::Acquire) {
            schedule.stop(TestStopReason::Cancelled);
            cancelled = true;
        }

        while let Some(admission) = schedule.admit_next() {
            let Some(location) = locations.get(admission.invocation().identity()).cloned() else {
                return Err(operation_diagnostics(
                    DiagnosticProjectCommandFailure::MissingTestHostLocation(test_identity(
                        admission.invocation().identity(),
                    )),
                ));
            };

            let workspace_root = workspace_root.to_path_buf();
            let sender = sender.clone();
            let worker_admission = admission.clone();

            progress.start(admission.invocation().identity());

            std::thread::spawn(move || {
                let started_at = Instant::now();
                let result = run_host(&workspace_root, &worker_admission, &location);
                let duration = TestDuration::try_from_duration(started_at.elapsed());
                let _ = sender.send((worker_admission, result, duration));
            });

            active += 1;
        }

        if cancelled && schedule.settled() {
            break;
        }

        if active == 0 {
            panic!(
                "validated project Invariant contract failed: {:?}",
                DiagnosticProjectOperation::TestAdmission
            );
        }

        let completion = match receiver.recv_timeout(HOST_POLL_INTERVAL) {
            Ok(completion) => completion,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                panic!(
                    "validated project Invariant contract failed: {:?}",
                    DiagnosticProjectOperation::TestHostCompletion
                );
            }
        };

        let (admission, mut result, duration) = completion;

        if let Some(duration) = duration {
            result = result.with_duration(duration);
        }

        active -= 1;

        progress.finish_result(&result);

        schedule.complete(&admission, result).map_err(|error| {
            let problem = match error {
                TestSchedulingError::InvocationNotActive(test) => {
                    DiagnosticTestSchedulingProblem::InvocationNotActive(test_identity(&test))
                }
                TestSchedulingError::ResultIdentityMismatch(test) => {
                    DiagnosticTestSchedulingProblem::ResultIdentityMismatch(test_identity(&test))
                }
            };

            operation_diagnostics(DiagnosticProjectCommandFailure::TestScheduling(problem))
        })?;
    }

    let completed = schedule
        .completed_results()
        .cloned()
        .map(|result| (result.identity().clone(), result))
        .collect::<BTreeMap<_, _>>();

    let results = invocations
        .into_iter()
        .map(|invocation| {
            completed
                .get(invocation.identity())
                .cloned()
                .unwrap_or_else(|| command_cancellation_result(&invocation))
        })
        .collect::<Vec<_>>();

    for result in &results {
        progress.finish_result(result);
    }

    Ok(results)
}

fn prepare_command_cancellation() -> Result<(), DiagnosticBag> {
    COMMAND_CANCELLED.store(false, Ordering::Release);

    let installed = CANCELLATION_HANDLER
        .get_or_init(|| ctrlc::set_handler(|| COMMAND_CANCELLED.store(true, Ordering::Release)));

    installed
        .as_ref()
        .copied()
        .map_err(cancellation_handler_diagnostics)
}

fn cancellation_handler_diagnostics(error: &ctrlc::Error) -> DiagnosticBag {
    match error {
        ctrlc::Error::System(error) => {
            operation_diagnostics(DiagnosticProjectCommandFailure::HostIo {
                operation: DiagnosticProjectOperation::TestCancellationHandler,
                error: DiagnosticIoErrorKind::from(error.kind()),
            })
        }
        ctrlc::Error::MultipleHandlers | ctrlc::Error::NoSuchSignal(_) => {
            panic!("test cancellation handler registration contract failed: {error:?}")
        }
    }
}

const fn capture_reservation(capture: TestCapturePolicy) -> u64 {
    match capture {
        TestCapturePolicy::Captured(limits) => limits.invocation_byte_limit(),
        TestCapturePolicy::Inherited | TestCapturePolicy::Discarded => 0,
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{
        DiagnosticArgValue, DiagnosticIoErrorKind, DiagnosticKind, DiagnosticProjectCommandFailure,
        DiagnosticProjectOperation,
    };
    use bray_messages::DiagnosticRenderer;
    use bray_test_protocol::{TestBatchPlan, TestBatchRequest};

    use super::super::super::test_support::write_empty_test_host;
    use super::{BuiltTestHost, cancellation_handler_diagnostics};

    #[test]
    fn cancellation_handler_system_failure_preserves_its_operation_and_io_cause() {
        for (kind, expected, reason) in [
            (
                std::io::ErrorKind::PermissionDenied,
                DiagnosticIoErrorKind::PermissionDenied,
                "permission denied",
            ),
            (
                std::io::ErrorKind::OutOfMemory,
                DiagnosticIoErrorKind::OutOfMemory,
                "host memory is exhausted",
            ),
        ] {
            let error = ctrlc::Error::System(std::io::Error::from(kind));
            let diagnostics = cancellation_handler_diagnostics(&error);
            let diagnostic = bray_testing::single_diagnostic(&diagnostics);

            assert_eq!(diagnostic.kind(), DiagnosticKind::ProjectCommandFailed);

            let failure = diagnostic
                .args()
                .iter()
                .find_map(|argument| match argument.value() {
                    DiagnosticArgValue::ProjectCommandFailure(failure) => Some(failure),
                    _ => None,
                });

            assert_eq!(
                failure,
                Some(&DiagnosticProjectCommandFailure::HostIo {
                    operation: DiagnosticProjectOperation::TestCancellationHandler,
                    error: expected,
                })
            );

            assert_eq!(
                DiagnosticRenderer::english().render(diagnostic).message(),
                format!(
                    "project command failed: could not install the test cancellation handler because the host reported {reason}"
                )
            );
        }
    }

    #[test]
    #[should_panic(
        expected = "test cancellation handler registration contract failed: MultipleHandlers"
    )]
    fn cancellation_handler_duplicate_registration_is_an_invariant_failure() {
        cancellation_handler_diagnostics(&ctrlc::Error::MultipleHandlers);
    }

    #[test]
    #[should_panic(
        expected = "test cancellation handler registration contract failed: NoSuchSignal(Ctrlc)"
    )]
    fn cancellation_handler_missing_builtin_signal_is_an_invariant_failure() {
        cancellation_handler_diagnostics(&ctrlc::Error::NoSuchSignal(ctrlc::SignalType::Ctrlc));
    }

    #[test]
    fn one_fresh_host_executes_multiple_ordered_plans_without_reloading_artifacts() {
        let directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("test directory should be created: {error:?}"));

        let (executable, catalog) = write_empty_test_host(directory.path());

        let host = BuiltTestHost::try_new(executable, catalog)
            .unwrap_or_else(|failure| panic!("fresh test host should be valid: {failure:?}"));

        let first = TestBatchPlan::try_new("first", [], 1, None)
            .unwrap_or_else(|error| panic!("first plan should be valid: {error:?}"));

        let second = TestBatchPlan::try_new("second", [], 2, Some(1000))
            .unwrap_or_else(|error| panic!("second plan should be valid: {error:?}"));

        let request = TestBatchRequest::try_new([first, second])
            .unwrap_or_else(|error| panic!("batch request should be valid: {error:?}"));

        let (reports, rendered) = super::execute_batch(
            directory.path(),
            vec![host],
            &request,
            2,
            false,
            &crate::tack::testing::TestBuildProvenance::new(false),
        )
        .unwrap_or_else(|diagnostics| panic!("batch should execute: {diagnostics:?}"));

        assert_eq!(
            reports
                .iter()
                .map(|(identity, _)| identity.as_str())
                .collect::<Vec<_>>(),
            ["first", "second"]
        );

        assert!(reports.iter().all(|(_, report)| report.succeeded()));
        assert!(rendered.contains("\"identity\": \"first\""));
        assert!(rendered.contains("\"identity\": \"second\""));
    }
}
