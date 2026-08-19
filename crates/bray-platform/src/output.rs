use std::cell::RefCell;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::ThreadId;

/// Standard output stream selected inside one structured run.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RunOutputStream {
    /// Standard output.
    StandardOutput,
    /// Standard error.
    StandardError,
}

/// Immutable captured state for one completed run stream.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapturedRunStream {
    bytes: Arc<[u8]>,
    discarded_byte_count: u64,
}

impl CapturedRunStream {
    /// Returns the retained output prefix.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns whether bytes were discarded after the retained prefix.
    pub const fn truncated(&self) -> bool {
        self.discarded_byte_count != 0
    }

    /// Returns the known number of discarded bytes.
    pub const fn discarded_byte_count(&self) -> u64 {
        self.discarded_byte_count
    }
}

/// Per-run routing for standard output and standard error.
#[derive(Clone, Debug)]
pub struct RunOutputContext {
    standard_output: RunOutputDestination,
    standard_error: RunOutputDestination,
}

impl RunOutputContext {
    /// Creates independent bounded sinks for both standard streams.
    pub fn captured(per_stream_byte_limit: usize, invocation_byte_limit: usize) -> Self {
        let budget = Arc::new(Mutex::new(invocation_byte_limit));

        Self {
            standard_output: RunOutputDestination::redirected_capture(
                per_stream_byte_limit,
                Arc::clone(&budget),
            ),
            standard_error: RunOutputDestination::redirected_capture(per_stream_byte_limit, budget),
        }
    }

    /// Routes both streams to the process handles.
    pub const fn inherited() -> Self {
        Self {
            standard_output: RunOutputDestination::Inherited,
            standard_error: RunOutputDestination::Inherited,
        }
    }

    /// Discards writes to both streams.
    pub fn discarded() -> Self {
        Self {
            standard_output: RunOutputDestination::redirected_discard(),
            standard_error: RunOutputDestination::redirected_discard(),
        }
    }

    /// Captures immutable output for one stream when capture was selected.
    pub fn captured_stream(&self, stream: RunOutputStream) -> Option<CapturedRunStream> {
        self.destination(stream).snapshot()
    }

    pub(crate) fn write(&self, stream: RunOutputStream, bytes: &[u8]) -> Option<usize> {
        self.destination(stream).write(bytes)
    }

    pub(crate) const fn flush(&self, stream: RunOutputStream) -> Option<()> {
        match self.destination(stream) {
            RunOutputDestination::Inherited => None,
            RunOutputDestination::Redirected(_) => Some(()),
        }
    }

    const fn destination(&self, stream: RunOutputStream) -> &RunOutputDestination {
        match stream {
            RunOutputStream::StandardOutput => &self.standard_output,
            RunOutputStream::StandardError => &self.standard_error,
        }
    }
}

#[derive(Clone, Debug)]
enum RunOutputDestination {
    Inherited,
    Redirected(Arc<RedirectedRunOutput>),
}

impl RunOutputDestination {
    fn redirected_capture(byte_limit: usize, invocation_budget: Arc<Mutex<usize>>) -> Self {
        Self::Redirected(Arc::new(RedirectedRunOutput {
            sink: RedirectedRunOutputSink::Capture(BoundedCapture {
                byte_limit,
                invocation_budget,
                state: Mutex::new(CaptureState::default()),
            }),
            operation: RunOutputLock::new(),
        }))
    }

    fn redirected_discard() -> Self {
        Self::Redirected(Arc::new(RedirectedRunOutput {
            sink: RedirectedRunOutputSink::Discard,
            operation: RunOutputLock::new(),
        }))
    }

    fn write(&self, bytes: &[u8]) -> Option<usize> {
        match self {
            Self::Inherited => None,
            Self::Redirected(output) => Some(output.write(bytes)),
        }
    }

    fn snapshot(&self) -> Option<CapturedRunStream> {
        let Self::Redirected(output) = self else {
            return None;
        };

        output.snapshot()
    }

}

#[derive(Debug)]
struct RedirectedRunOutput {
    sink: RedirectedRunOutputSink,
    operation: RunOutputLock,
}

impl RedirectedRunOutput {
    fn write(&self, bytes: &[u8]) -> usize {
        match &self.sink {
            RedirectedRunOutputSink::Capture(capture) => capture.write(bytes),
            RedirectedRunOutputSink::Discard => {}
        }

        bytes.len()
    }

    fn snapshot(&self) -> Option<CapturedRunStream> {
        match &self.sink {
            RedirectedRunOutputSink::Capture(capture) => Some(capture.snapshot()),
            RedirectedRunOutputSink::Discard => None,
        }
    }
}

#[derive(Debug)]
enum RedirectedRunOutputSink {
    Capture(BoundedCapture),
    Discard,
}

