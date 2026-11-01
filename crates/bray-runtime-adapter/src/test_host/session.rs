use std::cell::RefCell;
use std::fs::File;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use bray_platform::{RunOutputContext, RunOutputStream};
use bray_runtime_abi::{
    NativePanicCause, NativeRunOutcome, NativeRunState, NativeRuntimeStatus, NativeSourceAnchor,
};
use bray_source::{SourceId, SourceSpan, SourceVersion, TextRange, TextSize};
use bray_test_protocol::{
    AssertionFailure, CapturedStream, ExplicitTestFailure, TestCancellationSource,
    TestCapturePolicy, TestHostCommand, TestHostControl, TestHostResult, TestInfrastructureFailure,
    TestInfrastructureFailureKind, TestOutcome, TestPanicCause, TestPanicReport, TestSourceAnchor,
    TestTimeoutPolicy, read_host_command, read_host_control, write_host_result,
};

use bray_runtime::{
    RootCancellationHandle, RootCancellationSource, TimerRegistration, register_run_timeout,
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
    outcome: Option<Observation>,
    suppressed: Vec<Observation>,
    capture_failed: bool,
    cancellation: Option<RootCancellationHandle>,
    timer: Option<TimerRegistration>,
}

// Callback-time history owns fallibly allocated bytes. Protocol sharing is built after cleanup.
enum Observation {
    Outcome(TestOutcome),
    Panic {
        cause: NativePanicCause,
        source: NativeSourceAnchor,
        message: String,
    },
    Error {
        identity: bray_runtime_abi::NativeTypeIdentity,
        source: NativeSourceAnchor,
    },
}

impl Observation {
    fn into_outcome(self) -> TestOutcome {
        match self {
            Self::Outcome(outcome) => outcome,
            Self::Error { identity, source } => TestOutcome::ReturnedError {
                error_type: bray_test_protocol::TestErrorTypeIdentity::from_digest(
                    identity.bytes(),
                ),
                source: test_source(source),
                formatted_value: None,
            },
            Self::Panic {
                cause,
                source,
                message,
            } => {
                let source = test_source(source);

                let panic_cause = match cause {
                    NativePanicCause::ASSERTION => TestPanicCause::Assertion,
                    NativePanicCause::RUNTIME_PANIC => TestPanicCause::RuntimePanic,
                    NativePanicCause::ALLOCATION_FAILURE => TestPanicCause::AllocationFailure,
                    NativePanicCause::ENTRY_FAILURE => TestPanicCause::EntryFailure,
                    NativePanicCause::MEMORY_LAYOUT_SIZE_OVERFLOW => {
                        TestPanicCause::MemoryLayoutSizeOverflow
                    }
                    NativePanicCause::MEMORY_LAYOUT_UNSUPPORTED_ALIGNMENT => {
                        TestPanicCause::MemoryLayoutUnsupportedAlignment
                    }
                    NativePanicCause::MESSAGE | NativePanicCause::EXPLICIT_TEST_FAILURE => {
                        TestPanicCause::Message
                    }
                    _ => panic!("native report published an unknown panic cause"),
                };

                match (cause, source) {
                    (NativePanicCause::EXPLICIT_TEST_FAILURE, source) => {
                        TestOutcome::ExplicitFailure(ExplicitTestFailure::new(source, message))
                    }
                    (NativePanicCause::ASSERTION, Some(source)) => {
                        let failure = if message.is_empty() {
                            AssertionFailure::without_message(source)
                        } else {
                            AssertionFailure::with_message(source, message)
                        };

                        TestOutcome::AssertionFailure(failure)
                    }
                    _ => TestOutcome::Panicked(TestPanicReport::new(panic_cause, source, message)),
                }
            }
        }
    }

    fn is_passed(&self) -> bool {
        matches!(self, Self::Outcome(TestOutcome::Passed))
    }
}

fn reserve_capture<T>(values: &mut Vec<T>, additional: usize) -> bool {
    #[cfg(test)]
    if tests::capture_denied() {
        return false;
    }

    values.try_reserve(additional).is_ok()
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

        match register_run_timeout(limit.duration(), &cancellation) {
            Ok(timer) => {
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
                session
                    .outcome
                    .get_or_insert(Observation::Outcome(TestOutcome::Passed));
            }
            NativeRunState::CANCELLED => {
                let cancellation = session.cancellation_outcome();

                if let Some(previous) = session.outcome.replace(Observation::Outcome(cancellation))
                    && !previous.is_passed()
                {
                    if reserve_capture(&mut session.suppressed, 1) {
                        session.suppressed.insert(0, previous);
                    } else {
                        session.capture_failed = true;
                    }
                }
            }
            NativeRunState::PANICKED => {}
            NativeRunState::RUNTIME_FAILURE => {
                session.observe(Observation::Outcome(TestOutcome::InfrastructureFailed(
                    TestInfrastructureFailure::new(
                        TestInfrastructureFailureKind::Host,
                        u64::try_from(outcome.payload()).ok(),
                    ),
                )));
            }
            _ => panic!("native entry published an unknown run state"),
        }
    });
}

