use super::model::{ArtifactKind, ArtifactReport, Observation, WorkloadReport};

pub(super) struct CandidateWinners {
    pub(super) controlled_median: Option<u64>,
    pub(super) controlled_mad: Option<u64>,
    pub(super) process_median: Option<u64>,
    pub(super) throughput: Option<u64>,
    pub(super) executable_bytes: Option<u64>,
    pub(super) compile_link_nanoseconds: Option<u64>,
    pub(super) allocation_count: Option<u64>,
    pub(super) allocated_bytes: Option<u64>,
    pub(super) copied_bytes: Option<u64>,
}

impl CandidateWinners {
    pub(super) fn for_workload(workload: &WorkloadReport) -> Self {
        Self {
            controlled_median: minimum_complete(
                std::iter::once(Some(workload.bray_execution.median_nanoseconds)).chain(
                    workload
                        .peers
                        .values()
                        .map(|peer| Some(peer.controlled_execution.median_nanoseconds)),
                ),
            ),
            controlled_mad: minimum_complete(
                std::iter::once(Some(
                    workload.bray_execution.median_absolute_deviation_nanoseconds,
                ))
                .chain(workload.peers.values().map(|peer| {
                    Some(
                        peer.controlled_execution
                            .median_absolute_deviation_nanoseconds,
                    )
                })),
            ),
            process_median: minimum_complete(
                std::iter::once(Some(workload.process_execution.median_nanoseconds)).chain(
                    workload
                        .peers
                        .values()
                        .map(|peer| Some(peer.process_execution.median_nanoseconds)),
                ),
            ),
            throughput: maximum_complete(
                std::iter::once(Some(workload.bray_execution.median_units_per_second)).chain(
                    workload
                        .peers
                        .values()
                        .map(|peer| Some(peer.controlled_execution.median_units_per_second)),
                ),
            ),
            executable_bytes: minimum_complete(
                std::iter::once(executable_bytes(&workload.artifacts)).chain(
                    workload
                        .peers
                        .values()
                        .map(|peer| executable_bytes(&peer.artifacts)),
                ),
            ),
            compile_link_nanoseconds: minimum_complete(
                std::iter::once(Some(workload.compilation.elapsed_nanoseconds)).chain(
                    workload
                        .peers
                        .values()
                        .map(|peer| Some(peer.production_compile_link_nanoseconds)),
                ),
            ),
            allocation_count: minimum_complete(
                std::iter::once(observation_value(&workload.observations.allocation_count)).chain(
                    workload
                        .peers
                        .values()
                        .map(|peer| observation_value(&peer.observations.allocation_count)),
                ),
            ),
            allocated_bytes: minimum_complete(
                std::iter::once(observation_value(&workload.observations.allocated_bytes)).chain(
                    workload
                        .peers
                        .values()
                        .map(|peer| observation_value(&peer.observations.allocated_bytes)),
                ),
            ),
            copied_bytes: minimum_complete(
                std::iter::once(observation_value(&workload.observations.copied_bytes)).chain(
                    workload
                        .peers
                        .values()
                        .map(|peer| observation_value(&peer.observations.copied_bytes)),
                ),
            ),
        }
    }
}

fn minimum_complete(values: impl Iterator<Item = Option<u64>>) -> Option<u64> {
    complete_values(values)?.into_iter().min()
}

fn maximum_complete(values: impl Iterator<Item = Option<u64>>) -> Option<u64> {
    complete_values(values)?.into_iter().max()
}

fn complete_values(values: impl Iterator<Item = Option<u64>>) -> Option<Vec<u64>> {
    let values = values.collect::<Option<Vec<_>>>()?;

    (values.len() >= 2).then_some(values)
}

pub(super) fn executable_bytes(artifacts: &[ArtifactReport]) -> Option<u64> {
    artifacts
        .iter()
        .find(|artifact| artifact.kind == ArtifactKind::Executable)
        .map(|artifact| artifact.bytes)
}

pub(super) const fn observation_value(observation: &Observation) -> Option<u64> {
    match observation {
        Observation::Measured { value, .. } => Some(*value),
        Observation::Unavailable { .. } => None,
    }
}
