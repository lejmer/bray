use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub(super) const SCHEMA_REVISION: u32 = 5;
pub(super) const MAX_SAMPLE_COUNT: u32 = 10_000;
pub(super) const MAX_SECTION_COUNT: usize = 512;
pub(super) const MAX_RETAINED_INPUT_COUNT: usize = 4_096;
pub(super) const MAX_DYNAMIC_LIBRARY_COUNT: usize = 256;
pub(super) const MAX_PLATFORM_OPERATION_COUNT: usize = 32;
pub(super) const PROCESS_EXECUTION_SCOPE: &str =
    "wall-clock process execution including startup and teardown";
pub(super) const BRAY_EXECUTION_SCOPE: &str =
    "language-controlled workload execution excluding harness process startup and teardown";
pub(super) const STORAGE_OBSERVATION_SCOPE: &str =
    "generated memory work in one dedicated observed execution";

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(super) struct PerformanceReport {
    pub schema_revision: u32,
    pub identity: ReportIdentity,
    pub workloads: Vec<WorkloadReport>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(super) struct ReportIdentity {
    pub corpus_revision: u32,
    pub corpus_sha256: String,
    pub target: String,
    pub host: String,
    pub build_configuration: String,
    pub compiler_version: String,
    pub source_revision: String,
    pub llvm_version: String,
    pub runtime_linkage: RuntimeLinkage,
    pub warmup_iterations: u32,
    pub sample_iterations: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(super) struct WorkloadReport {
    pub id: String,
    pub peer_contract: String,
    pub category: WorkloadCategory,
    pub scale: u64,
    pub units: String,
    pub expected_output_sha256: String,
    pub compilation: bray_compilation::CompilationProfileReport,
    pub process_execution: ExecutionStatistics,
    pub bray_execution: ExecutionStatistics,
    pub artifacts: Vec<ArtifactReport>,
    pub observations: WorkloadObservations,
    pub peers: BTreeMap<PeerLanguage, PeerReport>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum PeerLanguage {
    Rust,
    Cpp,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(super) struct PeerReport {
    pub toolchain: String,
    pub build_configuration: PeerBuildConfiguration,
    pub source_sha256: String,
    pub production_compile_link_nanoseconds: u64,
    pub process_execution: ExecutionStatistics,
    pub controlled_execution: ExecutionStatistics,
    pub artifacts: Vec<ArtifactReport>,
    pub observations: WorkloadObservations,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(super) struct PeerBuildConfiguration {
    pub target: String,
    pub production_arguments: Vec<String>,
    pub timed_arguments: Vec<String>,
    pub linker: String,
    pub runtime_linkage: RuntimeLinkage,
    pub post_link_actions: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum RuntimeLinkage {
    StaticApplicationRuntime,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum WorkloadCategory {
    Small,
    CoreData,
    Formatting,
    Streaming,
    Concurrent,
    Filesystem,
    Process,
    Time,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(super) struct ExecutionStatistics {
    pub scope: String,
    pub samples_nanoseconds: Vec<u64>,
    pub minimum_nanoseconds: u64,
    pub median_nanoseconds: u64,
    pub median_absolute_deviation_nanoseconds: u64,
    pub maximum_nanoseconds: u64,
    pub median_units_per_second: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(super) struct ArtifactReport {
    pub kind: ArtifactKind,
    pub path: String,
    pub bytes: u64,
    pub sections: BoundedList<SectionSize>,
    pub dependencies: ArtifactDependencies,
    pub linker_map: Option<LinkerMapReport>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ArtifactKind {
    Executable,
    RelocatableObject,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(super) struct SectionSize {
    pub name: String,
    pub bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(super) struct ArtifactDependencies {
    pub static_inputs: BoundedList<RetainedInput>,
    pub dynamic_libraries: BoundedList<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(super) struct LinkerMapReport {
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(super) struct BoundedList<T> {
    pub entries: Vec<T>,
    pub omitted_count: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub(super) struct RetainedInput {
    pub artifact: String,
    pub member: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(super) struct WorkloadObservations {
    pub allocation_count: Observation,
    pub allocated_bytes: Observation,
    pub copied_bytes: Observation,
    pub platform_operations: BTreeMap<String, Observation>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub(super) enum Observation {
    Measured { value: u64, scope: String },
    Unavailable { reason: String },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(super) struct ComparisonReport {
    pub schema_revision: u32,
    pub baseline_identity: ReportIdentity,
    pub candidate_identity: ReportIdentity,
    pub workloads: Vec<WorkloadComparison>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(super) struct WorkloadComparison {
    pub id: String,
    pub process_execution: MetricComparison,
    pub bray_execution: MetricComparison,
    pub compiler_operations: BTreeMap<String, MetricComparison>,
    pub compiler_metrics: BTreeMap<String, MetricComparison>,
    pub artifacts: Vec<ArtifactComparison>,
    pub observations: ObservationComparisonReport,
    pub peers: BTreeMap<PeerLanguage, PeerComparison>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(super) struct PeerComparison {
    pub compile_link: MetricComparison,
    pub process_execution: MetricComparison,
    pub controlled_execution: MetricComparison,
    pub artifacts: Vec<ArtifactComparison>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(super) struct ArtifactComparison {
    pub kind: ArtifactKind,
    pub bytes: MetricComparison,
    pub sections: BTreeMap<String, MetricComparison>,
    pub omitted_sections: MetricComparison,
    pub added_static_inputs: Vec<RetainedInput>,
    pub removed_static_inputs: Vec<RetainedInput>,
    pub omitted_static_inputs: MetricComparison,
    pub added_dynamic_libraries: Vec<String>,
    pub removed_dynamic_libraries: Vec<String>,
    pub omitted_dynamic_libraries: MetricComparison,
    pub linker_map_bytes: Option<MetricComparison>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(super) struct ObservationComparisonReport {
    pub allocation_count: ObservationComparison,
    pub allocated_bytes: ObservationComparison,
    pub copied_bytes: ObservationComparison,
    pub platform_operations: BTreeMap<String, ObservationComparison>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub(super) enum ObservationComparison {
    Measured {
        comparison: MetricComparison,
        scope: String,
    },
    Incomparable {
        baseline: Observation,
        candidate: Observation,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(super) struct MetricComparison {
    pub baseline: u64,
    pub candidate: u64,
    pub delta: i128,
    pub delta_basis_points: Option<i128>,
    pub assessment: ChangeAssessment,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ChangeAssessment {
    Improved,
    Regressed,
    Indeterminate,
}
