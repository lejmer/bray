use std::cell::RefCell;
use std::sync::{Arc, Mutex};

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
            standard_output: RunOutputDestination::captured(
                per_stream_byte_limit,
                Arc::clone(&budget),
            ),
            standard_error: RunOutputDestination::captured(per_stream_byte_limit, budget),
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
    pub const fn discarded() -> Self {
        Self {
            standard_output: RunOutputDestination::Discarded,
            standard_error: RunOutputDestination::Discarded,
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
            RunOutputDestination::Captured(_) | RunOutputDestination::Discarded => Some(()),
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
    Captured(Arc<BoundedCapture>),
    Inherited,
    Discarded,
}

impl RunOutputDestination {
    fn captured(byte_limit: usize, invocation_budget: Arc<Mutex<usize>>) -> Self {
        Self::Captured(Arc::new(BoundedCapture {
            byte_limit,
            invocation_budget,
            state: Mutex::new(CaptureState::default()),
        }))
    }

    fn write(&self, bytes: &[u8]) -> Option<usize> {
        match self {
            Self::Captured(capture) => {
                capture.write(bytes);

                Some(bytes.len())
            }
            Self::Inherited => None,
            Self::Discarded => Some(bytes.len()),
        }
    }

    fn snapshot(&self) -> Option<CapturedRunStream> {
        let Self::Captured(capture) = self else {
            return None;
        };

        Some(capture.snapshot())
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

pub(crate) fn current_run_output_context() -> Option<RunOutputContext> {
    CURRENT_RUN_OUTPUT.with(|current| current.borrow().clone())
}

pub(crate) fn with_optional_run_output_context<T>(
    output: Option<RunOutputContext>,
    callback: impl FnOnce() -> T,
) -> T {
    let previous = CURRENT_RUN_OUTPUT.with(|current| current.replace(output));
    let _guard = RunOutputGuard(previous);

    callback()
}

pub(crate) fn write_current_run_output(stream: RunOutputStream, bytes: &[u8]) -> Option<usize> {
    CURRENT_RUN_OUTPUT.with(|current| {
        current
            .borrow()
            .as_ref()
            .and_then(|output| output.write(stream, bytes))
    })
}

pub(crate) fn flush_current_run_output(stream: RunOutputStream) -> Option<()> {
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
        CapturedRunStream, RunOutputContext, RunOutputStream, with_run_output_context,
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
}
