use crate::{NativeProductHostObservation, NativeRuntimeStatus, NativeStaticFinalizer};

/// Executes the selected Bray terminal work while its caller retains the context.
pub type NativeProductCleanupBody = extern "C" fn(usize) -> NativeRuntimeStatus;

/// Outcome of entering an existing execution service for terminal work.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct NativeProductCleanupResult {
    /// Whether the service entered and invoked the body.
    pub status: NativeRuntimeStatus,
    /// Contained execution-service incidents drained before returning.
    pub incidents: usize,
}

/// Demanded execution resources used by product and exact-thread cleanup.
///
/// Bray owns admission, eligibility, ordering and the terminal state. These callbacks
/// retain only execution resources and run work selected by that state. A descriptor
/// without these consumers carries no factory and selects no execution implementation.
/// Copying this boundary record does not duplicate resource ownership; release the
/// acquired resource once after its last callback completes.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct NativeProductExecution {
    /// Retained execution-resource identity, valid until `release`.
    pub resource: usize,
    /// Enter a cleanup context and invoke the body once on successful entry.
    pub with_cleanup:
        extern "C" fn(usize, usize, NativeProductCleanupBody) -> NativeProductCleanupResult,
    /// Admit one asynchronous frame before its static owner becomes live.
    pub admit: extern "C" fn(&mut usize) -> NativeRuntimeStatus,
    /// Release unused asynchronous frame admission after failed formation.
    pub discard: extern "C" fn(usize),
    /// Drive an admitted asynchronous finalizer and drain its execution incidents.
    pub finalize: extern "C" fn(usize, NativeStaticFinalizer) -> usize,
    /// Retire this product's worker attachments outside the host lock.
    pub detach_workers: extern "C" fn(usize, usize),
    /// Release execution retention after terminal reporting.
    pub release: extern "C" fn(usize),
    /// Acquire a foreign-thread attachment using the shared Bray product state.
    pub attach: extern "C" fn(usize) -> NativeProductHostObservation,
    /// Retire a foreign-thread attachment using the shared Bray product state.
    pub detach: extern "C" fn(usize) -> NativeProductHostObservation,
    /// Close a root and drain its implicit exact-thread statics.
    pub finish_root: extern "C" fn(usize) -> NativeProductHostObservation,
}

/// Acquires only resources required by the descriptor's reachable execution consumers.
/// The argument is one when a product static requires an asynchronous finalizer.
pub type NativeProductExecutionFactory =
    extern "C" fn(u32, &mut core::mem::MaybeUninit<NativeProductExecution>) -> NativeRuntimeStatus;
