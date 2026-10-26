use std::fs;
use std::hash::Hasher;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use bray_base::StableDigestHasher;
use bray_platform::{
    NativeChildProcess, NativeExitStatus, NativePipeReader, NativePipeWriter, NativeProcessCommand,
    NativeStdio, RunOutputContext, RunOutputStream, begin_current_run_output_operation,
    with_run_output_context,
};
use bray_test_protocol::{
    CapturedStream, TestAdmission, TestCapturePolicy, TestHostCommand, TestHostCommandId,
    TestHostControl, TestHostResult, TestHostTermination, TestInfrastructureFailure, TestInfrastructureFailureKind,
    TestInvocationPlan, TestInvocationResult, TestOutcome, read_host_result,
    TestStreamFailure, TestStreamFailureKind, write_host_command, write_host_control,
};

use super::super::model::HostLocation;
use super::scheduling::COMMAND_CANCELLED;

pub(super) const HOST_POLL_INTERVAL: Duration = Duration::from_millis(5);
const CANCELLATION_GRACE_PERIOD: Duration = Duration::from_secs(1);

pub(super) fn run_host(
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

    let result = match run_host_process(workspace_root, location, &command, &result_path) {
        Ok(completion) => host_result(admission, &command, &result_path, completion),
        Err(()) => infrastructure_result(admission),
    };

    let _ = fs::remove_file(result_path);

    result
}

fn host_result(
    admission: &TestAdmission,
    command: &TestHostCommand,
    path: &Path,
    completion: HostProcessCompletion,
) -> TestInvocationResult {
    match completion {
        HostProcessCompletion::Completed { status, stdout, stderr } => terminal_result(path, command)
            .map(|result| result.into_invocation_result(admission.invocation().identity().clone()))
            .unwrap_or_else(|kind| infrastructure_result_after_exit(admission, status, kind, stdout, stderr)),
        HostProcessCompletion::ForcedTermination { stdout, stderr } => TestInvocationResult::after_cleanup(
            admission.invocation().identity().clone(),
            TestOutcome::InfrastructureFailed(TestInfrastructureFailure::new(TestInfrastructureFailureKind::ForcedTermination, None)),
            stdout,
            stderr,
        ),
    }
}

fn terminal_result(path: &Path, command: &TestHostCommand) -> Result<TestHostResult, TestInfrastructureFailureKind> {
    let mut file = fs::File::open(path).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => TestInfrastructureFailureKind::MissingOutcome,
        _ => TestInfrastructureFailureKind::Host,
    })?;

    if file.metadata().map_err(|_| TestInfrastructureFailureKind::Host)?.len() == 0 {
        return Err(TestInfrastructureFailureKind::MissingOutcome);
    }

    // The protocol reader enforces its frame budget before allocating the payload.
    let result = read_host_result(&mut file).map_err(|_| TestInfrastructureFailureKind::Protocol)?;

    if result.command_id() != command.id() || result.catalog_digest() != command.catalog_digest() {
        return Err(TestInfrastructureFailureKind::Protocol);
    }

    Ok(result)
}

fn run_host_process(
    workspace_root: &Path,
    location: &HostLocation,
    command: &TestHostCommand,
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
        TestCapturePolicy::Captured(_) => {
            process.stdout(NativeStdio::Piped).stderr(NativeStdio::Piped);
        }
        TestCapturePolicy::Discarded => {
            process.stdout(NativeStdio::Null).stderr(NativeStdio::Null);
        }
    }

    let child = process.spawn().map_err(|_| ())?;
    let mut child = ReapedChild::new(child, command.capture());
    let mut input = child.take_stdin().ok_or(())?;

    let sent = write_host_command(&mut input, command).is_ok() && input.flush().is_ok();

    let timeout_deadline = match command.timeout() {
        bray_test_protocol::TestTimeoutPolicy::Unlimited => None,
        bray_test_protocol::TestTimeoutPolicy::Limit(limit) => {
            Some(Instant::now() + limit.duration() + CANCELLATION_GRACE_PERIOD)
        }
    };

    // An early crash can close stdin before startup is written. Reap its actual
    // cause instead of replacing it with the resulting broken-pipe error.
    let mut cancellation_deadline = (!sent).then(|| Instant::now() + CANCELLATION_GRACE_PERIOD);

    loop {
        child.drain_output();

        if let Some(status) = child.try_wait()? {
            let (stdout, stderr) = child.output(command.capture());

            return Ok(HostProcessCompletion::Completed { status, stdout, stderr });
        }

        if cancellation_deadline.is_none() && COMMAND_CANCELLED.load(Ordering::Acquire) {
            let _ = write_host_control(&mut input, TestHostControl::Cancel);
            let _ = input.flush();

            cancellation_deadline = Some(Instant::now() + CANCELLATION_GRACE_PERIOD);
        }

        if timeout_deadline.is_some_and(|deadline| Instant::now() >= deadline)
            || cancellation_deadline.is_some_and(|deadline| Instant::now() >= deadline)
        {
            child.force_terminate()?;

            let (stdout, stderr) = child.output(command.capture());

            return Ok(HostProcessCompletion::ForcedTermination { stdout, stderr });
        }

        std::thread::sleep(HOST_POLL_INTERVAL);
    }
}

