use std::cell::RefCell;
use std::fs::File;
use std::io;

use bray_runtime_interface::{NativePanicCause, NativeRunOutcome, NativeRunState, NativeSourceAnchor};
use bray_source::{SourceId, SourceSpan, SourceVersion, TextRange, TextSize};
use bray_test_protocol::{
    AssertionFailure, CapturedStream, ExplicitTestFailure, TestCancellationSource,
    TestCapturePolicy, TestHostCommand, TestHostResult, TestInfrastructureFailure,
    TestInfrastructureFailureKind, TestOutcome, TestPanicCause, TestPanicReport, TestSourceAnchor,
    TestTimeoutPolicy, read_host_command, write_host_result,
};

use crate::{
    RootCancellationHandle, RootCancellationSource, RunCancellationTimer, RunOutputContext,
    RunOutputStream, RunTimeoutScheduler, with_run_output_context,
};

const RESULT_PATH_VARIABLE: &str = "BRAY_TEST_RESULT_PATH";

thread_local! {
    static TEST_SESSION: RefCell<Option<NativeTestSession>> = const { RefCell::new(None) };
}

struct NativeTestSession {
    command: TestHostCommand,
    output: RunOutputContext,
    outcome: Option<TestOutcome>,
    cancellation: Option<RootCancellationHandle>,
    timeouts: Option<RunTimeoutScheduler>,
    timer: Option<RunCancellationTimer>,
}

pub(super) fn select_entry(entry: u32) -> bool {
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

pub(super) fn with_output<T>(callback: impl FnOnce() -> T) -> T {
    let output = TEST_SESSION.with(|session| {
        session
            .borrow()
            .as_ref()
            .map(|session| session.output.clone())
    });

    match output {
        Some(output) => with_run_output_context(output, callback),
        None => callback(),
    }
}

pub(super) fn register_timeout(cancellation: RootCancellationHandle) {
    TEST_SESSION.with(|session| {
        let mut session = session.borrow_mut();

        let Some(session) = session.as_mut() else {
            return;
        };

        session.cancellation = Some(cancellation.clone());

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

pub(super) fn record_outcome(outcome: NativeRunOutcome) {
    TEST_SESSION.with(|session| {
        let mut session = session.borrow_mut();

        let Some(session) = session.as_mut() else {
            return;
        };

        session.outcome = match outcome.state() {
            state if state == NativeRunState::COMPLETED => Some(TestOutcome::Passed),
            state if state == NativeRunState::CANCELLED => Some(session.cancellation_outcome()),
            state if state == NativeRunState::PANICKED => None,
            state if state == NativeRunState::RUNTIME_FAILURE => Some(
                TestOutcome::InfrastructureFailed(TestInfrastructureFailure::new(
                    TestInfrastructureFailureKind::Host,
                    u64::try_from(outcome.payload()).ok(),
                )),
            ),
            _ => Some(TestOutcome::InfrastructureFailed(TestInfrastructureFailure::new(
                TestInfrastructureFailureKind::MissingOutcome,
                None,
            ))),
        };
    });
}

pub(super) fn record_panic(
    cause: NativePanicCause,
    source: NativeSourceAnchor,
    message: String,
) {
    TEST_SESSION.with(|session| {
        let mut session = session.borrow_mut();

        let Some(session) = session.as_mut() else {
            return;
        };

        let source = test_source(source);

        session.outcome = Some(match cause {
            cause if cause == NativePanicCause::ASSERTION => {
                let failure = if message.is_empty() {
                    AssertionFailure::without_message(source)
                } else {
                    AssertionFailure::with_message(source, message)
                };

                TestOutcome::AssertionFailure(failure)
            }
            cause if cause == NativePanicCause::EXPLICIT_TEST_FAILURE => {
                TestOutcome::ExplicitFailure(ExplicitTestFailure::new(source, message))
            }
            _ => TestOutcome::Panicked(TestPanicReport::new(
                TestPanicCause::Message,
                Some(source),
                message,
            )),
        });
    });
}

pub(super) fn record_returned_error() {
    TEST_SESSION.with(|session| {
        let mut session = session.borrow_mut();

        let Some(session) = session.as_mut() else {
            return;
        };

        session.outcome = Some(match session.command.error_type().cloned() {
            Some(error_type) => TestOutcome::ReturnedError {
                error_type,
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
            session.outcome = Some(TestOutcome::InfrastructureFailed(
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
    fn read() -> Result<Self, ()> {
        let command = read_host_command(&mut io::stdin().lock()).map_err(|_| ())?;

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
            output,
            outcome: None,
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
                TestTimeoutPolicy::Unlimited => TestOutcome::Cancelled(TestCancellationSource::Invocation),
            },
            Some(RootCancellationSource::Explicit) | None => {
                TestOutcome::Cancelled(TestCancellationSource::Invocation)
            }
        }
    }

    fn fail(&mut self, kind: TestInfrastructureFailureKind) {
        self.outcome = Some(TestOutcome::InfrastructureFailed(TestInfrastructureFailure::new(
            kind, None,
        )));
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

        let result = TestHostResult::after_cleanup(outcome, standard_output, standard_error);
        let path = std::env::var_os(RESULT_PATH_VARIABLE).ok_or(io::ErrorKind::NotFound)?;
        let mut file = File::create(path)?;

        write_host_result(&mut file, &result).map_err(|_| io::ErrorKind::InvalidData.into())
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

fn test_source(source: NativeSourceAnchor) -> TestSourceAnchor {
    TestSourceAnchor::new(
        SourceSpan::new(
            SourceId::new(source.source()),
            TextRange::new(TextSize::new(source.start()), TextSize::new(source.end())),
        ),
        SourceVersion::new(source.version()),
    )
}
