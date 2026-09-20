use serde::{Deserialize, Serialize};

/// Current compiler profile schema revision.
pub const COMPILATION_PROFILE_SCHEMA_REVISION: u32 = 2;

/// Profiling detail requested for one compiler invocation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CompilationProfileMode {
    /// Aggregate operation timings, query behavior, and unit statistics.
    Summary,
    /// Summary data plus a bounded causal event timeline.
    Trace,
}

/// Profiling configuration for one compiler invocation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct CompilationProfileConfiguration {
    mode: CompilationProfileMode,
    trace_event_limit: usize,
}

impl CompilationProfileConfiguration {
    /// Default maximum number of detailed trace events retained by one invocation.
    pub const DEFAULT_TRACE_EVENT_LIMIT: usize = 65_536;

    /// Creates a profiling configuration for the selected detail mode.
    pub const fn new(mode: CompilationProfileMode) -> Self {
        Self {
            mode,
            trace_event_limit: Self::DEFAULT_TRACE_EVENT_LIMIT,
        }
    }

    /// Returns the selected profiling detail mode.
    pub const fn mode(self) -> CompilationProfileMode {
        self.mode
    }

    /// Returns the maximum retained detailed trace-event count.
    pub const fn trace_event_limit(self) -> usize {
        self.trace_event_limit
    }

    /// Selects the maximum number of detailed trace events retained by one invocation.
    pub const fn with_trace_event_limit(mut self, limit: usize) -> Self {
        self.trace_event_limit = limit;

        self
    }
}

/// Completion category for one profiled operation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CompilationProfileOutcome {
    /// The operation completed normally.
    Completed,
    /// The operation returned an ordinary failure.
    Failed,
    /// The operation stopped because its request was cancelled.
    Cancelled,
    /// The operation ended without publishing an outcome.
    Abandoned,
}

/// Stable category of work represented by a profiling descriptor.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CompilationProfileCategory {
    /// Compiler work performed by the current process.
    Work,
    /// Time during which work could not proceed.
    Wait,
    /// Work delegated to an external tool process.
    External,
    /// Cache behavior without an associated duration.
    Cache,
    /// A compiler unit or byte measurement.
    Measurement,
}

/// Stable aggregation applied to observations of one descriptor.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CompilationProfileAggregation {
    /// Add every observed value.
    Sum,
    /// Retain the greatest observed value.
    Maximum,
    /// Retain both the sum and maximum duration.
    SumAndMaximum,
}

/// Stable unit used by a profiling descriptor.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CompilationProfileUnit {
    /// Monotonic elapsed nanoseconds.
    Nanoseconds,
    /// A discrete item count.
    Count,
    /// An exact byte count.
    Bytes,
}

/// Stable subject category accepted by a profiling descriptor.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CompilationProfileSubjectKind {
    /// The complete compilation invocation.
    Compilation,
    /// One source unit.
    SourceUnit,
    /// One semantic unit.
    SemanticUnit,
    /// One code-generation unit.
    CodegenUnit,
    /// One selected product.
    Product,
    /// One emitted artifact.
    Artifact,
}

/// Stable invocation identity shared by every observation in one report.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CompilationProfileContext {
    /// Canonical source package identity.
    pub package: String,
    /// Canonical product identity available at the compiler boundary.
    pub product: String,
    /// Canonical target identity.
    pub target: String,
}

/// Stable subject attached to one detailed event.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CompilationProfileSubject {
    /// Subject category.
    pub kind: CompilationProfileSubjectKind,
    /// Schema-stable fingerprint of the complete private subject identity.
    pub fingerprint: String,
}

/// Descriptor metadata declared once for one operation kind.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CompilationProfileOperationDescriptor {
    /// Stable numeric descriptor identity.
    pub id: u16,
    /// Canonical operation name.
    pub name: String,
    /// Stable operation category.
    pub category: CompilationProfileCategory,
    /// Stable observation unit.
    pub unit: CompilationProfileUnit,
    /// Aggregation applied to operation observations.
    pub aggregation: CompilationProfileAggregation,
    /// Subject categories accepted by this operation.
    pub allowed_subjects: Vec<CompilationProfileSubjectKind>,
}

/// Descriptor metadata declared once for one query kind.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CompilationProfileQueryDescriptor {
    /// Stable numeric descriptor identity.
    pub id: u16,
    /// Canonical query name.
    pub name: String,
}

