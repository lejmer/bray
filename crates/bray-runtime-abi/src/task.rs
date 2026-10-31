use crate::{
    NativeActivationDescriptor, NativeActivationExecution, NativeFrameProgress, NativeReportRecords,
    NativeTaskHandle, NativeCleanupStorageShape, NativeFrameState, NativeInactiveFrame,
    NativeRunOutcome, NativeValueCleanupCallback,
};

/// Constructs one typed cleanup frame without executing its body. The caller
/// preadmits the concrete frame shape and retains the value and collection
/// until that frame retires. Construction returns its owned inactive token.
pub type NativeValueCleanupFactory = extern "C" fn(
    value: *mut u8,
    collection: &mut NativeRunOutcome,
    frame: &mut NativeInactiveFrame,
    outcome: &mut NativeRunOutcome,
);

/// Compiler-selected lifecycle operations for one concrete observation result.
/// The creating product retains these functions and backing descriptors until
/// the observation releases its provider reference.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct NativeTaskObservationGlue {
    /// Resolves child work while retaining the represented result.
    pub quiesce: Option<NativeValueCleanupFactory>,
    /// Resolves an abandoned result, including asynchronous finalization.
    pub resolve: Option<NativeValueCleanupFactory>,
    /// Broadcasts cancellation to the result's child runs.
    pub broadcast: Option<NativeValueCleanupCallback>,
    /// Destroys the already quiescent result synchronously as abandoned.
    pub destroy: Option<NativeValueCleanupCallback>,
    /// Concrete backing required by the quiescence factory, or null if trivial.
    pub quiesce_storage: *const NativeCleanupStorageShape,
    /// Concrete backing required by the resolution factory, or null if trivial.
    pub resolve_storage: *const NativeCleanupStorageShape,
    /// Checked requirements of synchronous result disposal.
    pub requirements: NativeFrameState,
    /// Final-consumer descriptor used to acquire the helper provider's root.
    pub provider: usize,
}

/// One resident Bray task-cell reference transported through the native bridge.
///
/// Only the Bray producer initializes this carrier after retaining its cell.
/// Moving it preserves that reference; releasing it uses the Bray protocol.
/// It retains storage independently of the sole source resolution owner.
#[repr(C)]
#[derive(Debug)]
pub struct NativeTaskReference {
    address: usize,
    runtime: u64,
    handle: u64,
}

impl NativeTaskReference {
    /// Creates an empty destination for a Bray producer; it owns no cell.
    pub const fn empty() -> Self {
        Self {
            address: 0,
            runtime: 0,
            handle: 0,
        }
    }

    /// Returns the task identity without creating another resolution claim.
    pub fn task(&self) -> NativeTaskHandle {
        NativeTaskHandle::new(self.handle)
            .expect("a Bray-issued cell reference contains its admitted task handle")
    }
}

/// Read-only scheduling metadata copied from the Bray control record.
/// It grants no dispatch or terminal-result authority.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NativeTaskSnapshot {
    /// The retained activation root, or zero after retirement.
    pub root: usize,
    /// The current producer callback's frame address, or zero outside a callback.
    pub callback: usize,
    /// Admitted, ready, running, waiting, terminal, or resolved (zero through five).
    pub phase: u32,
    /// Whether the current callback may compose a child.
    pub resumable: u8,
    /// Whether frame retirement has been requested.
    pub retiring: u8,
    /// Whether all frame retirement callbacks have returned.
    pub retired: u8,
    /// Whether a dispatcher currently retains the run.
    pub dispatching: u8,
    /// The current checked continuation and lane requirements.
    pub execution: NativeActivationExecution,
}

/// Scoped native services borrowed by one Bray task dispatch.
/// Every callback and its selected context remain live until dispatch returns.
#[repr(C)]
#[derive(Debug)]
pub struct NativeTaskDispatchServices {
    /// Invokes the frame producer and contains native unwinding.
    pub invoke: extern "C" fn(
        &NativeActivationExecution,
        &NativeActivationDescriptor,
        usize,
        u32,
        usize,
        &mut NativeFrameProgress,
        &mut NativeReportRecords,
    ),
    /// Checks the active execution lane without changing the frame.
    pub ready: extern "C" fn(&NativeActivationExecution) -> u32,
    /// Queries observable cancellation (zero) or retains a terminal shield (one).
    pub control: extern "C" fn(u32) -> u32,
    /// Transfers admitted incident records and releases unused records.
    pub transfer: extern "C" fn(
        &NativeActivationExecution,
        &mut NativeReportRecords,
        &mut NativeReportRecords,
    ),
    /// Delivers a registered task's wake notification.
    pub wake: extern "C" fn(u64),
}

#[cfg(test)]
mod tests {
    use super::{NativeTaskReference, NativeTaskSnapshot, NativeTaskObservationGlue};

    #[test]
    fn observation_glue_matches_bray_metadata() {
        assert_abi_layout!(NativeTaskObservationGlue, size: 64, align: 8, fields: {
            quiesce: 0,
            resolve: 8,
            broadcast: 16,
            destroy: 24,
            quiesce_storage: 32,
            resolve_storage: 40,
            requirements: 48,
            provider: 56,
        });
    }

    #[test]
    fn task_reference_matches_bray_carrier() {
        assert_abi_layout!(NativeTaskReference, size: 24, align: 8, fields: {
            address: 0,
            runtime: 8,
            handle: 16,
        });
    }

    #[test]
    fn task_snapshot_matches_bray_metadata() {
        assert_abi_layout!(NativeTaskSnapshot, size: 88, align: 8, fields: {
            root: 0,
            callback: 8,
            phase: 16,
            resumable: 20,
            retiring: 21,
            retired: 22,
            dispatching: 23,
            execution: 24,
        });
    }

}
