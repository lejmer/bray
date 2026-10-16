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

    /// Returns the sorted, deduplicated requirement set using shared immutable storage.
    pub fn set(values: impl IntoIterator<Item = Self>) -> &'static [Self] {
        let bits = values.into_iter().fold(0_u8, |bits, requirement| {
            bits | match requirement {
                Self::Blocking => 1,
                Self::Compute => 2,
                Self::MainThread => 4,
            }
        });

        match bits {
            0 => &[],
            1 => &[Self::Blocking],
            2 => &[Self::Compute],
            3 => &[Self::Blocking, Self::Compute],
            4 => &[Self::MainThread],
            5 => &[Self::Blocking, Self::MainThread],
            6 => &[Self::Compute, Self::MainThread],
            7 => &Self::ALL,
            _ => unreachable!("the closed execution requirements occupy three bits"),
        }
    }
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

#[cfg(test)]
mod tests {
    use super::ExecutionLaneRequirement;

    #[test]
    fn requirement_sets_are_sorted_deduplicated_and_shared() {
        for mask in 0_u8..8 {
            let expected: Vec<_> = ExecutionLaneRequirement::ALL
                .into_iter()
                .enumerate()
                .filter_map(|(index, value)| (mask & (1 << index) != 0).then_some(value))
                .collect();

            let values = expected
                .iter()
                .rev()
                .copied()
                .chain(expected.iter().copied());

            let actual = ExecutionLaneRequirement::set(values);

            assert_eq!(actual, expected);

            assert!(std::ptr::eq(
                actual,
                ExecutionLaneRequirement::set(expected)
            ));
        }
    }
}