/// Descriptor metadata declared once for one metric kind.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CompilationProfileMetricDescriptor {
    /// Stable numeric descriptor identity.
    pub id: u16,
    /// Canonical metric name.
    pub name: String,
    /// Canonical metric unit.
    pub unit: CompilationProfileUnit,
    /// Stable metric category.
    pub category: CompilationProfileCategory,
    /// Aggregation applied to metric observations.
    pub aggregation: CompilationProfileAggregation,
    /// Subject categories accepted by this metric.
    pub allowed_subjects: Vec<CompilationProfileSubjectKind>,
}

/// Descriptor metadata referenced by sparse observations and events.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CompilationProfileDescriptorCatalog {
    /// Operation descriptors available to this compiler.
    pub operations: Vec<CompilationProfileOperationDescriptor>,
    /// Query descriptors available to this compiler.
    pub queries: Vec<CompilationProfileQueryDescriptor>,
    /// Metric descriptors available to this compiler.
    pub metrics: Vec<CompilationProfileMetricDescriptor>,
}

/// Aggregate statistics for one observed compiler operation kind.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CompilationProfileOperationStatistics {
    /// Stable numeric descriptor identity.
    pub id: u16,
    /// Number of observed operation executions.
    pub executions: u64,
    /// Number of normally completed executions.
    pub completed: u64,
    /// Number of failed executions.
    pub failed: u64,
    /// Number of cancelled executions.
    pub cancelled: u64,
    /// Number of abandoned executions.
    pub abandoned: u64,
    /// Sum of observed execution durations.
    pub total_nanoseconds: u64,
    /// Same-thread time excluding nested profiled operations.
    pub self_nanoseconds: u64,
    /// Longest observed execution duration.
    pub maximum_nanoseconds: u64,
    /// Greatest number of compiler workers executing this operation concurrently.
    pub maximum_active_workers: u64,
}

/// Bounded latency distribution derived from a fixed logarithmic histogram.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct CompilationProfileDurationDistribution {
    /// Number of observed durations.
    pub samples: u64,
    /// Shortest observed duration.
    pub minimum_nanoseconds: u64,
    /// Upper bound of the histogram bucket containing the median observation.
    pub median_upper_bound_nanoseconds: u64,
    /// Upper bound of the histogram bucket containing the ninety-fifth percentile observation.
    pub p95_upper_bound_nanoseconds: u64,
    /// Longest observed duration.
    pub maximum_nanoseconds: u64,
}

/// Bounded count distribution derived from a fixed logarithmic histogram.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct CompilationProfileCountDistribution {
    /// Number of observed values.
    pub samples: u64,
    /// Smallest observed value.
    pub minimum: u64,
    /// Upper bound of the histogram bucket containing the median observation.
    pub median_upper_bound: u64,
    /// Upper bound of the histogram bucket containing the ninety-fifth percentile observation.
    pub p95_upper_bound: u64,
    /// Greatest observed value.
    pub maximum: u64,
}

/// Aggregate behavior for one observed compiler query kind.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CompilationProfileQueryStatistics {
    /// Stable numeric query-kind identity.
    pub id: u16,
    /// Number of requests for this query kind.
    pub requests: u64,
    /// Number of requests served from an already-published value.
    pub cache_hits: u64,
    /// Number of requests that found no published value.
    pub cache_misses: u64,
    /// Number of published values retained from a preceding snapshot.
    pub cross_snapshot_reuses: u64,
    /// Number of preceding-snapshot values invalidated before reuse.
    pub invalidations: u64,
    /// Number of independently executed evaluations.
    pub evaluations: u64,
    /// Number of waits for an evaluation owned by another task.
    pub waits: u64,
    /// Time spent evaluating this query kind.
    pub evaluation_nanoseconds: u64,
    /// Same-thread evaluation time excluding nested profiled operations.
    pub evaluation_self_nanoseconds: u64,
    /// Distribution of individual evaluation durations.
    pub evaluation_latency: CompilationProfileDurationDistribution,
    /// Time spent waiting for this query kind.
    pub wait_nanoseconds: u64,
    /// Time spent reaching an already-published value after a query request began.
    pub ready_value_nanoseconds: u64,
    /// Longest time spent reaching one already-published value.
    pub ready_value_maximum_nanoseconds: u64,
    /// Number of immutable values published by this query kind.
    pub published_values: u64,
    /// Inline bytes occupied by published fact-cell values.
    pub published_inline_bytes: u64,
    /// Number of query-result diagnostic collections observed at publication.
    pub diagnostic_collections: u64,
    /// Number of diagnostic instances attached to computed results where the query exposes them.
    pub result_diagnostics: u64,
    /// Number of query-result ownership handles copied for callers.
    pub cloned_values: u64,
    /// Inline bytes occupied by copied query-result ownership handles.
    pub cloned_inline_bytes: u64,
    /// Number of query-result diagnostic collections copied for callers.
    pub diagnostic_copies: u64,
    /// Number of diagnostics carried by copied query results.
    pub cloned_diagnostics: u64,
    /// Number of diagnostic merge boundaries attributed to this query kind.
    pub diagnostic_merges: u64,
    /// Number of diagnostics supplied across attributed merge boundaries.
    pub merged_diagnostics: u64,
}

