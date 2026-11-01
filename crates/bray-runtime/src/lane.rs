use bray_platform::RuntimeThreadId;
use bray_runtime_model::RuntimeCapability;

/// Placement constraint of one selected execution lane.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ExecutionLanePlacement {
    /// Work may move between compatible runtime workers.
    Migratable,
    /// Movable work is conservatively pinned to one runtime worker.
    PinnedWorker(RuntimeThreadId),
    /// Work must remain on its originating runtime thread.
    OriginThread(RuntimeThreadId),
    /// Work must run on the distinguished process main thread.
    MainThread(RuntimeThreadId),
}

/// Workload class isolated by one execution lane.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ExecutionWorkload {
    /// Cooperative work expected to suspend or complete promptly.
    Cooperative,
    /// Work permitted to block a native thread.
    Blocking,
    /// Sustained CPU-bound work.
    Compute,
    /// Work requiring both blocking and compute execution authority.
    BlockingCompute,
}

/// Exact runtime lane selected from checked state requirements.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ExecutionLane {
    placement: ExecutionLanePlacement,
    workload: ExecutionWorkload,
}

impl ExecutionLane {
    /// Creates an exact lane from its placement and workload class.
    pub const fn new(placement: ExecutionLanePlacement, workload: ExecutionWorkload) -> Self {
        Self {
            placement,
            workload,
        }
    }

    /// Returns where this lane may execute.
    pub const fn placement(self) -> ExecutionLanePlacement {
        self.placement
    }

    /// Returns this lane's workload class.
    pub const fn workload(self) -> ExecutionWorkload {
        self.workload
    }
}

/// Failure to select a lane satisfying checked frame-state requirements.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExecutionLaneSelectionError {
    /// An origin-thread dependency conflicts with the main-thread requirement.
    ConflictingAffinity,
    /// The selected runtime lacks a required lane capability.
    MissingCapability(RuntimeCapability),
}
