use std::marker::PhantomData;
use std::num::NonZeroU64;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;
use std::sync::Arc;
use std::thread::{self, JoinHandle, Thread};

use crate::{PlatformError, PlatformErrorKind, PlatformOperation};

/// Process-local identity of a native thread initialized for Bray execution.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RuntimeThreadId(NonZeroU64);

impl RuntimeThreadId {
    /// Returns the process-local numeric identity.
    pub const fn raw(self) -> u64 {
        self.0.get()
    }
}

/// Thread-bound observation of an identity owned by the resident runtime.
#[derive(Debug, Eq, Hash, PartialEq)]
pub struct RuntimeThread {
    id: RuntimeThreadId,
    thread_bound: PhantomData<Rc<()>>,
}

impl RuntimeThread {
    /// Observes a nonzero identity supplied by a resident native runtime.
    /// The view owns no attachment and grants no initialization authority.
    pub fn from_native_identity(identity: u64) -> Option<Self> {
        NonZeroU64::new(identity)
            .map(RuntimeThreadId)
            .map(|id| Self {
                id,
                thread_bound: PhantomData,
            })
    }

    /// Returns this initialized thread's process-local identity.
    pub const fn id(&self) -> RuntimeThreadId {
        self.id
    }

    /// Parks the current native thread until woken or spuriously resumed.
    pub fn park(&self) {
        thread::park();
    }
}

/// Terminal native thread callback outcome.
#[derive(Debug)]
pub enum NativeThreadOutcome<T> {
    /// The callback returned normally.
    Completed(T),
    /// The callback unwound across its native thread boundary.
    Panicked,
}

/// Nonempty diagnostic name assigned to one native runtime thread.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NativeThreadName(Arc<str>);

impl NativeThreadName {
    /// Creates a native thread name unless the supplied name is empty.
    pub fn try_new(name: impl Into<Arc<str>>) -> Option<Self> {
        let name = name.into();

        (!name.is_empty()).then_some(Self(name))
    }

    /// Returns the native thread name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Owned join authority and wake handle for one native thread.
#[derive(Debug)]
pub struct NativeThread<T> {
    thread: Thread,
    join: JoinHandle<NativeThreadOutcome<T>>,
}

impl<T: Send + 'static> NativeThread<T> {
    /// Creates a native thread whose caller supplies any runtime attachment.
    pub fn spawn(
        name: Option<NativeThreadName>,
        callback: impl FnOnce() -> T + Send + 'static,
    ) -> Result<Self, PlatformError> {
        let mut builder = thread::Builder::new();

        if let Some(name) = name {
            builder = builder.name(name.as_str().to_owned());
        }

        let join = builder
            .spawn(move || {
                catch_unwind(AssertUnwindSafe(callback)).map_or(
                    NativeThreadOutcome::Panicked,
                    NativeThreadOutcome::Completed,
                )
            })
            .map_err(|error| PlatformError::from_io(PlatformOperation::ThreadSpawn, &error))?;

        // The wake authority must remain available after the join handle is moved.
        let thread = join.thread().clone();

        Ok(Self { thread, join })
    }

    /// Wakes this thread if it is parked.
    pub fn wake(&self) {
        self.thread.unpark();
    }

    /// Returns whether this thread has finished and can be joined without blocking.
    pub fn is_finished(&self) -> bool {
        self.join.is_finished()
    }

    /// Joins the thread and returns its callback outcome.
    pub fn join(self) -> Result<NativeThreadOutcome<T>, PlatformError> {
        self.join.join().map_err(|_| {
            PlatformError::new(
                PlatformOperation::ThreadJoin,
                PlatformErrorKind::SynchronizationPoisoned,
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{NativeThread, NativeThreadName, NativeThreadOutcome, RuntimeThread};

    #[test]
    fn native_threads_preserve_their_name_and_return_value() {
        let name = NativeThreadName::try_new("bray-test-worker");

        let worker = NativeThread::spawn(name, || {
            assert_eq!(std::thread::current().name(), Some("bray-test-worker"));

            42
        })
        .expect("native thread must spawn");

        assert!(matches!(
            worker.join().expect("native thread must join"),
            NativeThreadOutcome::Completed(42)
        ));
    }

    #[test]
    fn native_thread_panics_do_not_cross_the_join_boundary() {
        let worker =
            NativeThread::spawn(None, || panic!("test panic")).expect("native thread must spawn");

        assert!(matches!(
            worker.join().expect("native thread must join"),
            NativeThreadOutcome::Panicked
        ));
    }

    #[test]
    fn native_identity_observations_require_nonzero_values() {
        assert!(RuntimeThread::from_native_identity(0).is_none());

        assert_eq!(
            RuntimeThread::from_native_identity(42)
                .expect("nonzero fixture identity must name a native thread")
                .id()
                .raw(),
            42
        );
    }
}