#[derive(Debug)]
struct RunOutputLock {
    owner: Mutex<Option<ThreadId>>,
    available: Condvar,
}

impl RunOutputLock {
    const fn new() -> Self {
        Self {
            owner: Mutex::new(None),
            available: Condvar::new(),
        }
    }

    fn lock(&self) -> Result<(), RunOutputOperationError> {
        let owner = std::thread::current().id();

        let mut held_by = self
            .owner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        while let Some(current) = *held_by {
            if current == owner {
                return Err(RunOutputOperationError::Reentrant);
            }

            held_by = self
                .available
                .wait(held_by)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }

        *held_by = Some(owner);

        Ok(())
    }

    fn unlock(&self) {
        let owner = std::thread::current().id();

        let mut held_by = self
            .owner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        debug_assert_eq!(*held_by, Some(owner));

        *held_by = None;
        self.available.notify_one();
    }
}

#[derive(Debug)]
struct BoundedCapture {
    byte_limit: usize,
    invocation_budget: Arc<Mutex<usize>>,
    state: Mutex<CaptureState>,
}

impl BoundedCapture {
    fn write(&self, bytes: &[u8]) {
        let mut invocation_budget = self
            .invocation_budget
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let retained = self.byte_limit.saturating_sub(state.bytes.len());
        let retained = retained.min(*invocation_budget).min(bytes.len());

        state.bytes.extend_from_slice(&bytes[..retained]);
        *invocation_budget -= retained;

        let discarded = bytes.len() - retained;
        let discarded = u64::try_from(discarded).unwrap_or(u64::MAX);

        state.discarded_byte_count = state.discarded_byte_count.saturating_add(discarded);
    }

    fn snapshot(&self) -> CapturedRunStream {
        let state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        CapturedRunStream {
            bytes: Arc::from(state.bytes.clone()),
            discarded_byte_count: state.discarded_byte_count,
        }
    }
}

#[derive(Debug, Default)]
struct CaptureState {
    bytes: Vec<u8>,
    discarded_byte_count: u64,
}

thread_local! {
    static CURRENT_RUN_OUTPUT: RefCell<Option<RunOutputContext>> =
        const { RefCell::new(None) };
}

/// Installs per-run standard-stream routing for the duration of a callback.
pub fn with_run_output_context<T>(output: RunOutputContext, callback: impl FnOnce() -> T) -> T {
    with_optional_run_output_context(Some(output), callback)
}

/// Returns the standard-stream routing active on the current thread.
pub fn current_run_output_context() -> Option<RunOutputContext> {
    CURRENT_RUN_OUTPUT.with(|current| current.borrow().clone())
}

/// Error produced when beginning a redirected standard-stream operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RunOutputOperationError {
    /// The current thread already owns an operation on this stream.
    Reentrant,
}

/// Exact-thread guard for one redirected standard-stream operation.
pub struct RunOutputOperation {
    output: Arc<RedirectedRunOutput>,
    stream: RunOutputStream,
    exact_thread: PhantomData<Rc<()>>,
}

impl RunOutputOperation {
    /// Returns the guarded stream.
    pub const fn stream(&self) -> RunOutputStream {
        self.stream
    }

    /// Writes bytes to the guarded destination.
    pub fn write(&self, bytes: &[u8]) -> usize {
        self.output.write(bytes)
    }

    /// Flushes the guarded destination.
    pub const fn flush(&self) {}
}

impl Drop for RunOutputOperation {
    fn drop(&mut self) {
        self.output.operation.unlock();
    }
}

/// Begins one operation on the current redirected stream, or returns no guard for inherited output.
pub fn begin_current_run_output_operation(
    stream: RunOutputStream,
) -> Result<Option<RunOutputOperation>, RunOutputOperationError> {
    let output = CURRENT_RUN_OUTPUT.with(|current| {
        let current = current.borrow();
        let output = current.as_ref()?;

        match output.destination(stream) {
            RunOutputDestination::Inherited => None,
            RunOutputDestination::Redirected(output) => Some(Arc::clone(output)),
        }
    });

    let Some(output) = output else {
        return Ok(None);
    };

    output.operation.lock()?;

    Ok(Some(RunOutputOperation {
        output,
        stream,
        exact_thread: PhantomData,
    }))
}

/// Installs optional standard-stream routing for the duration of a callback.
pub fn with_optional_run_output_context<T>(
    output: Option<RunOutputContext>,
    callback: impl FnOnce() -> T,
) -> T {
    let previous = CURRENT_RUN_OUTPUT.with(|current| current.replace(output));
    let _guard = RunOutputGuard(previous);

    callback()
}

