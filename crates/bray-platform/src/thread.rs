use std::cell::{Cell, RefCell};
use std::marker::PhantomData;
use std::num::NonZeroU64;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread::{self, JoinHandle, Thread};

use crate::{PlatformError, PlatformErrorKind, PlatformOperation};

static NEXT_RUNTIME_THREAD_ID: AtomicU64 = AtomicU64::new(1);
static MAIN_RUNTIME_THREAD_ID: AtomicU64 = AtomicU64::new(0);

thread_local! {
    static CURRENT_RUNTIME_THREAD: Cell<Option<RuntimeThreadId>> = const { Cell::new(None) };
    static RUNTIME_THREAD_EXIT_CALLBACKS: RefCell<Option<triomphe::Arc<ThreadExitCallbacks>>> =
        const { RefCell::new(None) };
}

#[cfg(test)]
thread_local! {
    static FAIL_THREAD_STORAGE: Cell<bool> = const { Cell::new(false) };
}

type ThreadExitCallbacks = RefCell<Vec<RuntimeThreadExitCallback>>;

/// One infallible native callback owned by an exact Bray thread attachment.
pub type RuntimeThreadExitCallback = extern "C-unwind" fn();

/// Failure to admit cleanup for the current exact-thread attachment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeThreadExitRegistrationError {
    /// No active attachment accepts new cleanup callbacks.
    NotAttached,
    /// Storage for the new callback could not be reserved.
    AllocationFailed,
}

