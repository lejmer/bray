/// Opaque owner of one Bray scheduler domain. Only the Bray producer initializes
/// it. The native bridge retains it through every registration and dispatch.
#[repr(C)]
#[derive(Debug, Default)]
pub struct NativeSchedulerReference {
    address: usize,
}

/// Sole native owner of a Bray deadline domain borrowing its enclosing scheduler.
#[repr(C)]
#[derive(Debug, Default)]
pub struct NativeDeadlineReference {
    address: usize,
}

/// Checked placement and workload selected by the Bray scheduler.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NativeSchedulerLane {
    /// Migratable, pinned, origin, or main (zero through three).
    pub placement: u32,
    /// Blocking and compute requirements as independent bits.
    pub workload: u32,
    /// Required attachment identity, or zero for migratable work.
    pub thread: u64,
}

/// Checked frame requirements, preserving their originating attachment.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct NativeSchedulerRequirements {
    /// Movable, origin, or main (zero through two).
    pub affinity: u32,
    /// Blocking, compute, and main requirements as independent bits.
    pub workloads: u32,
    /// Origin of the checked continuation.
    pub origin: u64,
}

/// Read-only scheduler metadata. It conveys no source result ownership.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct NativeSchedulerObservation {
    /// Native binding identity.
    pub task: u64,
    /// Generation-checked scheduler registration.
    pub registration: u64,
    /// Compatible checked execution lane.
    pub lane: NativeSchedulerLane,
    /// Idle, queued, running, or withdrawn (zero through three).
    pub phase: u32,
    /// Absent, explicit, timer, or cancellation (zero through three).
    pub cause: u32,
    /// Number of coalesced wake requests committed for this run.
    pub wakes: u64,
    /// Optional monotonic queue timestamp, or zero when unobserved.
    pub queued_at: u64,
}

#[cfg(test)]
mod tests {
    use super::{
        NativeDeadlineReference, NativeSchedulerLane, NativeSchedulerObservation, NativeSchedulerReference,
        NativeSchedulerRequirements,
    };

    #[test]
    fn scheduler_carriers_match_bray_layouts() {
        assert_abi_layout!(NativeDeadlineReference, size: 8, align: 8, fields: { address: 0 });
        assert_abi_layout!(NativeSchedulerReference, size: 8, align: 8, fields: { address: 0 });
        assert_abi_layout!(NativeSchedulerLane, size: 16, align: 8, fields: { placement: 0, workload: 4, thread: 8 });
        assert_abi_layout!(NativeSchedulerRequirements, size: 16, align: 8, fields: { affinity: 0, workloads: 4, origin: 8 });

        assert_abi_layout!(NativeSchedulerObservation, size: 56, align: 8, fields: {
            task: 0, registration: 8, lane: 16, phase: 32, cause: 36, wakes: 40, queued_at: 48
        });
    }
}