/// Aggregate scheduling behavior for one compiler invocation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CompilationProfileSchedulerStatistics {
    /// Configured compiler worker budget.
    pub worker_budget: u64,
    /// Sum of time during which scheduler slots executed compiler work.
    pub active_worker_nanoseconds: u64,
    /// Greatest number of scheduler slots executing concurrently.
    pub maximum_active_workers: u64,
    /// Number of explicitly scheduled ready-work waves.
    pub ready_waves: u64,
    /// Number of work items submitted across ready-work waves.
    pub ready_items: u64,
    /// Greatest number of items exposed by one ready-work wave.
    pub maximum_ready_width: u64,
    /// Bounded scheduling observations grouped by their active query or compiler phase.
    pub wave_classes: Vec<CompilationProfileSchedulingWaveStatistics>,
    /// Longest observed rooted query evaluation, including its required dependencies.
    pub query_critical_path_nanoseconds: u64,
}

impl Default for CompilationProfileSchedulerStatistics {
    fn default() -> Self {
        Self {
            worker_budget: 1,
            active_worker_nanoseconds: 0,
            maximum_active_workers: 0,
            ready_waves: 0,
            ready_items: 0,
            maximum_ready_width: 0,
            wave_classes: Vec::new(),
            query_critical_path_nanoseconds: 0,
        }
    }
}

/// Scheduling waves attributed to one active query or compiler phase.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CompilationProfileSchedulingWaveStatistics {
    /// Active operation descriptor for phase-owned work.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<u16>,
    /// Active query descriptor for dependency-owned work.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query_id: Option<u16>,
    /// Number of scheduling waves in this class.
    pub waves: u64,
    /// Number of work items planned across these waves.
    pub planned_items: u64,
    /// Number of work items that became ready and began evaluation.
    pub ready_items: u64,
    /// Distribution of ready work-item counts per wave.
    pub ready_width: CompilationProfileCountDistribution,
    /// Distribution of simultaneous workers per wave.
    pub active_workers: CompilationProfileCountDistribution,
}

/// One observed compiler unit or size statistic.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CompilationProfileMetric {
    /// Stable numeric descriptor identity.
    pub id: u16,
    /// Aggregated metric value.
    pub value: u64,
}

/// One exact runtime artifact selected for the product link.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CompilationProfileRuntimeArtifact {
    /// Stable runtime component identity from the selected catalog.
    pub identity: String,
    /// Published archive size authenticated for this selection.
    pub bytes: u64,
}

/// Deterministic native plan recorded before LLVM generation and optimization.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct CompilationProfileNativeCodegen {
    /// Every reachable generated definition and external leaf in stable identity order.
    pub instances: Vec<CompilationProfileCodegenInstance>,
    /// Every typed reachability edge in source and target identity order.
    pub dependencies: Vec<CompilationProfileCodegenDependency>,
    /// Every generated unit in emission order.
    pub units: Vec<CompilationProfileCodegenUnit>,
    /// Every selected standard-library archive supplied to native linking or ThinLTO.
    pub standard_library_artifacts: Vec<CompilationProfileOptimizationArtifact>,
}

/// One reachable generated definition or external leaf.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CompilationProfileCodegenInstance {
    /// Report-local stable identity referenced by edges and units.
    pub id: u32,
    /// Exact selected native symbol when this instance contributes one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Whether product root selection demanded this instance directly.
    pub root: bool,
    /// Whether the instance is a bodyless external leaf.
    pub external: bool,
}

/// One typed reachability edge between report-local instance identities.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CompilationProfileCodegenDependency {
    /// Demanding instance identity.
    pub source: u32,
    /// Demanded instance identity.
    pub target: u32,
    /// Stable dependency role.
    pub kind: String,
}

/// One independently generated unit and the boundary class that formed it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CompilationProfileCodegenUnit {
    /// Emission-order unit identity.
    pub id: u32,
    /// Deterministic estimated generation work.
    pub work: u64,
    /// Report-local instance identities contained by the unit.
    pub instances: Vec<u32>,
    /// Package boundary shared by ordinary unit members.
    pub packages: Vec<String>,
    /// Linkage boundary shared by ordinary unit members.
    pub linkages: Vec<String>,
    /// Visibility boundary shared by ordinary unit members.
    pub visibilities: Vec<String>,
}