struct ReapedChild {
    process: NativeChildProcess,
    reaped: bool,
    capture: Option<HostCapture>,
}

impl ReapedChild {
    fn new(mut process: NativeChildProcess, policy: TestCapturePolicy) -> Self {
        let capture = match policy {
            TestCapturePolicy::Captured(limits) => Some(HostCapture {
                output: RunOutputContext::captured(
                    usize::try_from(limits.per_stream_byte_limit()).unwrap_or(usize::MAX),
                    usize::try_from(limits.invocation_byte_limit()).unwrap_or(usize::MAX),
                ),
                pipes: [
                    Some(process.take_stdout().expect("captured host owns its configured stdout pipe")),
                    Some(process.take_stderr().expect("captured host owns its configured stderr pipe")),
                ],
                failures: [None, None],
            }),
            TestCapturePolicy::Inherited | TestCapturePolicy::Discarded => None,
        };

        Self { process, reaped: false, capture }
    }

    fn drain_output(&mut self) {
        if let Some(capture) = self.capture.as_mut() {
            capture.drain();
        }
    }

    fn output(&mut self, policy: TestCapturePolicy) -> (CapturedStream, CapturedStream) {
        assert!(self.reaped, "host output is collected only after native termination");

        self.capture.take().map(HostCapture::finish)
            .unwrap_or_else(|| (empty_stream(policy), empty_stream(policy)))
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
        if !self.reaped {
            let _ = self.process.terminate();
            let _ = self.process.wait();
        }

        // Stop the host before draining its remaining queued output.
        if let Some(capture) = self.capture.take() {
            let _ = capture.finish();
        }
    }
}

struct HostCapture {
    output: RunOutputContext,
    pipes: [Option<NativePipeReader>; 2],
    failures: [Option<TestStreamFailure>; 2],
}

impl HostCapture {
    fn drain(&mut self) -> bool {
        let mut read = false;
        let mut buffer = [0_u8; 8192];

        // The collector uses the same bounded sinks as ordinary test output.
        with_run_output_context(self.output.clone(), || {
            for ((pipe, failure), stream) in self.pipes.iter_mut().zip(self.failures.iter_mut())
                .zip([RunOutputStream::StandardOutput, RunOutputStream::StandardError]) {
                let Some(reader) = pipe.as_mut() else { continue; };

                let sink = begin_current_run_output_operation(stream)
                    .expect("a host collector has no reentrant output operation")
                    .expect("a host collector has bounded output sinks");

                // Bound each polling turn so an output flood cannot postpone cancellation.
                for _ in 0..8 {
                    match reader.read_available(&mut buffer) {
                        Ok(0) => break,
                        Ok(count) => { sink.write(&buffer[..count]); read = true; }
                        Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                        Err(error) => {
                            *failure = Some(TestStreamFailure::new(TestStreamFailureKind::Platform, error.raw_os_error().map(i64::from)));
                            *pipe = None;
                            break;
                        }
                    }
                }
            }
        });

        read
    }

