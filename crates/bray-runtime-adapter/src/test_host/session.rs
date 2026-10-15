use std::cell::RefCell;
use std::fs::File;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use bray_platform::{RunOutputContext, RunOutputStream};
use bray_runtime_abi::{NativePanicCause, NativeRunOutcome, NativeRunState, NativeSourceAnchor};
use bray_source::{SourceId, SourceSpan, SourceVersion, TextRange, TextSize};
use bray_test_protocol::{
    AssertionFailure, CapturedStream, ExplicitTestFailure, TestCancellationSource,
    TestCapturePolicy, TestHostCommand, TestHostControl, TestHostResult, TestInfrastructureFailure,
    TestInfrastructureFailureKind, TestOutcome, TestPanicCause, TestPanicReport, TestSourceAnchor,
    TestTimeoutPolicy, read_host_command, read_host_control, write_host_result,
};

use bray_runtime::{
    RootCancellationHandle, RootCancellationSource, RunCancellationTimer, RunTimeoutScheduler,
};

use bray_runtime::native::implementation::NativeHostCallbacks;

const RESULT_PATH_VARIABLE: &str = "BRAY_TEST_RESULT_PATH";

thread_local! {
    static TEST_SESSION: RefCell<Option<NativeTestSession>> = const { RefCell::new(None) };
}

struct NativeTestSession {
    command: TestHostCommand,
    command_cancellation: Arc<CommandCancellation>,
    output: RunOutputContext,
    outcome: Option<TestOutcome>,
    suppressed: Vec<TestOutcome>,
    cancellation: Option<RootCancellationHandle>,
    timeouts: Option<RunTimeoutScheduler>,
    timer: Option<RunCancellationTimer>,
}

struct CommandCancellation {
    requested: AtomicBool,
    cancellation: Mutex<Option<RootCancellationHandle>>,
}

pub(crate) fn callbacks() -> NativeHostCallbacks {
    NativeHostCallbacks::new(
        active,
        output,
        register_timeout,
        record_outcome,
        record_report,
        record_returned_error,
        record_cleanup_failure,
        finish,
    )
}

pub(crate) fn select_entry(entry: u32) -> bool {
    TEST_SESSION.with(|session| {
        let mut session = session.borrow_mut();

        if session.is_none() {
            *session = NativeTestSession::read().ok();
        }

        session
            .as_ref()
            .is_some_and(|session| session.command.entry().value() == entry)
    })
}

pub(super) fn active() -> bool {
    TEST_SESSION.with(|session| session.borrow().is_some())
}

fn output() -> Option<RunOutputContext> {
    TEST_SESSION.with(|session| {
        session
            .borrow()
            .as_ref()
            .map(|session| session.output.clone())
    })
}

pub(super) fn register_timeout(cancellation: RootCancellationHandle) {
    TEST_SESSION.with(|session| {
        let mut session = session.borrow_mut();

        let Some(session) = session.as_mut() else {
            return;
        };

        session.cancellation = Some(cancellation.clone());
        session.command_cancellation.register(cancellation.clone());

        let TestTimeoutPolicy::Limit(limit) = session.command.timeout() else {
            return;
        };

        let Ok(timeouts) = RunTimeoutScheduler::start() else {
            session.fail(TestInfrastructureFailureKind::Host);

            return;
        };

        match timeouts.register(limit.duration(), cancellation) {
            Ok(timer) => {
                session.timeouts = Some(timeouts);
                session.timer = Some(timer);
            }
            Err(_) => session.fail(TestInfrastructureFailureKind::Host),
        }
    });
}

pub(super) fn record_outcome(outcome: &NativeRunOutcome) {
    TEST_SESSION.with(|session| {
        let mut session = session.borrow_mut();

        let Some(session) = session.as_mut() else {
            return;
        };

        match outcome.state() {
            NativeRunState::COMPLETED => {
                session.outcome.get_or_insert(TestOutcome::Passed);
            }
            NativeRunState::CANCELLED => {
                let cancellation = session.cancellation_outcome();

                if let Some(previous) = session.outcome.replace(cancellation)
                    && previous != TestOutcome::Passed
                {
                    session.suppressed.insert(0, previous);
                }
            }
            NativeRunState::PANICKED => {}
            NativeRunState::RUNTIME_FAILURE => session.observe(TestOutcome::InfrastructureFailed(
                TestInfrastructureFailure::new(
                    TestInfrastructureFailureKind::Host,
                    u64::try_from(outcome.payload()).ok(),
                ),
            )),
            _ => panic!("native entry published an unknown run state"),
        }
    });
}

