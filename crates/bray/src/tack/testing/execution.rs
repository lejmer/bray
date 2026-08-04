use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use bray_diagnostics::DiagnosticBag;
use bray_platform::{NativeProcessCommand, NativeStdio};
use bray_test_protocol::{
    CapturedStream, TestAdmission, TestAdmissionSchedule, TestCapturePolicy, TestCatalogEntryId,
    TestCommandReport, TestExecutionMode, TestExecutionPlan, TestHostCommand,
    TestInfrastructureFailure, TestInfrastructureFailureKind, TestInvocationPlan,
    TestInvocationResult, TestOutcome, TestSelection, TestSelectionQuery, TestSelectionSummary,
    decode_test_catalog, read_host_result, write_host_command,
};
use bray_tooling::OutputFormat;

use super::model::{BuiltTestHost, HostLocation, LoadedTestHost};
use super::report::{product_reports, render_report};
use crate::tack::error::operation_diagnostics;
use crate::tack::model::TackTestOptions;

static NEXT_RESULT_FILE: AtomicU64 = AtomicU64::new(0);
const HOST_POLL_INTERVAL: Duration = Duration::from_millis(5);
const CANCELLATION_GRACE_PERIOD: Duration = Duration::from_secs(1);

pub(crate) fn execute(
    workspace_root: &Path,
    hosts: Vec<BuiltTestHost>,
    options: &TackTestOptions,
    worker_count: usize,
    output_format: OutputFormat,
) -> Result<(TestCommandReport, String), DiagnosticBag> {
    let hosts = load_hosts(hosts)?;
    let query = selection_query(options)?;

    let selections = hosts
        .iter()
        .map(|host| TestSelection::from_catalog(&host.catalog, &query))
        .collect::<Vec<_>>();

    let discovered = selections.iter().map(TestSelection::discovered_count).sum();
    let selected = selections.iter().map(|selection| selection.entries().len()).sum();
    let maximum_concurrency = options.maximum_concurrency.min(worker_count).max(1);

    let mode = if maximum_concurrency == 1 {
        TestExecutionMode::Sequential
    } else {
        TestExecutionMode::Parallel {
            maximum_concurrency: NonZeroUsize::new(maximum_concurrency)
                .ok_or_else(|| operation_diagnostics("test_concurrency"))?,
        }
    };

    let invocations = selections.iter().flat_map(|selection| {
        selection.entries().iter().map(|entry| {
            TestInvocationPlan::new(entry.identity().clone(), options.timeout, options.capture)
        })
    });

    let capture_budget = capture_reservation(options.capture)
        .saturating_mul(u64::try_from(maximum_concurrency).unwrap_or(u64::MAX));

    let plan = TestExecutionPlan::try_new(&selections, invocations, mode, capture_budget)
        .map_err(|_| operation_diagnostics("test_execution_plan"))?;

    let locations = host_locations(&hosts);
    let results = run_schedule(workspace_root, plan, &locations)?;
    let products = product_reports(&hosts, results);
    let report = TestCommandReport::new(TestSelectionSummary::new(discovered, selected), products);
    let rendered = render_report(&report, output_format, options.show_output)?;

    Ok((report, rendered))
}

fn load_hosts(hosts: Vec<BuiltTestHost>) -> Result<Vec<LoadedTestHost>, DiagnosticBag> {
    let mut loaded = Vec::with_capacity(hosts.len());

    for host in hosts {
        let bytes = fs::read(&host.catalog_path)
            .map_err(|_| operation_diagnostics("test_catalog_read"))?;

        let (catalog, digest) = decode_test_catalog(&bytes)
            .map_err(|_| operation_diagnostics("test_catalog_decode"))?;

        loaded.push(LoadedTestHost {
            executable: host.executable,
            catalog,
            digest,
        });
    }

    loaded.sort_by(|left, right| left.catalog.product().cmp(right.catalog.product()));

    Ok(loaded)
}