/// One selected standard-library native or optimization archive.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CompilationProfileOptimizationArtifact {
    /// Bundle-relative artifact path.
    pub path: String,
    /// Stable optimization partition identity when the archive contains LLVM modules.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partition: Option<String>,
    /// Number of independently summarized LLVM modules supplied by this archive.
    pub modules: u32,
    /// Authenticated archive size.
    pub bytes: u64,
    /// Platform-service roles implemented by this optimization partition.
    pub platform_services: Vec<String>,
}

/// One bounded detailed profiling event.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CompilationProfileEvent {
    /// Stable numeric operation descriptor identity.
    pub operation_id: u16,
    /// Monotonic event start relative to the profiling session.
    pub start_nanoseconds: u64,
    /// Observed event duration.
    pub duration_nanoseconds: u64,
    /// Stable numeric query-kind identity when this event describes a query.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query_id: Option<u16>,
    /// Stable detailed subject without private compiler addresses or demand identities.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<CompilationProfileSubject>,
    /// Operation completion category.
    pub outcome: CompilationProfileOutcome,
    /// Session-local worker shard that recorded the event.
    pub worker: u32,
    /// Worker-local sequence used to order equal timestamps.
    pub sequence: u64,
}

/// Time categories derived from nested operation spans.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CompilationProfileTimeBreakdown {
    /// In-process same-thread work summed across compiler workers.
    pub active_work_nanoseconds: u64,
    /// Same-thread time across every operation after subtracting nested operations.
    pub same_thread_self_nanoseconds: u64,
    /// Scheduler queue delay.
    pub scheduler_queue_nanoseconds: u64,
    /// Query dependency wait time.
    pub dependency_wait_nanoseconds: u64,
    /// Duration of work delegated to external tool processes.
    pub external_work_nanoseconds: u64,
}

/// Immutable profile from one compiler invocation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CompilationProfileReport {
    /// Profile schema revision.
    pub schema_revision: u32,
    /// Requested profiling detail mode.
    pub mode: CompilationProfileMode,
    /// Stable package, product, and target identity for this invocation.
    pub context: CompilationProfileContext,
    /// Configured event bound in trace mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_event_limit: Option<usize>,
    /// Invocation elapsed time when the report was captured.
    pub elapsed_nanoseconds: u64,
    /// Derived time categories for parallel and nested work.
    pub time: CompilationProfileTimeBreakdown,
    /// Worker occupancy, ready-work width, and query critical-path measurements.
    pub scheduler: CompilationProfileSchedulerStatistics,
    /// Descriptor metadata declared once for this report.
    pub descriptors: CompilationProfileDescriptorCatalog,
    /// Sparse operation aggregates with zero-observation kinds omitted.
    pub operations: Vec<CompilationProfileOperationStatistics>,
    /// Sparse query aggregates with zero-observation kinds omitted.
    pub queries: Vec<CompilationProfileQueryStatistics>,
    /// Sparse unit and size statistics with zero values omitted.
    pub metrics: Vec<CompilationProfileMetric>,
    /// Exact runtime artifacts selected for the product link.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runtime_artifacts: Vec<CompilationProfileRuntimeArtifact>,
    /// Exact private runtime ABI roles selected for the product.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runtime_roles: Vec<String>,
    /// Native callback entry symbols that require foreign thread entry and panic isolation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub native_callback_entries: Vec<String>,
    /// Deterministic reachability, partition, and standard-library selection inventory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_codegen: Option<CompilationProfileNativeCodegen>,
    /// Detailed events retained in trace mode.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<CompilationProfileEvent>,
    /// Detailed events discarded after bounded buffers became full.
    pub dropped_events: u64,
}

impl CompilationProfileReport {
    /// Resolves an operation descriptor referenced by an observation or event.
    pub fn operation_descriptor(&self, id: u16) -> Option<&CompilationProfileOperationDescriptor> {
        self.descriptors
            .operations
            .iter()
            .find(|entry| entry.id == id)
    }

    /// Resolves a query descriptor referenced by an observation or event.
    pub fn query_descriptor(&self, id: u16) -> Option<&CompilationProfileQueryDescriptor> {
        self.descriptors.queries.iter().find(|entry| entry.id == id)
    }

    /// Resolves a metric descriptor referenced by an observation.
    pub fn metric_descriptor(&self, id: u16) -> Option<&CompilationProfileMetricDescriptor> {
        self.descriptors.metrics.iter().find(|entry| entry.id == id)
    }
}
