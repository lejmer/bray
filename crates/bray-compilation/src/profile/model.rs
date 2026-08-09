use serde::{Deserialize, Serialize};

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

/// Aggregate statistics for one stable compiler operation kind.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CompilationProfileOperationStatistics {
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
}

/// Aggregate behavior for one stable compiler query kind.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CompilationProfileQueryStatistics {
    /// Stable numeric query-kind identity.
    pub id: u16,
    /// Canonical query-kind name.
    pub name: String,
    /// Number of requests for this query kind.
    pub requests: u64,
    /// Number of requests served from an already-published value.
    pub cache_hits: u64,
    /// Number of requests that found no published value.
    pub cache_misses: u64,
    /// Number of published facts retained from a preceding snapshot.
    pub cross_snapshot_reuses: u64,
    /// Number of preceding-snapshot facts invalidated before reuse.
    pub invalidations: u64,
    /// Number of independently executed evaluations.
    pub evaluations: u64,
    /// Number of waits for an evaluation owned by another task.
    pub waits: u64,
    /// Time spent evaluating this query kind.
    pub evaluation_nanoseconds: u64,
    /// Time spent waiting for this query kind.
    pub wait_nanoseconds: u64,
}

/// One stable compiler unit or size statistic.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CompilationProfileMetric {
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
    /// Aggregated metric value.
    pub value: u64,
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
    /// Canonical operation name.
    pub operation: String,
    /// Canonical query kind when this event describes a compiler query.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
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

/// Immutable versioned profile from one compiler invocation.
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
    /// Stable operation aggregates.
    pub operations: Vec<CompilationProfileOperationStatistics>,
    /// Stable query-kind aggregates.
    pub queries: Vec<CompilationProfileQueryStatistics>,
    /// Stable unit and size statistics.
    pub metrics: Vec<CompilationProfileMetric>,
    /// Detailed events retained in trace mode.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<CompilationProfileEvent>,
    /// Detailed events discarded after bounded buffers became full.
    pub dropped_events: u64,
}
