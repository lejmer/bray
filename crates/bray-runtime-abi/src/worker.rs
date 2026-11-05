//! Borrowed native context for the Bray-owned worker lifecycle.

/// Stable allocation owned by one Bray worker pool.
#[repr(C)]
#[derive(Debug, Default)]
pub struct NativeWorkerReference {
    address: usize,
}

/// Borrows the actual OS-entry scope through its terminal cleanup.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct NativeWorkerScope {
    address: usize,
}

/// Native resources retained by a worker pool before publication.
#[repr(C)]
#[derive(Debug)]
pub struct NativeWorkerCallback {
    /// Identity of the admitted native resource owner.
    pub context: usize,
    /// Retains or releases pool context, binds, selects, drives or unbinds a worker,
    /// inspects its wait duration, installs an admission failure, or completes
    /// service shutdown. Operation 3 returns readiness and operation 6 returns
    /// the wait delay in milliseconds. Other operations return their status.
    pub call: extern "C" fn(
        usize,
        u32,
        &NativeWorkerReference,
        &NativeWorkerScope,
        u64,
        &mut crate::NativeRunOutcome,
    ) -> u32,
}

/// Specific target failure preserved across worker admission.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct NativeWorkerFailure {
    /// Allocation, native creation, attachment, or closed admission.
    pub stage: u32,
    /// Status category reported by the failed admission stage.
    pub category: u32,
    /// Original platform error code, if supplied by the target.
    pub native_code: i64,
}

#[cfg(all(test, target_pointer_width = "64"))]
mod tests {
    use super::{
        NativeWorkerCallback, NativeWorkerFailure, NativeWorkerReference, NativeWorkerScope,
    };

    #[test]
    fn worker_boundaries_match_the_source_layout() {
        assert_abi_layout!(NativeWorkerReference, size: 8, align: 8, fields: { address: 0 });
        assert_abi_layout!(NativeWorkerScope, size: 8, align: 8, fields: { address: 0 });
        assert_abi_layout!(NativeWorkerCallback, size: 16, align: 8, fields: { context: 0, call: 8 });

        assert_abi_layout!(NativeWorkerFailure, size: 16, align: 8, fields: {
            stage: 0, category: 4, native_code: 8,
        });
    }
}
