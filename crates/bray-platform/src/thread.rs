use std::cell::Cell;
use std::marker::PhantomData;
use std::num::NonZeroU64;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread::{self, JoinHandle, Thread};

use bray_base::NonEmptySharedStr;

use crate::{PlatformError, PlatformErrorKind, PlatformOperation};

static NEXT_RUNTIME_THREAD_ID: AtomicU64 = AtomicU64::new(1);

thread_local! {
    static CURRENT_RUNTIME_THREAD: Cell<Option<RuntimeThreadId>> = const { Cell::new(None) };
}

/// Process-local identity of a native thread initialized for Bray execution.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RuntimeThreadId(NonZeroU64);

impl RuntimeThreadId {
    /// Returns the process-local numeric identity.
    pub const fn raw(self) -> u64 {
        self.0.get()
    }
}

/// Runtime state installed for the duration of one native thread callback.
#[derive(Debug, Eq, Hash, PartialEq)]
pub struct RuntimeThread {
    id: RuntimeThreadId,
    thread_bound: PhantomData<Rc<()>>,
}

impl RuntimeThread {
    /// Returns this initialized thread's process-local identity.
    pub const fn id(&self) -> RuntimeThreadId {
        self.id
    }

    /// Parks the current native thread until woken or spuriously resumed.
    pub fn park(&self) {
        thread::park();
    }
}

/// Returns the current runtime thread when called inside initialized native execution.
pub fn current_runtime_thread() -> Option<RuntimeThread> {
    CURRENT_RUNTIME_THREAD.get().map(RuntimeThread::new)
}

/// Scoped initialization of Bray runtime state on an existing native thread.
#[derive(Debug)]
pub struct RuntimeThreadScope {
    runtime: RuntimeThread,
    thread_bound: PhantomData<Rc<()>>,
}

impl RuntimeThreadScope {
    /// Initializes the current thread until this scope is dropped.
    pub fn enter() -> Result<Self, PlatformError> {
        Self::enter_with_id(next_runtime_thread_id()?)
    }

    /// Returns the runtime identity installed by this scope.
    pub const fn runtime(&self) -> &RuntimeThread {
        &self.runtime
    }

    fn enter_with_id(id: RuntimeThreadId) -> Result<Self, PlatformError> {
        let previous = CURRENT_RUNTIME_THREAD.replace(Some(id));

        if previous.is_some() {
            CURRENT_RUNTIME_THREAD.set(previous);

            return Err(PlatformError::new(
                PlatformOperation::ThreadRuntimeInitialization,
                PlatformErrorKind::RuntimeThreadAlreadyInitialized,
            ));
        }

        Ok(Self {
            runtime: RuntimeThread::new(id),
            thread_bound: PhantomData,
        })
    }
}

impl RuntimeThread {
    const fn new(id: RuntimeThreadId) -> Self {
        Self {
            id,
            thread_bound: PhantomData,
        }
    }
}

impl Drop for RuntimeThreadScope {
    fn drop(&mut self) {
        CURRENT_RUNTIME_THREAD.set(None);
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

/// Owned join authority and wake handle for one native thread.
#[derive(Debug)]
pub struct NativeThread<T> {
    thread: Thread,
    join: JoinHandle<NativeThreadOutcome<T>>,
}

impl<T: Send + 'static> NativeThread<T> {
    /// Creates an initialized native thread and transfers callback ownership to it.
    pub fn spawn(
        name: Option<NonEmptySharedStr>,
        callback: impl FnOnce(RuntimeThread) -> T + Send + 'static,
    ) -> Result<Self, PlatformError> {
        let id = next_runtime_thread_id()?;
        let mut builder = thread::Builder::new();

        if let Some(name) = name {
            builder = builder.name(name.as_str().to_owned());
        }

        let join = builder
            .spawn(move || run_initialized_thread(id, callback))
            .map_err(|error| PlatformError::from_io(PlatformOperation::ThreadSpawn, &error))?;

        // The wake authority must remain available after the join handle is moved.
        let thread = join.thread().clone();

        Ok(Self { thread, join })
    }

    /// Wakes this thread if it is parked.
    pub fn wake(&self) {
        self.thread.unpark();
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

fn next_runtime_thread_id() -> Result<RuntimeThreadId, PlatformError> {
    let id = NEXT_RUNTIME_THREAD_ID
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            current.checked_add(1)
        })
        .map_err(|_| {
            PlatformError::new(
                PlatformOperation::ThreadRuntimeInitialization,
                PlatformErrorKind::ThreadIdentityExhausted,
            )
        })?;

    NonZeroU64::new(id).map(RuntimeThreadId).ok_or_else(|| {
        PlatformError::new(
            PlatformOperation::ThreadRuntimeInitialization,
            PlatformErrorKind::ThreadIdentityExhausted,
        )
    })
}

fn run_initialized_thread<T>(
    id: RuntimeThreadId,
    callback: impl FnOnce(RuntimeThread) -> T,
) -> NativeThreadOutcome<T> {
    let Ok(_scope) = RuntimeThreadScope::enter_with_id(id) else {
        return NativeThreadOutcome::Panicked;
    };

    catch_unwind(AssertUnwindSafe(|| callback(RuntimeThread::new(id)))).map_or(
        NativeThreadOutcome::Panicked,
        NativeThreadOutcome::Completed,
    )
}

#[cfg(test)]
mod tests {
    use bray_base::NonEmptySharedStr;

    use super::{
        NativeThread, NativeThreadOutcome, RuntimeThread, RuntimeThreadScope,
        current_runtime_thread,
    };

    #[test]
    fn native_threads_install_and_remove_runtime_context() {
        let name = NonEmptySharedStr::try_new("bray-test-worker");

        let worker = NativeThread::spawn(name, |runtime| {
            assert_eq!(
                current_runtime_thread().as_ref().map(RuntimeThread::id),
                Some(runtime.id())
            );

            runtime.id()
        })
        .unwrap_or_else(|error| panic!("native thread must spawn: {error:?}"));

        let outcome = worker
            .join()
            .unwrap_or_else(|error| panic!("native thread must join: {error:?}"));

        assert!(matches!(outcome, NativeThreadOutcome::Completed(_)));
        assert_eq!(current_runtime_thread(), None);
    }

    #[test]
    fn native_thread_panics_do_not_cross_the_join_boundary() {
        let worker = NativeThread::spawn(None, |_| {
            panic!("test panic");
        })
        .unwrap_or_else(|error| panic!("native thread must spawn: {error:?}"));

        let outcome = worker
            .join()
            .unwrap_or_else(|error| panic!("native thread must join: {error:?}"));

        assert!(matches!(outcome, NativeThreadOutcome::Panicked));
    }

    #[test]
    fn existing_native_threads_use_scoped_runtime_initialization() {
        assert_eq!(current_runtime_thread(), None);

        let scope = RuntimeThreadScope::enter()
            .unwrap_or_else(|error| panic!("current thread must initialize: {error:?}"));

        assert_eq!(
            current_runtime_thread().as_ref().map(RuntimeThread::id),
            Some(scope.runtime().id())
        );

        assert!(RuntimeThreadScope::enter().is_err());

        drop(scope);

        assert_eq!(current_runtime_thread(), None);
    }
}