fn selection_query(options: &TackTestOptions) -> Result<TestSelectionQuery, DiagnosticBag> {
    let filters = options
        .filters
        .iter()
        .map(|filter| {
            bray_test_protocol::TestFilter::name_contains(filter.as_str())
                .ok_or_else(|| operation_diagnostics("test_filter"))
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
) -> Result<Vec<TestInvocationResult>, DiagnosticBag> {
    let mut schedule = TestAdmissionSchedule::new(plan);

    let (sender, receiver) = mpsc::channel();

    let mut active = 0;

    while !schedule.completed_all() {
        while let Some(admission) = schedule.admit_next() {
            let Some(location) = locations.get(admission.invocation().identity()).cloned() else {
                return Err(operation_diagnostics("test_host_location"));
            };

            let workspace_root = workspace_root.to_path_buf();
            let sender = sender.clone();
            let worker_admission = admission.clone();

            std::thread::spawn(move || {
                let result = run_host(&workspace_root, &worker_admission, &location);
                let _ = sender.send((worker_admission, result));
            });

            active += 1;
        }

        if active == 0 {
            return Err(operation_diagnostics("test_admission_stalled"));
        }

        let (admission, result) = receiver
            .recv()
            .map_err(|_| operation_diagnostics("test_host_completion"))?;

        active -= 1;

        schedule
            .complete(&admission, result)
            .map_err(|_| operation_diagnostics("test_schedule_completion"))?;
    }

    Ok(schedule.completed_results().cloned().collect())
}

fn run_host(
    workspace_root: &Path,
    admission: &TestAdmission,
    location: &HostLocation,
) -> TestInvocationResult {
    let result_path = result_path();

    let command = TestHostCommand::new(
        location.entry,
        admission.invocation().timeout(),
        admission.invocation().capture(),
        admission.entry().error_type().cloned(),
    );

    let result = match run_host_process(workspace_root, location, command, &result_path) {
        Ok(HostProcessCompletion::Completed) => fs::read(&result_path)
            .map_err(|_| ())
            .and_then(|bytes| read_host_result(&mut bytes.as_slice()).map_err(|_| ()))
            .map(|result| {
                result.into_invocation_result(admission.invocation().identity().clone())
            })
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
            process.stdout(NativeStdio::Inherit).stderr(NativeStdio::Inherit);
        }
        TestCapturePolicy::Captured(_) | TestCapturePolicy::Discarded => {
            process.stdout(NativeStdio::Null).stderr(NativeStdio::Null);
        }
    }

    let mut child = process.spawn().map_err(|_| ())?;
    let mut input = child.take_stdin().ok_or(())?;

    write_host_command(&mut input, &command).map_err(|_| ())?;
    input.flush().map_err(|_| ())?;
    drop(input);

    let deadline = match command.timeout() {
        bray_test_protocol::TestTimeoutPolicy::Unlimited => None,
        bray_test_protocol::TestTimeoutPolicy::Limit(limit) => {
            Some(Instant::now() + limit.duration() + CANCELLATION_GRACE_PERIOD)
        }
    };

    loop {
        if child.try_wait().map_err(|_| ())?.is_some() {
            return Ok(HostProcessCompletion::Completed);
        }

        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            child.terminate().map_err(|_| ())?;
            child.wait().map_err(|_| ())?;

            return Ok(HostProcessCompletion::ForcedTermination);
        }

        std::thread::sleep(HOST_POLL_INTERVAL);
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
    infrastructure_result_with_kind(
        admission,
        TestInfrastructureFailureKind::ForcedTermination,
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

fn result_path() -> PathBuf {
    let identity = NEXT_RESULT_FILE.fetch_add(1, Ordering::Relaxed);

    std::env::temp_dir().join(format!(
        "bray-test-{}-{identity}.result",
        std::process::id()
    ))
}

const fn capture_reservation(capture: TestCapturePolicy) -> u64 {
    match capture {
        TestCapturePolicy::Captured(limits) => limits.invocation_byte_limit(),
        TestCapturePolicy::Inherited | TestCapturePolicy::Discarded => 0,
    }
}