pub(super) fn record_report(
    primary: &bray_runtime_abi::NativePanicPrimary,
    identity: Option<bray_runtime_abi::NativeTypeIdentity>,
) -> NativeRuntimeStatus {
    if !active() {
        return NativeRuntimeStatus::NOT_INITIALIZED;
    }

    let observation = if let Some(identity) = identity {
        Observation::Error {
            identity,
            source: primary.source(),
        }
    } else {
        let mut bytes = Vec::new();

        if !reserve_capture(&mut bytes, primary.message().len()) {
            return capture_failure(NativeRuntimeStatus::RUNTIME_FAILURE);
        }

        bytes.resize(primary.message().len(), 0);

        let status = primary.message().copy_to(0, &mut bytes);

        if !status.is_success() {
            return capture_failure(status);
        }

        Observation::Panic {
            cause: primary.cause(),
            source: primary.source(),
            message: String::from_utf8(bytes).expect("panic messages contain checked UTF-8"),
        }
    };

    TEST_SESSION.with_borrow_mut(|session| {
        session
            .as_mut()
            .expect("selected report observation retains its session")
            .observe(observation)
    })
}

fn capture_failure(status: NativeRuntimeStatus) -> NativeRuntimeStatus {
    TEST_SESSION.with_borrow_mut(|session| {
        session
            .as_mut()
            .expect("selected capture retains its session")
            .capture_failed = true;
    });

    status
}

pub(super) fn record_returned_error() -> NativeRuntimeStatus {
    TEST_SESSION.with(|session| {
        let mut session = session.borrow_mut();

        let Some(session) = session.as_mut() else {
            return NativeRuntimeStatus::NOT_INITIALIZED;
        };

        session.observe(Observation::Outcome(
            match session.command.error_type().cloned() {
                Some(error_type) => TestOutcome::ReturnedError {
                    error_type,
                    source: None,
                    formatted_value: None,
                },
                None => TestOutcome::InfrastructureFailed(TestInfrastructureFailure::new(
                    TestInfrastructureFailureKind::Protocol,
                    None,
                )),
            },
        ))
    })
}