/// Registers cleanup to run in reverse order before the current attachment ends.
/// Admission failure leaves previously registered callbacks intact.
pub fn register_runtime_thread_exit_callback(
    callback: RuntimeThreadExitCallback,
) -> Result<(), RuntimeThreadExitRegistrationError> {
    let unavailable = RuntimeThreadExitRegistrationError::NotAttached;

    if current_runtime_thread().is_none() {
        return Err(unavailable);
    }

    RUNTIME_THREAD_EXIT_CALLBACKS
        .try_with(|current| {
            let callbacks = current.borrow().as_ref().cloned().ok_or(unavailable)?;
            let mut callbacks = callbacks.borrow_mut();

            callbacks
                .try_reserve(1)
                .map_err(|_| RuntimeThreadExitRegistrationError::AllocationFailed)?;

            callbacks.push(callback);

            Ok(())
        })
        .unwrap_or(Err(unavailable))
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

/// Marks the current initialized runtime thread as the distinguished process main thread.
pub fn mark_current_runtime_thread_as_main() -> bool {
    let Some(current) = current_runtime_thread() else {
        return false;
    };

    MAIN_RUNTIME_THREAD_ID
        .compare_exchange(0, current.id().raw(), Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
        || MAIN_RUNTIME_THREAD_ID.load(Ordering::Acquire) == current.id().raw()
}

/// Returns the distinguished process main thread when it has been initialized.
pub fn main_runtime_thread() -> Option<RuntimeThread> {
    NonZeroU64::new(MAIN_RUNTIME_THREAD_ID.load(Ordering::Acquire))
        .map(RuntimeThreadId)
        .map(RuntimeThread::new)
}

/// Admitted storage and identity for attaching one future runtime thread.
/// This reservation can move between threads before it is consumed.
pub struct RuntimeThreadReservation {
    id: RuntimeThreadId,
    callbacks: triomphe::UniqueArc<ThreadExitCallbacks>,
}

impl std::fmt::Debug for RuntimeThreadReservation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RuntimeThreadReservation")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

impl RuntimeThreadReservation {
    /// Reserves a future attachment without changing the current thread.
    pub fn reserve() -> Result<Self, PlatformError> {
        let callbacks = reserve_thread_callbacks()?;
        let id = next_runtime_thread_id()?;

        Ok(Self { id, callbacks })
    }

    /// Attaches the current thread without allocating, rejecting an existing attachment.
    pub fn enter(self) -> Result<RuntimeThreadScope, PlatformError> {
        ensure_thread_unattached()?;

        Ok(RuntimeThreadScope::attach(self.id, self.callbacks))
    }

    /// Reuses the current attachment or consumes the reservation without allocating.
    pub fn enter_or_reuse(self) -> RuntimeThreadEntry {
        match current_runtime_thread() {
            Some(runtime) => RuntimeThreadEntry::Current(runtime),
            None => {
                RuntimeThreadEntry::Attached(RuntimeThreadScope::attach(self.id, self.callbacks))
            }
        }
    }
}

/// Scoped initialization of Bray runtime state on an existing native thread.
#[derive(Debug)]
pub struct RuntimeThreadScope {
    runtime: RuntimeThread,
    callbacks: triomphe::Arc<ThreadExitCallbacks>,
    active: bool,
    thread_bound: PhantomData<Rc<()>>,
}

/// One callback entry on either an already initialized or newly attached runtime thread.
#[derive(Debug)]
pub enum RuntimeThreadEntry {
    /// The caller thread already belongs to Bray runtime execution.
    Current(RuntimeThread),
    /// The foreign caller thread is initialized for this entry's lifetime.
    Attached(RuntimeThreadScope),
}

impl RuntimeThreadEntry {
    /// Returns the runtime-thread identity active for this entry.
    pub const fn runtime(&self) -> &RuntimeThread {
        match self {
            Self::Current(runtime) => runtime,
            Self::Attached(scope) => scope.runtime(),
        }
    }

    /// Finishes an attached entry and returns the number of cleanup callbacks that panicked.
    pub fn finish(self) -> usize {
        match self {
            Self::Current(_) => 0,
            Self::Attached(scope) => scope.finish(),
        }
    }
}

impl RuntimeThreadScope {
    /// Initializes the current thread until this scope is dropped.
    pub fn enter() -> Result<Self, PlatformError> {
        ensure_thread_unattached()?;

        RuntimeThreadReservation::reserve()?.enter()
    }

    /// Reuses an active runtime thread or attaches the current foreign thread for this entry.
    pub fn enter_or_reuse() -> Result<RuntimeThreadEntry, PlatformError> {
        match current_runtime_thread() {
            Some(runtime) => Ok(RuntimeThreadEntry::Current(runtime)),
            None => Self::enter().map(RuntimeThreadEntry::Attached),
        }
    }

    /// Returns the runtime identity installed by this scope.
    pub const fn runtime(&self) -> &RuntimeThread {
        &self.runtime
    }

    fn attach(id: RuntimeThreadId, callbacks: triomphe::UniqueArc<ThreadExitCallbacks>) -> Self {
        let callbacks = callbacks.shareable();
        CURRENT_RUNTIME_THREAD.set(Some(id));
        RUNTIME_THREAD_EXIT_CALLBACKS.with(|current| current.replace(Some(callbacks.clone())));

        Self {
            runtime: RuntimeThread::new(id),
            callbacks,
            active: true,
            thread_bound: PhantomData,
        }
    }

    /// Finishes this attachment and returns the number of cleanup callbacks that panicked.
    pub fn finish(mut self) -> usize {
        self.finish_attachment()
    }

    fn finish_attachment(&mut self) -> usize {
        if !self.active {
            return 0;
        }

        self.active = false;

        let mut incidents = 0usize;

        // The attachment owns callbacks even when the registration TLS has already been destroyed.
        let _ = RUNTIME_THREAD_EXIT_CALLBACKS.try_with(|current| current.replace(None));
        let mut callbacks = std::mem::take(&mut *self.callbacks.borrow_mut());

        while let Some(callback) = callbacks.pop() {
            if catch_unwind(AssertUnwindSafe(|| callback())).is_err() {
                incidents = incidents.saturating_add(1);
            }
        }

        let _ = MAIN_RUNTIME_THREAD_ID.compare_exchange(
            self.runtime.id().raw(),
            0,
            Ordering::AcqRel,
            Ordering::Acquire,
        );

        CURRENT_RUNTIME_THREAD.set(None);

        incidents
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
        let _ = self.finish_attachment();
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
    /// Creates an initialized native thread and transfers callback ownership to it.
    pub fn spawn(
        name: Option<NativeThreadName>,
        callback: impl FnOnce(RuntimeThread) -> T + Send + 'static,
    ) -> Result<Self, PlatformError> {
        let reservation = RuntimeThreadReservation::reserve()?;
        let mut builder = thread::Builder::new();

        if let Some(name) = name {
            builder = builder.name(name.as_str().to_owned());
        }

        let join = builder
            .spawn(move || run_initialized_thread(reservation, callback))
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

fn ensure_thread_unattached() -> Result<(), PlatformError> {
    if current_runtime_thread().is_some() {
        return Err(PlatformError::new(
            PlatformOperation::ThreadRuntimeInitialization,
            PlatformErrorKind::RuntimeThreadAlreadyInitialized,
        ));
    }

    Ok(())
}

fn reserve_thread_callbacks() -> Result<triomphe::UniqueArc<ThreadExitCallbacks>, PlatformError> {
    let allocation_failure = PlatformError::new(
        PlatformOperation::ThreadRuntimeInitialization,
        PlatformErrorKind::Io(std::io::ErrorKind::OutOfMemory),
    );

    #[cfg(test)]
    if FAIL_THREAD_STORAGE.get() {
        return Err(allocation_failure);
    }

    triomphe::UniqueArc::try_new(RefCell::new(Vec::new())).map_err(|_| allocation_failure)
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
    reservation: RuntimeThreadReservation,
    callback: impl FnOnce(RuntimeThread) -> T,
) -> NativeThreadOutcome<T> {
    let Ok(scope) = reservation.enter() else {
        return NativeThreadOutcome::Panicked;
    };

    catch_unwind(AssertUnwindSafe(|| {
        callback(RuntimeThread::new(scope.runtime().id()))
    }))
    .map_or(
        NativeThreadOutcome::Panicked,
        NativeThreadOutcome::Completed,
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn reserved_attachment_moves_to_its_destination_and_enters_without_allocation() {
        let current = super::RuntimeThreadScope::enter().unwrap();
        let original = current.runtime().id();
        let reservation = super::RuntimeThreadReservation::reserve().unwrap();
        let reserved = reservation.id;
        assert_eq!(super::current_runtime_thread().unwrap().id(), original);

        std::thread::spawn(move || {
            assert!(super::current_runtime_thread().is_none());
            super::FAIL_THREAD_STORAGE.set(true);
            let scope = reservation.enter_or_reuse();
            super::FAIL_THREAD_STORAGE.set(false);
            assert_eq!(scope.runtime().id(), reserved);
            assert_eq!(super::current_runtime_thread().unwrap().id(), reserved);
            drop(scope);
            assert!(super::current_runtime_thread().is_none());
        })
        .join()
        .unwrap();

        assert_eq!(super::current_runtime_thread().unwrap().id(), original);
        let reservation = super::RuntimeThreadReservation::reserve().unwrap();
        super::FAIL_THREAD_STORAGE.set(true);
        let reused = reservation.enter_or_reuse();
        let failed = super::RuntimeThreadReservation::reserve();
        super::FAIL_THREAD_STORAGE.set(false);
        assert_eq!(reused.runtime().id(), original);

        assert_eq!(
            failed.unwrap_err().kind(),
            crate::PlatformErrorKind::Io(std::io::ErrorKind::OutOfMemory)
        );

        drop(reused);
        assert_eq!(super::current_runtime_thread().unwrap().id(), original);
        let duplicate = super::RuntimeThreadReservation::reserve().unwrap().enter();

        assert_eq!(
            duplicate.unwrap_err().kind(),
            crate::PlatformErrorKind::RuntimeThreadAlreadyInitialized
        );

        drop(current);
        assert!(super::current_runtime_thread().is_none());
    }

    #[test]
    fn attachment_storage_failure_precedes_identity_and_native_callback_execution() {
        use crate::{PlatformErrorKind, PlatformOperation};
        use std::io::ErrorKind;
        use std::sync::atomic::{AtomicUsize, Ordering};
        static CALLBACKS: AtomicUsize = AtomicUsize::new(0);

        super::FAIL_THREAD_STORAGE.set(true);
        let attached = super::RuntimeThreadScope::enter();
        super::FAIL_THREAD_STORAGE.set(false);
        let error = attached.unwrap_err();

        assert_eq!(
            error.operation(),
            PlatformOperation::ThreadRuntimeInitialization
        );

        assert_eq!(error.kind(), PlatformErrorKind::Io(ErrorKind::OutOfMemory));
        assert!(super::current_runtime_thread().is_none());

        let scope = super::RuntimeThreadScope::enter().unwrap();
        let identity = scope.runtime().id();
        super::FAIL_THREAD_STORAGE.set(true);
        let duplicate = super::RuntimeThreadScope::enter();
        super::FAIL_THREAD_STORAGE.set(false);

        assert_eq!(
            duplicate.unwrap_err().kind(),
            PlatformErrorKind::RuntimeThreadAlreadyInitialized
        );

        assert_eq!(super::current_runtime_thread().unwrap().id(), identity);
        drop(scope);

        super::FAIL_THREAD_STORAGE.set(true);

        let spawned = super::NativeThread::spawn(None, |_| {
            CALLBACKS.fetch_add(1, Ordering::Relaxed);
        });

        super::FAIL_THREAD_STORAGE.set(false);

        let error = match spawned {
            Ok(thread) => {
                thread.join().unwrap();

                panic!("failed admission must not spawn the callback")
            }
            Err(error) => error,
        };

        assert_eq!(error.kind(), PlatformErrorKind::Io(ErrorKind::OutOfMemory));
        assert_eq!(CALLBACKS.load(Ordering::Relaxed), 0);

        assert!(matches!(
            super::NativeThread::spawn(None, |_| 42)
                .unwrap()
                .join()
                .unwrap(),
            super::NativeThreadOutcome::Completed(42)
        ));
    }

    use std::cell::Cell;

    use super::{
        NativeThread, NativeThreadName, NativeThreadOutcome, RuntimeThread, RuntimeThreadEntry,
        RuntimeThreadExitRegistrationError, RuntimeThreadScope, current_runtime_thread,
        main_runtime_thread, mark_current_runtime_thread_as_main,
        register_runtime_thread_exit_callback,
    };

    thread_local! {
        static EXIT_ORDER: Cell<u64> = const { Cell::new(0) };
    }

    extern "C-unwind" fn first_exit() {
        append_exit(1);
    }

    extern "C-unwind" fn second_exit() {
        append_exit(2);
    }

    extern "C-unwind" fn panicking_exit() {
        panic!("test attachment cleanup incident");
    }

    fn append_exit(exit: u64) {
        EXIT_ORDER.with(|order| {
            let value = order
                .get()
                .checked_mul(10)
                .and_then(|value| value.checked_add(exit))
                .unwrap_or_else(|| panic!("test exit order must remain representable"));

            order.set(value);
        });
    }

    #[test]
    fn native_threads_install_and_remove_runtime_context() {
        let name = NativeThreadName::try_new("bray-test-worker");

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

    #[test]
    fn callback_entries_reuse_runtime_threads_and_attach_foreign_threads() {
        let attached = RuntimeThreadScope::enter_or_reuse()
            .unwrap_or_else(|error| panic!("foreign thread must attach: {error:?}"));

        assert!(matches!(attached, RuntimeThreadEntry::Attached(_)));

        let id = attached.runtime().id();

        let current = RuntimeThreadScope::enter_or_reuse()
            .unwrap_or_else(|error| panic!("runtime thread must be reused: {error:?}"));

        assert!(matches!(current, RuntimeThreadEntry::Current(_)));
        assert_eq!(current.runtime().id(), id);

        drop(current);

        assert_eq!(
            current_runtime_thread().map(|runtime| runtime.id()),
            Some(id)
        );

        drop(attached);
        assert_eq!(current_runtime_thread(), None);
    }

    #[test]
    fn attachment_exit_callbacks_run_in_reverse_order_for_each_scope() {
        EXIT_ORDER.set(0);

        let scope = RuntimeThreadScope::enter()
            .unwrap_or_else(|error| panic!("runtime thread must attach: {error:?}"));

        assert!(register_runtime_thread_exit_callback(first_exit).is_ok());
        assert!(register_runtime_thread_exit_callback(second_exit).is_ok());

        drop(scope);

        assert_eq!(EXIT_ORDER.get(), 21);

        assert_eq!(
            register_runtime_thread_exit_callback(first_exit),
            Err(RuntimeThreadExitRegistrationError::NotAttached)
        );

        let scope = RuntimeThreadScope::enter()
            .unwrap_or_else(|error| panic!("runtime thread must reattach: {error:?}"));

        assert!(register_runtime_thread_exit_callback(first_exit).is_ok());

        drop(scope);

        assert_eq!(EXIT_ORDER.get(), 211);
    }

    #[test]
    fn attachment_owns_callbacks_after_registration_tls_is_destroyed() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        static EXITS: AtomicUsize = AtomicUsize::new(0);

        extern "C-unwind" fn exit() {
            assert!(current_runtime_thread().is_some());

            assert_eq!(
                register_runtime_thread_exit_callback(exit),
                Err(RuntimeThreadExitRegistrationError::NotAttached)
            );

            EXITS.fetch_add(1, Ordering::Relaxed);
        }

        std::thread::spawn(|| {
            thread_local! {
                static RETAINED_SCOPE: std::cell::RefCell<Option<RuntimeThreadScope>> = const { std::cell::RefCell::new(None) };
            }

            RETAINED_SCOPE.with(|_| {});

            let scope = RuntimeThreadScope::enter().unwrap();

            assert!(register_runtime_thread_exit_callback(exit).is_ok());
            RETAINED_SCOPE.with(|retained| retained.replace(Some(scope)));
        }).join().unwrap();

        assert_eq!(EXITS.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn attachment_cleanup_closes_registration_before_running_callbacks() {
        extern "C-unwind" fn exit() {
            assert!(current_runtime_thread().is_some());

            assert_eq!(
                register_runtime_thread_exit_callback(exit),
                Err(RuntimeThreadExitRegistrationError::NotAttached)
            );
        }

        let scope = RuntimeThreadScope::enter().unwrap();

        assert!(register_runtime_thread_exit_callback(exit).is_ok());
        assert_eq!(scope.finish(), 0);
    }

    #[test]
    fn attachment_exit_continues_after_a_cleanup_callback_panics() {
        EXIT_ORDER.set(0);

        let scope = RuntimeThreadScope::enter()
            .unwrap_or_else(|error| panic!("runtime thread must attach: {error:?}"));

        assert!(register_runtime_thread_exit_callback(first_exit).is_ok());
        assert!(register_runtime_thread_exit_callback(panicking_exit).is_ok());
        assert!(register_runtime_thread_exit_callback(second_exit).is_ok());

        drop(scope);

        assert_eq!(EXIT_ORDER.get(), 21);
        assert_eq!(current_runtime_thread(), None);
    }

    #[test]
    fn explicit_attachment_finish_reports_cleanup_callback_panics() {
        EXIT_ORDER.set(0);

        let scope = RuntimeThreadScope::enter()
            .unwrap_or_else(|error| panic!("runtime thread must attach: {error:?}"));

        assert!(register_runtime_thread_exit_callback(first_exit).is_ok());
        assert!(register_runtime_thread_exit_callback(panicking_exit).is_ok());
        assert!(register_runtime_thread_exit_callback(second_exit).is_ok());

        assert_eq!(scope.finish(), 1);
        assert_eq!(EXIT_ORDER.get(), 21);
        assert_eq!(current_runtime_thread(), None);
    }

    #[test]
    fn main_thread_identity_is_scoped_to_its_runtime_attachment() {
        let scope = RuntimeThreadScope::enter()
            .unwrap_or_else(|error| panic!("runtime thread must attach: {error:?}"));

        assert!(mark_current_runtime_thread_as_main());

        assert_eq!(
            main_runtime_thread().map(|thread| thread.id()),
            Some(scope.runtime().id())
        );

        drop(scope);

        assert_eq!(main_runtime_thread(), None);
    }
}