    fn finish(mut self) -> (CapturedStream, CapturedStream) {
        let deadline = Instant::now() + CANCELLATION_GRACE_PERIOD;

        // A descendant may inherit the write handles. Drain queued output without
        // waiting for EOF, and bound work if that descendant continues writing.
        while self.drain() {
            if Instant::now() >= deadline {
                for failure in &mut self.failures {
                    failure.get_or_insert(TestStreamFailure::new(TestStreamFailureKind::ResourceExhausted, None));
                }

                break;
            }
        }

        let [stdout, stderr] = [(RunOutputStream::StandardOutput, self.failures[0]), (RunOutputStream::StandardError, self.failures[1])]
            .map(|(stream, failure)| {
                let captured = self.output.captured_stream(stream).expect("host output has bounded capture sinks");

                CapturedStream::captured(captured.bytes().iter().copied(), captured.discarded_byte_count(), failure)
            });

        (stdout, stderr)
    }
}

#[derive(Debug)]
enum HostProcessCompletion {
    Completed { status: NativeExitStatus, stdout: CapturedStream, stderr: CapturedStream },
    ForcedTermination { stdout: CapturedStream, stderr: CapturedStream },
}

fn infrastructure_result(admission: &TestAdmission) -> TestInvocationResult {
    infrastructure_result_with_kind(admission, TestInfrastructureFailureKind::Host)
}

fn infrastructure_result_after_exit(
    admission: &TestAdmission,
    status: NativeExitStatus,
    kind: TestInfrastructureFailureKind,
    stdout: CapturedStream,
    stderr: CapturedStream,
) -> TestInvocationResult {
    let kind = if status.success() { kind } else { TestInfrastructureFailureKind::Host };
    let mut failure = TestInfrastructureFailure::new(kind, None);

    let termination = status.code().map(|code| TestHostTermination::ExitCode { code })
        .or_else(|| status.target_termination().map(|signal| TestHostTermination::Signal { signal }));

    if let Some(termination) = termination {
        failure = failure.with_host_termination(termination);
    }

    TestInvocationResult::after_cleanup(
        admission.invocation().identity().clone(),
        TestOutcome::InfrastructureFailed(failure),
        stdout,
        stderr,
    )
}