/// Writes bytes through the standard-stream routing active on the current thread.
pub fn write_current_run_output(stream: RunOutputStream, bytes: &[u8]) -> Option<usize> {
    CURRENT_RUN_OUTPUT.with(|current| {
        current
            .borrow()
            .as_ref()
            .and_then(|output| output.write(stream, bytes))
    })
}

/// Flushes the current run destination when output is redirected by the host.
pub fn flush_current_run_output(stream: RunOutputStream) -> Option<()> {
    CURRENT_RUN_OUTPUT.with(|current| {
        current
            .borrow()
            .as_ref()
            .and_then(|output| output.flush(stream))
    })
}

struct RunOutputGuard(Option<RunOutputContext>);

impl Drop for RunOutputGuard {
    fn drop(&mut self) {
        let previous = self.0.take();

        CURRENT_RUN_OUTPUT.with(|current| {
            current.replace(previous);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CapturedRunStream, RunOutputContext, RunOutputOperationError, RunOutputStream,
        with_run_output_context,
    };

    #[test]
    fn bounded_capture_retains_prefix_and_counts_every_discarded_byte() {
        let output = RunOutputContext::captured(5, 5);

        with_run_output_context(output.clone(), || {
            assert_eq!(
                super::write_current_run_output(RunOutputStream::StandardOutput, b"abc"),
                Some(3)
            );

            assert_eq!(
                super::write_current_run_output(RunOutputStream::StandardOutput, b"defgh"),
                Some(5)
            );

            assert_eq!(
                super::write_current_run_output(RunOutputStream::StandardError, b"xy"),
                Some(2)
            );
        });

        let captured = output
            .captured_stream(RunOutputStream::StandardOutput)
            .unwrap_or_else(|| panic!("captured stream must be available"));

        assert_eq!(captured.bytes(), b"abcde");
        assert!(captured.truncated());
        assert_eq!(captured.discarded_byte_count(), 3);

        let standard_error = output
            .captured_stream(RunOutputStream::StandardError)
            .unwrap_or_else(|| panic!("captured stream must be available"));

        assert_eq!(standard_error.bytes(), b"");
        assert_eq!(standard_error.discarded_byte_count(), 2);
    }

    #[test]
    fn independent_contexts_never_share_output() {
        let first = RunOutputContext::captured(16, 16);
        let second = RunOutputContext::captured(16, 16);

        with_run_output_context(first.clone(), || {
            super::write_current_run_output(RunOutputStream::StandardError, b"first");
        });

        with_run_output_context(second.clone(), || {
            super::write_current_run_output(RunOutputStream::StandardError, b"second");
        });

        assert_eq!(
            first
                .captured_stream(RunOutputStream::StandardError)
                .as_ref()
                .map(CapturedRunStream::bytes),
            Some(&b"first"[..])
        );

        assert_eq!(
            second
                .captured_stream(RunOutputStream::StandardError)
                .as_ref()
                .map(CapturedRunStream::bytes),
            Some(&b"second"[..])
        );
    }

    #[test]
    fn redirected_operations_release_on_panic() {
        let output = RunOutputContext::discarded();

        with_run_output_context(output, || {
            let panic = std::panic::catch_unwind(|| {
                let _operation = super::begin_current_run_output_operation(
                    RunOutputStream::StandardOutput,
                )
                .unwrap_or_else(|error| panic!("operation must begin: {error:?}"))
                .unwrap_or_else(|| panic!("discarded output must be redirected"));

                assert!(matches!(
                    super::begin_current_run_output_operation(RunOutputStream::StandardOutput),
                    Err(RunOutputOperationError::Reentrant)
                ));

                panic!("leave the operation through panic");
            });

            assert!(panic.is_err());

            let operation = super::begin_current_run_output_operation(
                RunOutputStream::StandardOutput,
            )
            .unwrap_or_else(|error| panic!("operation must begin after panic: {error:?}"))
            .unwrap_or_else(|| panic!("discarded output must be redirected"));

            drop(operation);
        });
    }

    #[test]
    fn independent_contexts_own_independent_operation_guards() {
        let first = RunOutputContext::captured(8, 8);
        let second = RunOutputContext::captured(8, 8);

        with_run_output_context(first, || {
            let first = super::begin_current_run_output_operation(RunOutputStream::StandardOutput)
                .unwrap_or_else(|error| panic!("first operation must begin: {error:?}"))
                .unwrap_or_else(|| panic!("first output must be redirected"));

            with_run_output_context(second, || {
                let second = super::begin_current_run_output_operation(
                    RunOutputStream::StandardOutput,
                )
                .unwrap_or_else(|error| panic!("second operation must begin: {error:?}"))
                .unwrap_or_else(|| panic!("second output must be redirected"));

                drop(second);
            });

            drop(first);
        });
    }
}