pub(super) fn record_report(
    primary: &bray_runtime_abi::NativePanicPrimary,
    identity: Option<bray_runtime_abi::NativeTypeIdentity>,
) {
    if let Some(identity) = identity {
        TEST_SESSION.with_borrow_mut(|session| {
            if let Some(session) = session {
                session.observe(TestOutcome::ReturnedError {
                    error_type: bray_test_protocol::TestErrorTypeIdentity::from_digest(
                        identity.bytes(),
                    ),
                    source: test_source(primary.source()),
                    formatted_value: None,
                });
            }
        });

        return;
    }

    // Capture is an explicit independent history request. Ordinary output borrows bytes in Bray.
    let mut bytes = vec![0; primary.message().len()];
    let status = primary.message().copy_to(0, &mut bytes);

    assert!(
        status.is_success(),
        "captured report retains its complete message provider"
    );

    let message = String::from_utf8(bytes).expect("panic messages contain checked UTF-8");
    let cause = primary.cause();
    let source = primary.source();

    TEST_SESSION.with(|session| {
        let mut session = session.borrow_mut();

        let Some(session) = session.as_mut() else {
            return;
        };

        let source = test_source(source);

        let panic_cause = match cause {
            cause if cause == NativePanicCause::ASSERTION => TestPanicCause::Assertion,
            cause if cause == NativePanicCause::RUNTIME_PANIC => TestPanicCause::RuntimePanic,
            cause if cause == NativePanicCause::ALLOCATION_FAILURE => {
                TestPanicCause::AllocationFailure
            }
            cause if cause == NativePanicCause::ENTRY_FAILURE => TestPanicCause::EntryFailure,
            NativePanicCause::MESSAGE | NativePanicCause::EXPLICIT_TEST_FAILURE => {
                TestPanicCause::Message
            }
            _ => panic!("native report published an unknown panic cause"),
        };

        session.observe(match (cause, source) {
            (cause, source) if cause == NativePanicCause::EXPLICIT_TEST_FAILURE => {
                TestOutcome::ExplicitFailure(ExplicitTestFailure::new(source, message))
            }
            (cause, Some(source)) if cause == NativePanicCause::ASSERTION => {
                let failure = if message.is_empty() {
                    AssertionFailure::without_message(source)
                } else {
                    AssertionFailure::with_message(source, message)
                };

                TestOutcome::AssertionFailure(failure)
            }
            _ => TestOutcome::Panicked(TestPanicReport::new(panic_cause, source, message)),
        });
    });
}

pub(super) fn record_returned_error() {
    TEST_SESSION.with(|session| {
        let mut session = session.borrow_mut();

        let Some(session) = session.as_mut() else {
            return;
        };

        session.observe(match session.command.error_type().cloned() {
            Some(error_type) => TestOutcome::ReturnedError {
                error_type,
                source: None,
                formatted_value: None,
            },
            None => TestOutcome::InfrastructureFailed(TestInfrastructureFailure::new(
                TestInfrastructureFailureKind::Protocol,
                None,
            )),
        });
    });
}

pub(super) fn record_cleanup_failure(count: usize) {
    TEST_SESSION.with(|session| {
        let mut session = session.borrow_mut();

        let Some(session) = session.as_mut() else {
            return;
        };

        if count != 0 {
            session.observe(TestOutcome::InfrastructureFailed(
                TestInfrastructureFailure::new(
                    TestInfrastructureFailureKind::Cleanup,
                    u64::try_from(count).ok(),
                ),
            ));
        }
    });
}

pub(super) fn finish() -> io::Result<()> {
    let session = TEST_SESSION.with(|session| session.take());

    let Some(session) = session else {
        return Ok(());
    };

    session.write_result()
}

impl NativeTestSession {
    fn observe(&mut self, outcome: TestOutcome) {
        if self
            .outcome
            .as_ref()
            .is_none_or(|previous| *previous == TestOutcome::Passed)
        {
            self.outcome = Some(outcome);
        } else {
            self.suppressed.push(outcome);
        }
    }

