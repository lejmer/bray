use std::collections::BTreeMap;
use std::fs;
use std::hash::Hasher;
use std::io::Write;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use bray_base::StableDigestHasher;
use bray_diagnostics::{
    DiagnosticBag, DiagnosticProjectCommandFailure, DiagnosticProjectOperation,
    DiagnosticProjectSelectionProblem, DiagnosticTestExecutionPlanProblem,
    DiagnosticTestSchedulingProblem,
};
use bray_platform::{NativeChildProcess, NativePipeWriter, NativeProcessCommand, NativeStdio};
use bray_test_protocol::{
    CapturedStream, TestAdmission, TestAdmissionSchedule, TestCapturePolicy, TestCatalogEntryId,
    TestCommandReport, TestDuration, TestExecutionMode, TestExecutionPlan,
    TestExecutionPlanBuildError, TestHostCommand, TestHostCommandId, TestHostControl, TestIdentity,
    TestInfrastructureFailure, TestInfrastructureFailureKind, TestInvocationPlan,
    TestInvocationResult, TestOutcome, TestSchedulingError, TestSelection, TestSelectionQuery,
    TestSelectionSummary, TestStopReason, read_host_result, write_host_command, write_host_control,
};
use bray_tooling::OutputFormat;

use super::model::{BuiltTestHost, HostLocation, LoadedTestHost};
use super::progress::TestProgress;
use super::report::{product_reports, render_report};
use crate::tack::error::{operation_diagnostics, selection_diagnostics};
use crate::tack::model::TackTestOptions;

static COMMAND_CANCELLED: AtomicBool = AtomicBool::new(false);
static CANCELLATION_HANDLER: OnceLock<Result<(), ()>> = OnceLock::new();
const HOST_POLL_INTERVAL: Duration = Duration::from_millis(5);
const CANCELLATION_GRACE_PERIOD: Duration = Duration::from_secs(1);

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
) -> Result<(TestCommandReport, String), DiagnosticBag> {
    prepare_command_cancellation()?;

    let hosts = load_hosts(hosts)?;
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

    let locations = host_locations(&hosts);

    let mut progress = TestProgress::new(&plan, interactive);
    let started_at = Instant::now();

    let results = match run_schedule(workspace_root, plan, &locations, &mut progress) {
        Ok(results) => results,
        Err(diagnostics) => {
            progress.fail();

            return Err(diagnostics);
        }
    };

    let products = product_reports(&hosts, results);

    let mut report =
        TestCommandReport::new(TestSelectionSummary::new(discovered, selected), products);

    if let Some(duration) = TestDuration::try_from_duration(started_at.elapsed()) {
        report = report.with_duration(duration);
    }

    progress.finish(&report, options.show_output);

    let rendered = render_report(&report, output_format, options.show_output, interactive)?;

    Ok((report, rendered))
}