pub(super) fn record_cleanup_failure(count: usize) {
    TEST_SESSION.with(|session| {
        let mut session = session.borrow_mut();

        let Some(session) = session.as_mut() else {
            return;
        };

        if count != 0 {
            session.observe(Observation::Outcome(TestOutcome::InfrastructureFailed(
                TestInfrastructureFailure::new(
                    TestInfrastructureFailureKind::Cleanup,
                    u64::try_from(count).ok(),
                ),
            )));
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
    fn observe(&mut self, outcome: Observation) -> NativeRuntimeStatus {
        if self.outcome.as_ref().is_none_or(Observation::is_passed) {
            self.outcome = Some(outcome);
        } else if reserve_capture(&mut self.suppressed, 1) {
            self.suppressed.push(outcome);
        } else {
            self.capture_failed = true;

            return NativeRuntimeStatus::RUNTIME_FAILURE;
        }

        NativeRuntimeStatus::SUCCESS
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
            capture_failed: false,
            cancellation: None,
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
        self.outcome = Some(Observation::Outcome(TestOutcome::InfrastructureFailed(
            TestInfrastructureFailure::new(kind, None),
        )));
    }

    fn write_result(self) -> io::Result<()> {
        let outcome = self
            .outcome
            .map(Observation::into_outcome)
            .unwrap_or_else(|| {
                TestOutcome::InfrastructureFailed(TestInfrastructureFailure::new(
                    TestInfrastructureFailureKind::MissingOutcome,
                    None,
                ))
            });

        let mut suppressed: Vec<_> = self
            .suppressed
            .into_iter()
            .map(Observation::into_outcome)
            .collect();

        let outcome = if self.capture_failed {
            if outcome != TestOutcome::Passed {
                suppressed.insert(0, outcome);
            }

            TestOutcome::InfrastructureFailed(TestInfrastructureFailure::new(
                TestInfrastructureFailureKind::Capture,
                None,
            ))
        } else {
            outcome
        };

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
        .with_suppressed(suppressed);

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

    thread_local! {
        static DENY_CAPTURE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }

    pub(super) fn capture_denied() -> bool {
        DENY_CAPTURE.get()
    }

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
            capture_failed: false,
            cancellation: None,
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

        let result = TEST_SESSION
            .take()
            .expect("fixture owner must contain the value being taken");

        assert_eq!(
            result.outcome.map(super::Observation::into_outcome),
            Some(TestOutcome::Panicked(TestPanicReport::new(
                TestPanicCause::Assertion,
                None,
                ""
            )))
        );

        assert_eq!(
            result
                .suppressed
                .into_iter()
                .map(super::Observation::into_outcome)
                .collect::<Vec<_>>(),
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

        let result = TEST_SESSION
            .take()
            .expect("fixture owner must contain the value being taken");

        assert_eq!(
            result.outcome.map(super::Observation::into_outcome),
            Some(TestOutcome::Cancelled(TestCancellationSource::Invocation))
        );

        assert_eq!(
            result
                .suppressed
                .into_iter()
                .map(super::Observation::into_outcome)
                .collect::<Vec<_>>(),
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

        let result = TEST_SESSION
            .take()
            .expect("fixture owner must contain the value being taken");

        assert!(matches!(
            result.outcome.map(super::Observation::into_outcome),
            Some(TestOutcome::Panicked(_))
        ));

        assert_eq!(
            result
                .suppressed
                .into_iter()
                .map(super::Observation::into_outcome)
                .collect::<Vec<_>>(),
            [TestOutcome::ReturnedError {
                error_type: bray_test_protocol::TestErrorTypeIdentity::from_digest([29; 32]),
                source: super::test_source(source),
                formatted_value: None,
            }]
        );
    }
    #[test]
    fn denied_capture_returns_failure_without_protocol_allocation() {
        TEST_SESSION.set(Some(session()));
        DENY_CAPTURE.set(true);

        let primary = NativePanicPrimary::new(
            NativePanicCause::MESSAGE,
            NativeSourceAnchor::unavailable(),
            NativePanicMessage::empty(),
        );

        assert_eq!(
            record_report(&primary, None),
            bray_runtime_abi::NativeRuntimeStatus::RUNTIME_FAILURE
        );

        assert!(TEST_SESSION.with_borrow(|session| {
            session
                .as_ref()
                .expect("fixture test-host session must be active")
                .capture_failed
        }));

        // A typed primary needs no callback-time string or shared allocation.
        assert!(
            record_report(
                &primary,
                Some(bray_runtime_abi::NativeTypeIdentity::new([29; 32]))
            )
            .is_success()
        );

        assert_eq!(
            record_report(
                &primary,
                Some(bray_runtime_abi::NativeTypeIdentity::new([30; 32]))
            ),
            bray_runtime_abi::NativeRuntimeStatus::RUNTIME_FAILURE
        );

        record_outcome(&NativeRunOutcome::new(NativeRunState::CANCELLED, 0));
        DENY_CAPTURE.set(false);

        let result = TEST_SESSION
            .take()
            .expect("fixture owner must contain the value being taken");

        assert!(result.capture_failed);
        assert!(result.suppressed.is_empty());

        assert_eq!(
            result.outcome.map(super::Observation::into_outcome),
            Some(TestOutcome::Cancelled(TestCancellationSource::Invocation))
        );
    }

    #[test]
    fn capture_copy_failure_returns_without_losing_the_provider_owner() {
        extern "C" fn copy(
            _: usize,
            _: usize,
            _: *mut u8,
            _: usize,
        ) -> bray_runtime_abi::NativeRuntimeStatus {
            bray_runtime_abi::NativeRuntimeStatus::RUNTIME_FAILURE
        }

        thread_local! {
            static RELEASES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
        }

        extern "C" fn release(_: usize, _: usize, _: &mut NativeRunOutcome) {
            RELEASES.set(RELEASES.get() + 1);
        }

        TEST_SESSION.set(Some(session()));
        RELEASES.set(0);

        for _ in 0..2 {
            let primary = NativePanicPrimary::new(
                NativePanicCause::MESSAGE,
                NativeSourceAnchor::unavailable(),
                NativePanicMessage::new(1, 1, Some(copy), Some(release)),
            );

            assert_eq!(
                record_report(&primary, None),
                bray_runtime_abi::NativeRuntimeStatus::RUNTIME_FAILURE
            );

            drop(primary);
        }

        assert_eq!(RELEASES.get(), 2);

        assert!(
            TEST_SESSION
                .take()
                .expect("fixture owner must contain the value being taken")
                .capture_failed
        );
    }
}