    fn read() -> Result<Self, ()> {
        let command = read_host_command(&mut io::stdin().lock()).map_err(|_| ())?;
        let command_cancellation = Arc::new(CommandCancellation::new());

        CommandCancellation::listen(command_cancellation.clone());

        let output = match command.capture() {
            TestCapturePolicy::Captured(limits) => RunOutputContext::captured(
                usize::try_from(limits.per_stream_byte_limit()).map_err(|_| ())?,
                usize::try_from(limits.invocation_byte_limit()).map_err(|_| ())?,
            ),
            TestCapturePolicy::Inherited => RunOutputContext::inherited(),
            TestCapturePolicy::Discarded => RunOutputContext::discarded(),
        };

        Ok(Self {
            command,
            command_cancellation,
            output,
            outcome: None,
            suppressed: Vec::new(),
            cancellation: None,
            timeouts: None,
            timer: None,
        })
    }

    fn cancellation_outcome(&self) -> TestOutcome {
        match self
            .cancellation
            .as_ref()
            .and_then(RootCancellationHandle::source)
        {
            Some(RootCancellationSource::Timeout) => match self.command.timeout() {
                TestTimeoutPolicy::Limit(limit) => TestOutcome::TimedOut(limit),
                TestTimeoutPolicy::Unlimited => {
                    TestOutcome::Cancelled(TestCancellationSource::Invocation)
                }
            },
            Some(RootCancellationSource::Explicit) if self.command_cancellation.requested() => {
                TestOutcome::Cancelled(TestCancellationSource::Command)
            }
            Some(RootCancellationSource::Explicit) | None => {
                TestOutcome::Cancelled(TestCancellationSource::Invocation)
            }
        }
    }

    fn fail(&mut self, kind: TestInfrastructureFailureKind) {
        self.outcome = Some(TestOutcome::InfrastructureFailed(
            TestInfrastructureFailure::new(kind, None),
        ));
    }

    fn write_result(self) -> io::Result<()> {
        let outcome = self.outcome.unwrap_or_else(|| {
            TestOutcome::InfrastructureFailed(TestInfrastructureFailure::new(
                TestInfrastructureFailureKind::MissingOutcome,
                None,
            ))
        });

        let standard_output = completed_stream(
            &self.output,
            RunOutputStream::StandardOutput,
            self.command.capture(),
        );

        let standard_error = completed_stream(
            &self.output,
            RunOutputStream::StandardError,
            self.command.capture(),
        );

        let result = TestHostResult::after_cleanup(
            self.command.id(),
            self.command.catalog_digest(),
            outcome,
            standard_output,
            standard_error,
        )
        .with_suppressed(self.suppressed);

        let path = std::env::var_os(RESULT_PATH_VARIABLE).ok_or(io::ErrorKind::NotFound)?;
        let mut file = File::create(path)?;

        write_host_result(&mut file, &result).map_err(|_| io::ErrorKind::InvalidData.into())
    }
}

impl CommandCancellation {
    fn new() -> Self {
        Self {
            requested: AtomicBool::new(false),
            cancellation: Mutex::new(None),
        }
    }

    fn listen(cancellation: Arc<Self>) {
        std::thread::spawn(move || {
            let mut input = io::stdin().lock();

            while let Ok(control) = read_host_control(&mut input) {
                match control {
                    TestHostControl::Cancel => cancellation.request(),
                }
            }
        });
    }

    fn register(&self, cancellation: RootCancellationHandle) {
        let mut registered = self
            .cancellation
            .lock()
            .unwrap_or_else(PoisonError::into_inner);

        *registered = Some(cancellation.clone());

        if self.requested() {
            cancellation.request();
        }
    }

    fn request(&self) {
        self.requested.store(true, Ordering::Release);

        let cancellation = self
            .cancellation
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();

        if let Some(cancellation) = cancellation {
            cancellation.request();
        }
    }

    fn requested(&self) -> bool {
        self.requested.load(Ordering::Acquire)
    }
}

fn completed_stream(
    output: &RunOutputContext,
    stream: RunOutputStream,
    capture: TestCapturePolicy,
) -> CapturedStream {
    match capture {
        TestCapturePolicy::Captured(_) => {
            let Some(captured) = output.captured_stream(stream) else {
                return CapturedStream::captured([], 0, None);
            };

            CapturedStream::captured(
                captured.bytes().iter().copied(),
                captured.discarded_byte_count(),
                None,
            )
        }
        TestCapturePolicy::Inherited => CapturedStream::inherited(None),
        TestCapturePolicy::Discarded => CapturedStream::discarded(),
    }
}

