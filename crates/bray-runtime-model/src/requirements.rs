/// Execution-lane predicate that reachable code requires the product to satisfy.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ExecutionLaneRequirement {
    /// Execution may perform blocking work.
    Blocking,
    /// Execution may perform CPU-bound compute work.
    Compute,
    /// Execution must occur on the distinguished process main thread.
    MainThread,
}

impl ExecutionLaneRequirement {
    /// Every execution-lane requirement in stable order.
    pub const ALL: [Self; 3] = [Self::Blocking, Self::Compute, Self::MainThread];
}

/// Runtime facility required by reachable product code.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RuntimeCapability {
    /// Compiler-generated performance observation hooks.
    PerformanceObservation,
    /// Baseline cooperative task execution.
    CooperativeExecution,
    /// Thread-local task lanes.
    LocalLanes,
    /// Task lanes that may migrate between compatible worker threads.
    MigratableLanes,
    /// Blocking execution lanes.
    BlockingLanes,
    /// CPU-bound compute execution lanes.
    ComputeLanes,
    /// Distinguished main-thread execution lane.
    MainThreadLane,
    /// Reactor-backed external event integration.
    Reactor,
}

impl RuntimeCapability {
    /// Every runtime capability in stable order.
    pub const ALL: [Self; 8] = [
        Self::PerformanceObservation,
        Self::CooperativeExecution,
        Self::LocalLanes,
        Self::MigratableLanes,
        Self::BlockingLanes,
        Self::ComputeLanes,
        Self::MainThreadLane,
        Self::Reactor,
    ];

    /// Returns this capability's stable textual name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PerformanceObservation => "performance_observation",
            Self::CooperativeExecution => "cooperative_execution",
            Self::LocalLanes => "local_lanes",
            Self::MigratableLanes => "migratable_lanes",
            Self::BlockingLanes => "blocking_lanes",
            Self::ComputeLanes => "compute_lanes",
            Self::MainThreadLane => "main_thread_lane",
            Self::Reactor => "reactor",
        }
    }

    /// Resolves one stable textual capability name.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|capability| capability.as_str() == name)
    }
}
