use bray_platform::RuntimeThreadId;
use bray_runtime_interface::{ExecutionLaneRequirement, ProtectedFrameAffinity, RuntimeCapability};

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
    /// One state requests incompatible blocking and compute workload classes.
    ConflictingWorkloads,
    /// An origin-thread dependency conflicts with the main-thread requirement.
    ConflictingAffinity,
    /// The selected runtime lacks a required lane capability.
    MissingCapability(RuntimeCapability),
}

pub(crate) fn select_execution_lane(
    requirements: &[ExecutionLaneRequirement],
    affinity: ProtectedFrameAffinity,
    capabilities: &[RuntimeCapability],
    origin: RuntimeThreadId,
    main_thread: RuntimeThreadId,
) -> Result<ExecutionLane, ExecutionLaneSelectionError> {
    let workload = select_workload(requirements, capabilities)?;

    let requires_main_thread = requirements
        .binary_search(&ExecutionLaneRequirement::MainThread)
        .is_ok();

    let placement = match affinity {
        ProtectedFrameAffinity::MainThread => {
            require_capability(capabilities, RuntimeCapability::MainThreadLane)?;

            ExecutionLanePlacement::MainThread(main_thread)
        }
        ProtectedFrameAffinity::OriginThread if requires_main_thread => {
            if origin != main_thread {
                return Err(ExecutionLaneSelectionError::ConflictingAffinity);
            }

            require_capability(capabilities, RuntimeCapability::MainThreadLane)?;

            ExecutionLanePlacement::MainThread(main_thread)
        }
        ProtectedFrameAffinity::OriginThread => {
            require_capability(capabilities, RuntimeCapability::LocalLanes)?;

            ExecutionLanePlacement::OriginThread(origin)
        }
        ProtectedFrameAffinity::Movable if requires_main_thread => {
            require_capability(capabilities, RuntimeCapability::MainThreadLane)?;

            ExecutionLanePlacement::MainThread(main_thread)
        }
        ProtectedFrameAffinity::Movable
            if has_capability(capabilities, RuntimeCapability::MigratableLanes) =>
        {
            ExecutionLanePlacement::Migratable
        }
        ProtectedFrameAffinity::Movable => ExecutionLanePlacement::PinnedWorker(origin),
    };

    Ok(ExecutionLane::new(placement, workload))
}

fn select_workload(
    requirements: &[ExecutionLaneRequirement],
    capabilities: &[RuntimeCapability],
) -> Result<ExecutionWorkload, ExecutionLaneSelectionError> {
    let blocking = requirements
        .binary_search(&ExecutionLaneRequirement::Blocking)
        .is_ok();

    let compute = requirements
        .binary_search(&ExecutionLaneRequirement::Compute)
        .is_ok();

    if blocking && compute {
        return Err(ExecutionLaneSelectionError::ConflictingWorkloads);
    }

    if blocking {
        require_capability(capabilities, RuntimeCapability::BlockingLanes)?;

        return Ok(ExecutionWorkload::Blocking);
    }

    if compute {
        require_capability(capabilities, RuntimeCapability::ComputeLanes)?;

        return Ok(ExecutionWorkload::Compute);
    }

    require_capability(capabilities, RuntimeCapability::CooperativeExecution)?;

    Ok(ExecutionWorkload::Cooperative)
}

fn require_capability(
    capabilities: &[RuntimeCapability],
    capability: RuntimeCapability,
) -> Result<(), ExecutionLaneSelectionError> {
    if has_capability(capabilities, capability) {
        return Ok(());
    }

    Err(ExecutionLaneSelectionError::MissingCapability(capability))
}

fn has_capability(capabilities: &[RuntimeCapability], capability: RuntimeCapability) -> bool {
    capabilities.binary_search(&capability).is_ok()
}

#[cfg(test)]
mod tests {
    use bray_platform::RuntimeThreadScope;

    use super::{
        ExecutionLanePlacement, ExecutionLaneSelectionError, ExecutionWorkload,
        select_execution_lane,
    };
    use bray_runtime_interface::{
        ExecutionLaneRequirement, ProtectedFrameAffinity, RuntimeCapability,
    };

    #[test]
    fn lane_selection_preserves_workload_and_affinity_requirements() {
        let main = RuntimeThreadScope::enter()
            .unwrap_or_else(|error| panic!("main runtime thread must initialize: {error:?}"));

        let thread = main.runtime().id();

        let capabilities = [
            RuntimeCapability::CooperativeExecution,
            RuntimeCapability::LocalLanes,
            RuntimeCapability::MigratableLanes,
            RuntimeCapability::BlockingLanes,
            RuntimeCapability::ComputeLanes,
            RuntimeCapability::MainThreadLane,
        ];

        let blocking = select_execution_lane(
            &[ExecutionLaneRequirement::Blocking],
            ProtectedFrameAffinity::OriginThread,
            &capabilities,
            thread,
            thread,
        )
        .unwrap_or_else(|error| panic!("blocking lane must be available: {error:?}"));

        assert_eq!(blocking.workload(), ExecutionWorkload::Blocking);

        assert_eq!(
            blocking.placement(),
            ExecutionLanePlacement::OriginThread(thread)
        );

        let compute = select_execution_lane(
            &[ExecutionLaneRequirement::Compute],
            ProtectedFrameAffinity::Movable,
            &capabilities,
            thread,
            thread,
        )
        .unwrap_or_else(|error| panic!("compute lane must be available: {error:?}"));

        assert_eq!(compute.workload(), ExecutionWorkload::Compute);
        assert_eq!(compute.placement(), ExecutionLanePlacement::Migratable);
    }

    #[test]
    fn incompatible_or_unavailable_lanes_are_rejected() {
        let main = RuntimeThreadScope::enter()
            .unwrap_or_else(|error| panic!("main runtime thread must initialize: {error:?}"));

        let thread = main.runtime().id();

        assert_eq!(
            select_execution_lane(
                &[
                    ExecutionLaneRequirement::Blocking,
                    ExecutionLaneRequirement::Compute
                ],
                ProtectedFrameAffinity::Movable,
                &[RuntimeCapability::CooperativeExecution],
                thread,
                thread,
            ),
            Err(ExecutionLaneSelectionError::ConflictingWorkloads)
        );

        assert_eq!(
            select_execution_lane(
                &[ExecutionLaneRequirement::Blocking],
                ProtectedFrameAffinity::Movable,
                &[RuntimeCapability::CooperativeExecution],
                thread,
                thread,
            ),
            Err(ExecutionLaneSelectionError::MissingCapability(
                RuntimeCapability::BlockingLanes
            ))
        );
    }
}