fn test_source(source: NativeSourceAnchor) -> Option<TestSourceAnchor> {
    source.is_available().then(|| {
        TestSourceAnchor::new(
            source.namespace(),
            SourceSpan::new(
                SourceId::new(source.source()),
                TextRange::new(TextSize::new(source.start()), TextSize::new(source.end())),
            ),
            SourceVersion::new(source.version()),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::{
        CommandCancellation, NativeTestSession, TEST_SESSION, record_outcome, record_report,
    };
    use bray_runtime_abi::{
        NativePanicCause, NativePanicMessage, NativePanicPrimary, NativeRunOutcome, NativeRunState,
        NativeSourceAnchor,
    };
    use bray_test_protocol::{
        TestCancellationSource, TestCapturePolicy, TestCatalogDigest, TestCatalogEntryId,
        TestHostCommand, TestHostCommandId, TestOutcome, TestPanicCause, TestPanicReport,
        TestTimeoutPolicy,
    };
    use std::sync::Arc;

    fn session() -> NativeTestSession {
        NativeTestSession {
            command: TestHostCommand::new(
                TestHostCommandId::from_bytes([1; 32]),
                TestCatalogDigest::from_bytes([2; 32]),
                TestCatalogEntryId::new(0),
                TestTimeoutPolicy::Unlimited,
                TestCapturePolicy::Discarded,
                None,
            ),
            command_cancellation: Arc::new(CommandCancellation::new()),
            output: bray_platform::RunOutputContext::discarded(),
            outcome: None,
            suppressed: Vec::new(),
            cancellation: None,
            timeouts: None,
            timer: None,
        }
    }

    fn observe(cause: NativePanicCause) {
        record_report(
            &NativePanicPrimary::new(
                cause,
                NativeSourceAnchor::unavailable(),
                NativePanicMessage::empty(),
            ),
            None,
        );
    }

    #[test]
    fn body_primary_survives_cleanup_and_completed_bridge_observation() {
        TEST_SESSION.set(Some(session()));
        observe(NativePanicCause::ASSERTION);
        observe(NativePanicCause::ALLOCATION_FAILURE);
        record_outcome(&NativeRunOutcome::new(NativeRunState::COMPLETED, 0));

        let result = TEST_SESSION.take().unwrap();

        assert_eq!(
            result.outcome,
            Some(TestOutcome::Panicked(TestPanicReport::new(
                TestPanicCause::Assertion,
                None,
                ""
            )))
        );

        assert_eq!(
            result.suppressed,
            [TestOutcome::Panicked(TestPanicReport::new(
                TestPanicCause::AllocationFailure,
                None,
                ""
            ))]
        );
    }

    #[test]
    fn cancellation_precedes_cleanup_observed_before_bridge_return() {
        TEST_SESSION.set(Some(session()));
        observe(NativePanicCause::ASSERTION);
        observe(NativePanicCause::ALLOCATION_FAILURE);
        record_outcome(&NativeRunOutcome::new(NativeRunState::CANCELLED, 0));

        let result = TEST_SESSION.take().unwrap();

        assert_eq!(
            result.outcome,
            Some(TestOutcome::Cancelled(TestCancellationSource::Invocation))
        );

        assert_eq!(
            result.suppressed,
            [
                TestOutcome::Panicked(TestPanicReport::new(TestPanicCause::Assertion, None, "")),
                TestOutcome::Panicked(TestPanicReport::new(
                    TestPanicCause::AllocationFailure,
                    None,
                    ""
                )),
            ]
        );
    }
    #[test]
    fn cleanup_type_does_not_come_from_the_test_entry() {
        TEST_SESSION.set(Some(session()));
        observe(NativePanicCause::ASSERTION);

        let source = NativeSourceAnchor::new([7; 32], 2, 3, 9, 11);

        record_report(
            &NativePanicPrimary::new(
                NativePanicCause::ENTRY_FAILURE,
                source,
                NativePanicMessage::empty(),
            ),
            Some(bray_runtime_abi::NativeTypeIdentity::new([29; 32])),
        );

        let result = TEST_SESSION.take().unwrap();

        assert!(matches!(result.outcome, Some(TestOutcome::Panicked(_))));

        assert_eq!(
            result.suppressed,
            [TestOutcome::ReturnedError {
                error_type: bray_test_protocol::TestErrorTypeIdentity::from_digest([29; 32]),
                source: super::test_source(source),
                formatted_value: None,
            }]
        );
    }
}