pub(super) fn command_cancellation_result(invocation: &TestInvocationPlan) -> TestInvocationResult {
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

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;

    use bray_source::{SourceId, SourceSpan, SourceVersion, TextRange, TextSize};
    use bray_symbols::{CallableExecution, ModulePathKey, SymbolName, TestExecutionConstraint, TestResultShape};
    use bray_test_protocol::{
        CapturedStream, CapturedStreamPolicy, TestAdmission, TestAdmissionSchedule, TestCaptureLimits,
        TestCapturePolicy, TestCatalog, TestCatalogDigest, TestCatalogEntryId, TestCommandReport,
        TestDeclarationPath, TestDuration, TestEntryMetadata, TestExecutionMode, TestExecutionPlan,
        TestHostCommand, TestHostCommandId, TestHostResult, TestHostTermination, TestIdentity,
        TestInfrastructureFailureKind, TestInvocationPlan, TestOutcome, TestProductReport,
        TestSelection, TestSelectionQuery, TestSelectionSummary, TestSourceAnchor, TestTimeoutPolicy,
        write_host_result,
    };

    use super::{HostLocation, host_result, run_host, run_host_process};
    use super::super::super::report::{TestBuildProvenance, render_report};
    use super::super::super::test_support::product;

    const HOST: &str = include_str!("fixtures/native_host.rs");

    fn fixture(scenario: &str) -> (tempfile::TempDir, HostLocation) {
        let directory = tempfile::tempdir().expect("native host fixture directory");
        let source = directory.path().join("host.rs");
        let executable = directory.path().join("host.exe");

        std::fs::write(&source, HOST).expect("write native host fixture");
        std::fs::write(directory.path().join("scenario"), scenario).expect("select native host behavior");

        let compiled = std::process::Command::new("rustc")
            .arg("--edition=2024").arg(&source).arg("-o").arg(&executable)
            .output().expect("compile native host fixture");

        assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));

        (directory, HostLocation {
            executable,
            entry: TestCatalogEntryId::new(0),
            catalog_digest: TestCatalogDigest::from_bytes([4; 32]),
        })
    }

    fn admission(capture: TestCapturePolicy, timeout: TestTimeoutPolicy) -> TestAdmission {
        let identity = TestIdentity::new(product(), TestDeclarationPath::new(
            ModulePathKey::try_new(["app"]).expect("fixture module path"), SymbolName::try_new("crash").expect("fixture test name"),
        ));

        let source = TestSourceAnchor::new([0; 32], SourceSpan::new(SourceId::new(0),
            TextRange::new(TextSize::ZERO, TextSize::ZERO)), SourceVersion::new(0));

        let entry = TestEntryMetadata::new(identity.clone(), source, CallableExecution::Synchronous,
            TestExecutionConstraint::Parallel, TestResultShape::Unit, None);

        let catalog = TestCatalog::try_new(product(), [entry]).expect("fixture catalog");
        let selection = TestSelection::from_catalog(&catalog, &TestSelectionQuery::default());

        let plan = TestExecutionPlan::try_new(&[selection], [TestInvocationPlan::new(identity, timeout, capture)],
            TestExecutionMode::Parallel { maximum_concurrency: NonZeroUsize::MIN }, 4096).expect("fixture execution plan");

        TestAdmissionSchedule::new(plan).admit_next().expect("fixture invocation is admitted")
    }

    fn captured_admission() -> TestAdmission {
        admission(TestCapturePolicy::Captured(TestCaptureLimits::new(1024, 2048)), TestTimeoutPolicy::Unlimited)
    }

    fn assert_reports(result: bray_test_protocol::TestInvocationResult, termination: TestHostTermination) {
        let TestOutcome::InfrastructureFailed(failure) = result.outcome() else { panic!("native crash must be infrastructure failure"); };

        assert_eq!(failure.kind(), TestInfrastructureFailureKind::Host);
        assert_eq!(failure.host_termination(), Some(termination));
        assert_eq!(result.standard_output().bytes(), b"startup stdout\n");
        assert_eq!(result.standard_error().bytes(), b"fatal host startup\n");

        let report = TestCommandReport::new(TestSelectionSummary::new(1, 1),
            [TestProductReport::new(product(), TestCatalogDigest::from_bytes([4; 32]), [result])]);

        let build = TestBuildProvenance::new(false);
        let json = render_report(&report, &build, bray_tooling::OutputFormat::Json, false, false).expect("render native JSON report");
        let json: serde_json::Value = serde_json::from_str(&json).expect("native JSON report is valid");
        let actual = &json["products"][0]["tests"][0]["outcome"]["host_termination"];

        assert_eq!(actual, &serde_json::to_value(termination).expect("native termination JSON"));

        let text = render_report(&report, &build, bray_tooling::OutputFormat::Text, false, false).expect("render native text report");

        assert!(text.contains("app.crash"), "{text}");
        assert!(text.contains("fatal host startup"), "{text}");

        let cause = match termination {
            TestHostTermination::ExitCode { code } => format!("host exited with code {code}"),
            TestHostTermination::Signal { signal } => format!("host terminated by signal {signal}"),
        };

        assert!(text.contains(&cause), "{text}");
    }

    #[test]
    fn native_exit_without_terminal_result_retains_cause_and_startup_output() {
        let (directory, location) = fixture("exit");

        let result = run_host(directory.path(), &captured_admission(), &location);

        assert_reports(result, TestHostTermination::ExitCode { code: 37 });
    }

    #[test]
    #[cfg(unix)]
    fn native_abort_and_early_stdin_close_retain_the_fatal_signal() {
        for scenario in ["abort", "abort_early"] {
            let (directory, location) = fixture(scenario);

            let result = run_host(directory.path(), &captured_admission(), &location);

            assert_reports(result, TestHostTermination::Signal { signal: 6 });
        }
    }

    #[test]
    fn missing_and_malformed_terminal_results_keep_their_specific_failure() {
        for (scenario, kind) in [("missing", TestInfrastructureFailureKind::MissingOutcome), ("malformed", TestInfrastructureFailureKind::Protocol)] {
            let (directory, location) = fixture(scenario);

            let result = run_host(directory.path(), &captured_admission(), &location);

            let TestOutcome::InfrastructureFailed(failure) = result.outcome() else { panic!("terminal result must fail"); };

            assert_eq!(failure.kind(), kind);
            assert_eq!(failure.host_termination(), Some(TestHostTermination::ExitCode { code: 0 }));
            assert_eq!(result.standard_error().bytes(), b"fatal host startup\n");
        }
    }

    #[test]
    fn concurrent_host_output_obeys_both_capture_budgets_without_blocking() {
        let (directory, location) = fixture("flood");

        let admission = admission(TestCapturePolicy::Captured(TestCaptureLimits::new(4, 6)), TestTimeoutPolicy::Unlimited);
        let result = run_host(directory.path(), &admission, &location);
        let stdout = result.standard_output();
        let stderr = result.standard_error();

        assert!(stdout.bytes().len() <= 4 && stderr.bytes().len() <= 4);
        assert_eq!(stdout.bytes().len() + stderr.bytes().len(), 6);
        assert_eq!(stdout.discarded_byte_count() + stderr.discarded_byte_count(), 262144 - 6);
    }

    #[test]
    fn uncaptured_hosts_keep_the_selected_output_policy() {
        for (capture, expected) in [(TestCapturePolicy::Inherited, CapturedStreamPolicy::Inherited), (TestCapturePolicy::Discarded, CapturedStreamPolicy::Discarded)] {
            let (directory, location) = fixture("exit");

            let result = run_host(directory.path(), &admission(capture, TestTimeoutPolicy::Unlimited), &location);

            assert_eq!(result.standard_output().policy(), expected);
            assert_eq!(result.standard_error().policy(), expected);
            assert!(result.standard_output().bytes().is_empty() && result.standard_error().bytes().is_empty());
        }
    }

    #[test]
    fn nonresponsive_host_keeps_its_forced_termination_outcome() {
        let (directory, location) = fixture("hang");

        let admission = admission(TestCapturePolicy::Captured(TestCaptureLimits::new(1024, 2048)),
            TestTimeoutPolicy::Limit(TestDuration::from_nanoseconds(1_000_000)));

        let result = run_host(directory.path(), &admission, &location);

        let TestOutcome::InfrastructureFailed(failure) = result.outcome() else { panic!("host must be forced to terminate"); };

        assert_eq!(failure.kind(), TestInfrastructureFailureKind::ForcedTermination);
        assert_eq!(failure.host_termination(), None);
    }

    #[test]
    fn terminated_host_does_not_wait_for_a_descendant_to_close_inherited_pipes() {
        let (directory, location) = fixture("descendant");

        let root = directory.path().to_path_buf();

        let (sender, receiver) = std::sync::mpsc::channel();

        let worker = std::thread::spawn(move || {
            let _ = sender.send(run_host(&root, &captured_admission(), &location));
        });

        let result = receiver.recv_timeout(std::time::Duration::from_secs(2));

        // Release the descendant even if the bounded-completion assertion fails.
        std::fs::write(directory.path().join("release"), b"release").expect("release descendant");
        worker.join().expect("host worker finishes after descendant release");

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);

        while !directory.path().join("finished").exists() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        assert!(directory.path().join("finished").exists(), "descendant completes before fixture cleanup");
        assert_reports(result.expect("host completion cannot wait for descendant EOF"), TestHostTermination::ExitCode { code: 37 });
    }

    #[test]
    fn authenticated_terminal_result_owns_output_and_rejects_other_command_identities() {
        let (directory, location) = fixture("exit");

        let admission = captured_admission();

        let command = TestHostCommand::new(TestHostCommandId::from_bytes([3; 32]), location.catalog_digest,
            location.entry, admission.invocation().timeout(), admission.invocation().capture(), None);

        let path = directory.path().join("result");

        for (id, digest, authenticated) in [
            (command.id(), command.catalog_digest(), true),
            (TestHostCommandId::from_bytes([5; 32]), command.catalog_digest(), false),
            (command.id(), TestCatalogDigest::from_bytes([6; 32]), false),
        ] {
            let result = TestHostResult::after_cleanup(id, digest, TestOutcome::Passed,
                CapturedStream::captured(b"per-test".iter().copied(), 0, None), CapturedStream::captured([], 0, None));

            let mut file = std::fs::File::create(&path).expect("fixture terminal result file");

            write_host_result(&mut file, &result).expect("write fixture terminal result");

            let completion = run_host_process(directory.path(), &location, &command, &path).expect("native host process completes");
            let result = host_result(&admission, &command, &path, completion);

            if authenticated {
                assert_eq!(result.outcome(), &TestOutcome::Passed);
                assert_eq!(result.standard_output().bytes(), b"per-test");
                assert!(result.standard_error().bytes().is_empty());
            } else {
                assert!(matches!(result.outcome(), TestOutcome::InfrastructureFailed(_)));
                assert_eq!(result.standard_error().bytes(), b"fatal host startup\n");
            }
        }
    }
}