fn load_hosts(hosts: Vec<BuiltTestHost>) -> Result<Vec<LoadedTestHost>, DiagnosticBag> {
    let mut loaded = Vec::with_capacity(hosts.len());

    for host in hosts {
        loaded.push(host.load().ok_or_else(|| {
            operation_diagnostics(DiagnosticProjectCommandFailure::MissingResult(
                DiagnosticProjectOperation::TestHostPublication,
            ))
        })?);
    }

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
            return Err(operation_diagnostics(
                DiagnosticProjectCommandFailure::Invariant(
                    DiagnosticProjectOperation::TestAdmission,
                ),
            ));
        }

        let completion = match receiver.recv_timeout(HOST_POLL_INTERVAL) {
            Ok(completion) => completion,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(operation_diagnostics(
                    DiagnosticProjectCommandFailure::Invariant(
                        DiagnosticProjectOperation::TestHostCompletion,
                    ),
                ));
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

fn run_host(
    workspace_root: &Path,
    admission: &TestAdmission,
    location: &HostLocation,
) -> TestInvocationResult {
    let Ok(result_path) = result_path() else {
        return infrastructure_result(admission);
    };

    let command_id = command_id(&result_path, admission);

    let command = TestHostCommand::new(
        command_id,
        location.catalog_digest,
        location.entry,
        admission.invocation().timeout(),
        admission.invocation().capture(),
        admission.entry().error_type().cloned(),
    );

    let result = match run_host_process(workspace_root, location, command, &result_path) {
        Ok(HostProcessCompletion::Completed) => fs::read(&result_path)
            .map_err(|_| ())
            .and_then(|bytes| read_host_result(&mut bytes.as_slice()).map_err(|_| ()))
            .and_then(|result| {
                if result.command_id() == command_id
                    && result.catalog_digest() == location.catalog_digest
                {
                    Ok(result)
                } else {
                    Err(())
                }
            })
            .map(|result| result.into_invocation_result(admission.invocation().identity().clone()))
            .unwrap_or_else(|()| infrastructure_result(admission)),
        Ok(HostProcessCompletion::ForcedTermination) => forced_termination_result(admission),
        Err(()) => infrastructure_result(admission),
    };

    let _ = fs::remove_file(result_path);

    result
}

fn run_host_process(
    workspace_root: &Path,
    location: &HostLocation,
    command: TestHostCommand,
    result_path: &Path,
) -> Result<HostProcessCompletion, ()> {
    let mut process = NativeProcessCommand::new(&location.executable).map_err(|_| ())?;

    process
        .current_dir(workspace_root)
        .env("BRAY_TEST_RESULT_PATH", result_path)
        .stdin(NativeStdio::Piped);

    match command.capture() {
        TestCapturePolicy::Inherited => {
            process
                .stdout(NativeStdio::Inherit)
                .stderr(NativeStdio::Inherit);
        }
        TestCapturePolicy::Captured(_) | TestCapturePolicy::Discarded => {
            process.stdout(NativeStdio::Null).stderr(NativeStdio::Null);
        }
    }

    let child = process.spawn().map_err(|_| ())?;
    let mut child = ReapedChild::new(child);
    let mut input = child.take_stdin().ok_or(())?;

    write_host_command(&mut input, &command).map_err(|_| ())?;
    input.flush().map_err(|_| ())?;

    let timeout_deadline = match command.timeout() {
        bray_test_protocol::TestTimeoutPolicy::Unlimited => None,
        bray_test_protocol::TestTimeoutPolicy::Limit(limit) => {
            Some(Instant::now() + limit.duration() + CANCELLATION_GRACE_PERIOD)
        }
    };

    let mut cancellation_deadline = None;

    loop {
        if child.try_wait()?.is_some() {
            return Ok(HostProcessCompletion::Completed);
        }

        if cancellation_deadline.is_none() && COMMAND_CANCELLED.load(Ordering::Acquire) {
            write_host_control(&mut input, TestHostControl::Cancel).map_err(|_| ())?;
            input.flush().map_err(|_| ())?;
            cancellation_deadline = Some(Instant::now() + CANCELLATION_GRACE_PERIOD);
        }

        if timeout_deadline.is_some_and(|deadline| Instant::now() >= deadline)
            || cancellation_deadline.is_some_and(|deadline| Instant::now() >= deadline)
        {
            child.force_terminate()?;

            return Ok(HostProcessCompletion::ForcedTermination);
        }

        std::thread::sleep(HOST_POLL_INTERVAL);
    }
}

struct ReapedChild {
    process: NativeChildProcess,
    reaped: bool,
}

impl ReapedChild {
    const fn new(process: NativeChildProcess) -> Self {
        Self {
            process,
            reaped: false,
        }
    }

    fn take_stdin(&mut self) -> Option<NativePipeWriter> {
        self.process.take_stdin()
    }

    fn try_wait(&mut self) -> Result<Option<bray_platform::NativeExitStatus>, ()> {
        let status = self.process.try_wait().map_err(|_| ())?;

        self.reaped = status.is_some();

        Ok(status)
    }

    fn force_terminate(&mut self) -> Result<(), ()> {
        self.process.terminate().map_err(|_| ())?;
        self.process.wait().map_err(|_| ())?;
        self.reaped = true;

        Ok(())
    }
}

impl Drop for ReapedChild {
    fn drop(&mut self) {
        if self.reaped {
            return;
        }

        let _ = self.process.terminate();
        let _ = self.process.wait();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HostProcessCompletion {
    Completed,
    ForcedTermination,
}

fn infrastructure_result(admission: &TestAdmission) -> TestInvocationResult {
    infrastructure_result_with_kind(admission, TestInfrastructureFailureKind::Host)
}

fn forced_termination_result(admission: &TestAdmission) -> TestInvocationResult {
    infrastructure_result_with_kind(admission, TestInfrastructureFailureKind::ForcedTermination)
}

fn command_cancellation_result(invocation: &TestInvocationPlan) -> TestInvocationResult {
    TestInvocationResult::after_cleanup(
        invocation.identity().clone(),
        TestOutcome::Cancelled(bray_test_protocol::TestCancellationSource::Command),
        empty_stream(invocation.capture()),
        empty_stream(invocation.capture()),
    )
}

fn infrastructure_result_with_kind(
    admission: &TestAdmission,
    kind: TestInfrastructureFailureKind,
) -> TestInvocationResult {
    TestInvocationResult::after_cleanup(
        admission.invocation().identity().clone(),
        TestOutcome::InfrastructureFailed(TestInfrastructureFailure::new(kind, None)),
        empty_stream(admission.invocation().capture()),
        empty_stream(admission.invocation().capture()),
    )
}

fn empty_stream(capture: TestCapturePolicy) -> CapturedStream {
    match capture {
        TestCapturePolicy::Captured(_) => CapturedStream::captured([], 0, None),
        TestCapturePolicy::Inherited => CapturedStream::inherited(None),
        TestCapturePolicy::Discarded => CapturedStream::discarded(),
    }
}

fn result_path() -> Result<PathBuf, ()> {
    let file = tempfile::NamedTempFile::new().map_err(|_| ())?;

    let (_, path) = file.keep().map_err(|_| ())?;

    Ok(path)
}

fn command_id(result_path: &Path, admission: &TestAdmission) -> TestHostCommandId {
    let mut digest = StableDigestHasher::new();

    digest.write(result_path.as_os_str().to_string_lossy().as_bytes());

    digest.write(
        admission
            .invocation()
            .identity()
            .declaration()
            .name()
            .as_str()
            .as_bytes(),
    );

    TestHostCommandId::from_bytes(digest.finalize())
}

fn prepare_command_cancellation() -> Result<(), DiagnosticBag> {
    COMMAND_CANCELLED.store(false, Ordering::Release);

    let installed = CANCELLATION_HANDLER.get_or_init(|| {
        ctrlc::set_handler(|| COMMAND_CANCELLED.store(true, Ordering::Release)).map_err(|_| ())
    });

    installed.as_ref().map(|()| ()).map_err(|()| {
        operation_diagnostics(DiagnosticProjectCommandFailure::Invariant(
            DiagnosticProjectOperation::TestCancellationHandler,
        ))
    })
}

const fn capture_reservation(capture: TestCapturePolicy) -> u64 {
    match capture {
        TestCapturePolicy::Captured(limits) => limits.invocation_byte_limit(),
        TestCapturePolicy::Inherited | TestCapturePolicy::Discarded => 0,
    }
}
