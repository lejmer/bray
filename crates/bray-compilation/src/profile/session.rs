use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::ThreadId;
use std::time::Instant;

use super::descriptor::{
    ProfileMetricKind, ProfileOperation, ProfileQueryKind, records_trace,
};
use super::model::{
    CompilationProfileAggregation, CompilationProfileCategory, CompilationProfileConfiguration,
    CompilationProfileContext, CompilationProfileEvent, CompilationProfileMetric,
    CompilationProfileOperationStatistics, CompilationProfileOutcome,
    CompilationProfileQueryStatistics, CompilationProfileReport, CompilationProfileSubject,
    CompilationProfileSubjectKind, CompilationProfileTimeBreakdown, CompilationProfileUnit,
};
use crate::fact::CompilationFactKey;

const PROFILE_SCHEMA_REVISION: u32 = 1;

pub(crate) struct ProfileSession {
    configuration: CompilationProfileConfiguration,
    context: CompilationProfileContext,
    clock: Arc<dyn ProfileClock>,
    shards: Box<[Mutex<ProfileShard>]>,
}

impl std::fmt::Debug for ProfileSession {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProfileSession")
            .field("configuration", &self.configuration)
            .field("shard_count", &self.shards.len())
            .finish_non_exhaustive()
    }
}

impl ProfileSession {
    pub(crate) fn new(
        configuration: CompilationProfileConfiguration,
        worker_count: usize,
        context: CompilationProfileContext,
    ) -> Arc<Self> {
        Self::with_clock(
            configuration,
            worker_count,
            context,
            Arc::new(MonotonicClock::new()),
        )
    }

    fn with_clock(
        configuration: CompilationProfileConfiguration,
        worker_count: usize,
        context: CompilationProfileContext,
        clock: Arc<dyn ProfileClock>,
    ) -> Arc<Self> {
        let shard_count = worker_count.saturating_add(1).max(1);

        let trace_event_limit = if records_trace(configuration.mode()) {
            configuration.trace_event_limit()
        } else {
            0
        };

        let shards = (0..shard_count)
            .map(|index| {
                let base_capacity = trace_event_limit / shard_count;
                let remainder = trace_event_limit % shard_count;
                let capacity = base_capacity + usize::from(index < remainder);

                Mutex::new(ProfileShard::new(capacity))
            })
            .collect();

        Arc::new(Self {
            configuration,
            context,
            clock,
            shards,
        })
    }

    pub(crate) fn configuration(&self) -> CompilationProfileConfiguration {
        self.configuration
    }

    pub(crate) fn start(
        &self,
        operation: ProfileOperation,
        query: Option<ProfileQueryKind>,
    ) -> ProfileSpan<'_> {
        self.start_with_subject(operation, query, None)
    }

    pub(crate) fn start_query(
        &self,
        operation: ProfileOperation,
        key: &CompilationFactKey,
    ) -> ProfileSpan<'_> {
        let query = ProfileQueryKind::from_key(key);
        let subject = records_trace(self.configuration.mode()).then(|| profile_subject(key));

        self.start_with_subject(operation, Some(query), subject)
    }

    fn start_with_subject(
        &self,
        operation: ProfileOperation,
        query: Option<ProfileQueryKind>,
        subject: Option<ProfileSubjectRecord>,
    ) -> ProfileSpan<'_> {
        let worker = self.worker_index();
        let thread = std::thread::current().id();
        let started_at = self.clock.now_nanoseconds();

        let span_id = {
            let mut shard = available_shard(&self.shards[worker]);
            let span_id = shard.next_span;

            shard.next_span = shard.next_span.saturating_add(1);

            shard.spans.push(ActiveSpan {
                id: span_id,
                thread,
                child_nanoseconds: 0,
            });

            span_id
        };

        ProfileSpan {
            session: self,
            operation,
            query,
            subject,
            worker,
            thread,
            span_id,
            started_at,
            active: true,
        }
    }

    pub(crate) fn record_query_request(&self, query: ProfileQueryKind) {
        let mut shard = self.shard();
        let statistics = &mut shard.queries[query.index()];

        statistics.requests = statistics.requests.saturating_add(1);
    }

    pub(crate) fn record_query_cache_hit(&self, query: ProfileQueryKind) {
        let mut shard = self.shard();
        let statistics = &mut shard.queries[query.index()];

        statistics.cache_hits = statistics.cache_hits.saturating_add(1);
    }

    pub(crate) fn record_query_cache_miss(&self, query: ProfileQueryKind) {
        let mut shard = self.shard();
        let statistics = &mut shard.queries[query.index()];

        statistics.cache_misses = statistics.cache_misses.saturating_add(1);
    }

    pub(crate) fn record_cross_snapshot_reuse(&self, query: ProfileQueryKind) {
        let mut shard = self.shard();
        let statistics = &mut shard.queries[query.index()];

        statistics.cross_snapshot_reuses =
            statistics.cross_snapshot_reuses.saturating_add(1);
    }

    pub(crate) fn record_invalidation(&self, query: ProfileQueryKind) {
        let mut shard = self.shard();
        let statistics = &mut shard.queries[query.index()];

        statistics.invalidations = statistics.invalidations.saturating_add(1);
    }

    pub(crate) fn add_metric(&self, metric: ProfileMetricKind, value: u64) {
        let mut shard = self.shard();
        let current = &mut shard.metrics[metric.index()];

        *current = current.saturating_add(value);
    }

    pub(crate) fn report(&self) -> CompilationProfileReport {
        let mut operations = [ProfileAggregate::default(); ProfileOperation::COUNT];
        let mut queries = [ProfileQueryAggregate::default(); ProfileQueryKind::COUNT];
        let mut metrics = [0_u64; ProfileMetricKind::COUNT];
        let mut events = Vec::new();
        let mut dropped_events = 0_u64;

        for shard in &self.shards {
            let shard = available_shard(shard);

            merge_aggregates(&mut operations, &shard.operations);
            merge_queries(&mut queries, &shard.queries);
            merge_metrics(&mut metrics, &shard.metrics);

            dropped_events = dropped_events.saturating_add(shard.dropped_events);
            events.extend(shard.events.iter().copied());
        }

        events.sort_by_key(|event| (event.started_at, event.worker, event.sequence));

        CompilationProfileReport {
            schema_revision: PROFILE_SCHEMA_REVISION,
            mode: self.configuration.mode(),
            context: self.context.clone(),
            trace_event_limit: records_trace(self.configuration.mode())
                .then_some(self.configuration.trace_event_limit()),
            elapsed_nanoseconds: self.clock.now_nanoseconds(),
            time: time_breakdown(&operations),
            operations: operation_reports(&operations),
            queries: query_reports(&queries),
            metrics: metric_reports(&metrics),
            events: event_reports(events),
            dropped_events,
        }
    }

    fn finish_span(
        &self,
        operation: ProfileOperation,
        query: Option<ProfileQueryKind>,
        subject: Option<ProfileSubjectRecord>,
        worker: usize,
        thread: ThreadId,
        span_id: u64,
        started_at: u64,
        outcome: CompilationProfileOutcome,
    ) {
        let finished_at = self.clock.now_nanoseconds();
        let duration = finished_at.saturating_sub(started_at);
        let mut shard = available_shard(&self.shards[worker]);
        let self_nanoseconds = finish_active_span(&mut shard.spans, thread, span_id, duration);

        shard.operations[operation.index()].record(duration, self_nanoseconds, outcome);

        if let Some(query) = query {
            let statistics = &mut shard.queries[query.index()];

            match operation {
                ProfileOperation::QueryEvaluation => {
                    statistics.evaluations = statistics.evaluations.saturating_add(1);

                    statistics.evaluation_nanoseconds = statistics
                        .evaluation_nanoseconds
                        .saturating_add(duration);

                    if outcome == CompilationProfileOutcome::Completed
                        && let Some(metric) = query.completed_unit_metric()
                    {
                        shard.metrics[metric.index()] =
                            shard.metrics[metric.index()].saturating_add(1);
                    }
                }
                ProfileOperation::DependencyWait => {
                    statistics.waits = statistics.waits.saturating_add(1);

                    statistics.wait_nanoseconds =
                        statistics.wait_nanoseconds.saturating_add(duration);
                }
                _ => {}
            }
        }

        if !records_trace(self.configuration.mode()) {
            return;
        }

        let sequence = shard.next_sequence;

        shard.next_sequence = shard.next_sequence.saturating_add(1);

        if shard.events.len() == shard.events.capacity() {
            shard.dropped_events = shard.dropped_events.saturating_add(1);

            return;
        }

        shard.events.push(ProfileEventRecord {
            started_at,
            duration,
            operation,
            query,
            subject,
            outcome,
            worker: u32::try_from(worker).unwrap_or(u32::MAX),
            sequence,
        });
    }

    fn worker_index(&self) -> usize {
        rayon::current_thread_index()
            .filter(|index| *index < self.shards.len().saturating_sub(1))
            .unwrap_or_else(|| self.shards.len().saturating_sub(1))
    }

    fn shard(&self) -> MutexGuard<'_, ProfileShard> {
        available_shard(&self.shards[self.worker_index()])
    }
}

pub(crate) struct ProfileSpan<'session> {
    session: &'session ProfileSession,
    operation: ProfileOperation,
    query: Option<ProfileQueryKind>,
    subject: Option<ProfileSubjectRecord>,
    worker: usize,
    thread: ThreadId,
    span_id: u64,
    started_at: u64,
    active: bool,
}

impl ProfileSpan<'_> {
    pub(crate) fn finish(mut self, outcome: CompilationProfileOutcome) {
        self.session.finish_span(
            self.operation,
            self.query,
            self.subject,
            self.worker,
            self.thread,
            self.span_id,
            self.started_at,
            outcome,
        );

        self.active = false;
    }
}

impl Drop for ProfileSpan<'_> {
    fn drop(&mut self) {
        if self.active {
            self.session.finish_span(
                self.operation,
                self.query,
                self.subject,
                self.worker,
                self.thread,
                self.span_id,
                self.started_at,
                CompilationProfileOutcome::Abandoned,
            );
        }
    }
}

trait ProfileClock: Send + Sync {
    fn now_nanoseconds(&self) -> u64;
}

struct MonotonicClock {
    origin: Instant,
}

impl MonotonicClock {
    fn new() -> Self {
        Self {
            origin: Instant::now(),
        }
    }
}

impl ProfileClock for MonotonicClock {
    fn now_nanoseconds(&self) -> u64 {
        u64::try_from(self.origin.elapsed().as_nanos()).unwrap_or(u64::MAX)
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct ProfileAggregate {
    executions: u64,
    completed: u64,
    failed: u64,
    cancelled: u64,
    abandoned: u64,
    total_nanoseconds: u64,
    self_nanoseconds: u64,
    maximum_nanoseconds: u64,
}

impl ProfileAggregate {
    fn record(
        &mut self,
        duration: u64,
        self_nanoseconds: u64,
        outcome: CompilationProfileOutcome,
    ) {
        self.executions = self.executions.saturating_add(1);
        self.total_nanoseconds = self.total_nanoseconds.saturating_add(duration);
        self.self_nanoseconds = self.self_nanoseconds.saturating_add(self_nanoseconds);
        self.maximum_nanoseconds = self.maximum_nanoseconds.max(duration);

        let outcome_count = match outcome {
            CompilationProfileOutcome::Completed => &mut self.completed,
            CompilationProfileOutcome::Failed => &mut self.failed,
            CompilationProfileOutcome::Cancelled => &mut self.cancelled,
            CompilationProfileOutcome::Abandoned => &mut self.abandoned,
        };

        *outcome_count = outcome_count.saturating_add(1);
    }

    fn merge(&mut self, other: Self) {
        self.executions = self.executions.saturating_add(other.executions);
        self.completed = self.completed.saturating_add(other.completed);
        self.failed = self.failed.saturating_add(other.failed);
        self.cancelled = self.cancelled.saturating_add(other.cancelled);
        self.abandoned = self.abandoned.saturating_add(other.abandoned);

        self.total_nanoseconds = self
            .total_nanoseconds
            .saturating_add(other.total_nanoseconds);

        self.self_nanoseconds = self
            .self_nanoseconds
            .saturating_add(other.self_nanoseconds);

        self.maximum_nanoseconds = self.maximum_nanoseconds.max(other.maximum_nanoseconds);
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct ProfileQueryAggregate {
    requests: u64,
    cache_hits: u64,
    cache_misses: u64,
    cross_snapshot_reuses: u64,
    invalidations: u64,
    evaluations: u64,
    waits: u64,
    evaluation_nanoseconds: u64,
    wait_nanoseconds: u64,
}

impl ProfileQueryAggregate {
    fn merge(&mut self, other: Self) {
        self.requests = self.requests.saturating_add(other.requests);
        self.cache_hits = self.cache_hits.saturating_add(other.cache_hits);
        self.cache_misses = self.cache_misses.saturating_add(other.cache_misses);

        self.cross_snapshot_reuses = self
            .cross_snapshot_reuses
            .saturating_add(other.cross_snapshot_reuses);

        self.invalidations = self.invalidations.saturating_add(other.invalidations);
        self.evaluations = self.evaluations.saturating_add(other.evaluations);
        self.waits = self.waits.saturating_add(other.waits);

        self.evaluation_nanoseconds = self
            .evaluation_nanoseconds
            .saturating_add(other.evaluation_nanoseconds);

        self.wait_nanoseconds = self.wait_nanoseconds.saturating_add(other.wait_nanoseconds);
    }
}

#[derive(Clone, Copy, Debug)]
struct ProfileEventRecord {
    started_at: u64,
    duration: u64,
    operation: ProfileOperation,
    query: Option<ProfileQueryKind>,
    subject: Option<ProfileSubjectRecord>,
    outcome: CompilationProfileOutcome,
    worker: u32,
    sequence: u64,
}

#[derive(Clone, Copy, Debug)]
struct ProfileSubjectRecord {
    kind: CompilationProfileSubjectKind,
    fingerprint: u64,
}

#[derive(Clone, Copy, Debug)]
struct ActiveSpan {
    id: u64,
    thread: ThreadId,
    child_nanoseconds: u64,
}

#[derive(Debug)]
struct ProfileShard {
    operations: [ProfileAggregate; ProfileOperation::COUNT],
    queries: [ProfileQueryAggregate; ProfileQueryKind::COUNT],
    metrics: [u64; ProfileMetricKind::COUNT],
    events: Vec<ProfileEventRecord>,
    spans: Vec<ActiveSpan>,
    next_span: u64,
    next_sequence: u64,
    dropped_events: u64,
}

impl ProfileShard {
    fn new(trace_capacity: usize) -> Self {
        Self {
            operations: [ProfileAggregate::default(); ProfileOperation::COUNT],
            queries: [ProfileQueryAggregate::default(); ProfileQueryKind::COUNT],
            metrics: [0; ProfileMetricKind::COUNT],
            events: Vec::with_capacity(trace_capacity),
            spans: Vec::new(),
            next_span: 0,
            next_sequence: 0,
            dropped_events: 0,
        }
    }
}

fn available_shard(shard: &Mutex<ProfileShard>) -> MutexGuard<'_, ProfileShard> {
    shard.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn finish_active_span(
    spans: &mut Vec<ActiveSpan>,
    thread: ThreadId,
    span_id: u64,
    duration: u64,
) -> u64 {
    let Some(index) = spans
        .iter()
        .rposition(|span| span.thread == thread && span.id == span_id)
    else {
        return duration;
    };

    let span = spans.remove(index);

    if let Some(parent) = spans[..index]
        .iter_mut()
        .rfind(|parent| parent.thread == thread)
    {
        parent.child_nanoseconds = parent.child_nanoseconds.saturating_add(duration);
    }

    duration.saturating_sub(span.child_nanoseconds)
}

fn profile_subject(key: &CompilationFactKey) -> ProfileSubjectRecord {
    let kind = match key {
        CompilationFactKey::SourceUnitSyntax(_)
        | CompilationFactKey::SourceReferenceIndex(_)
        | CompilationFactKey::DeclarationChunk(_) => CompilationProfileSubjectKind::SourceUnit,
        CompilationFactKey::CodegenArtifact(_) => CompilationProfileSubjectKind::CodegenUnit,
        CompilationFactKey::NativeProduct(_)
        | CompilationFactKey::PackageInterfaceExportBundle
        | CompilationFactKey::ProductSourceGraph
        | CompilationFactKey::ProductSemantics
        | CompilationFactKey::TestDiscovery(_) => CompilationProfileSubjectKind::Product,
        key if key.bound_unit_key().is_some() => CompilationProfileSubjectKind::SemanticUnit,
        _ => CompilationProfileSubjectKind::Compilation,
    };

    let mut hasher = StableSubjectHasher::new();
    key.hash(&mut hasher);

    ProfileSubjectRecord {
        kind,
        fingerprint: hasher.finish(),
    }
}

struct StableSubjectHasher(u64);

impl StableSubjectHasher {
    const fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
}

impl Hasher for StableSubjectHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
}

fn merge_aggregates(
    destination: &mut [ProfileAggregate; ProfileOperation::COUNT],
    source: &[ProfileAggregate; ProfileOperation::COUNT],
) {
    for (destination, source) in destination.iter_mut().zip(source) {
        destination.merge(*source);
    }
}

fn merge_queries(
    destination: &mut [ProfileQueryAggregate; ProfileQueryKind::COUNT],
    source: &[ProfileQueryAggregate; ProfileQueryKind::COUNT],
) {
    for (destination, source) in destination.iter_mut().zip(source) {
        destination.merge(*source);
    }
}

fn merge_metrics(
    destination: &mut [u64; ProfileMetricKind::COUNT],
    source: &[u64; ProfileMetricKind::COUNT],
) {
    for (destination, source) in destination.iter_mut().zip(source) {
        *destination = destination.saturating_add(*source);
    }
}

fn operation_reports(
    aggregates: &[ProfileAggregate; ProfileOperation::COUNT],
) -> Vec<CompilationProfileOperationStatistics> {
    ProfileOperation::all()
        .into_iter()
        .zip(aggregates)
        .filter(|(_, aggregate)| aggregate.executions > 0)
        .map(|(operation, aggregate)| CompilationProfileOperationStatistics {
            id: operation.id(),
            name: operation.name().to_owned(),
            category: operation.category(),
            unit: CompilationProfileUnit::Nanoseconds,
            aggregation: CompilationProfileAggregation::SumAndMaximum,
            allowed_subjects: operation.allowed_subjects().to_vec(),
            executions: aggregate.executions,
            completed: aggregate.completed,
            failed: aggregate.failed,
            cancelled: aggregate.cancelled,
            abandoned: aggregate.abandoned,
            total_nanoseconds: aggregate.total_nanoseconds,
            self_nanoseconds: aggregate.self_nanoseconds,
            maximum_nanoseconds: aggregate.maximum_nanoseconds,
        })
        .collect()
}

fn query_reports(
    aggregates: &[ProfileQueryAggregate; ProfileQueryKind::COUNT],
) -> Vec<CompilationProfileQueryStatistics> {
    ProfileQueryKind::all()
        .into_iter()
        .zip(aggregates)
        .filter(|(_, aggregate)| {
            aggregate.requests > 0
                || aggregate.cross_snapshot_reuses > 0
                || aggregate.invalidations > 0
        })
        .map(|(query, aggregate)| CompilationProfileQueryStatistics {
            id: query.id(),
            name: query.name().to_owned(),
            requests: aggregate.requests,
            cache_hits: aggregate.cache_hits,
            cache_misses: aggregate.cache_misses,
            cross_snapshot_reuses: aggregate.cross_snapshot_reuses,
            invalidations: aggregate.invalidations,
            evaluations: aggregate.evaluations,
            waits: aggregate.waits,
            evaluation_nanoseconds: aggregate.evaluation_nanoseconds,
            wait_nanoseconds: aggregate.wait_nanoseconds,
        })
        .collect()
}

fn metric_reports(values: &[u64; ProfileMetricKind::COUNT]) -> Vec<CompilationProfileMetric> {
    ProfileMetricKind::all()
        .into_iter()
        .zip(values)
        .filter(|(_, value)| **value > 0)
        .map(|(metric, value)| {
            let (name, unit) = metric.descriptor();

            CompilationProfileMetric {
                id: metric.id(),
                name: name.to_owned(),
                unit,
                category: CompilationProfileCategory::Measurement,
                aggregation: CompilationProfileAggregation::Sum,
                allowed_subjects: metric.allowed_subjects().to_vec(),
                value: *value,
            }
        })
        .collect()
}

fn event_reports(records: Vec<ProfileEventRecord>) -> Vec<CompilationProfileEvent> {
    records
        .into_iter()
        .map(|record| CompilationProfileEvent {
            start_nanoseconds: record.started_at,
            duration_nanoseconds: record.duration,
            operation_id: record.operation.id(),
            operation: record.operation.name().to_owned(),
            query: record.query.map(|query| query.name().to_owned()),
            query_id: record.query.map(ProfileQueryKind::id),
            subject: record.subject.map(|subject| CompilationProfileSubject {
                kind: subject.kind,
                fingerprint: format!("{:016x}", subject.fingerprint),
            }),
            outcome: record.outcome,
            worker: record.worker,
            sequence: record.sequence,
        })
        .collect()
}

fn time_breakdown(
    aggregates: &[ProfileAggregate; ProfileOperation::COUNT],
) -> CompilationProfileTimeBreakdown {
    let mut active_work_nanoseconds = 0_u64;
    let mut same_thread_self_nanoseconds = 0_u64;
    let mut external_work_nanoseconds = 0_u64;

    for (operation, aggregate) in ProfileOperation::all().into_iter().zip(aggregates) {
        match operation.category() {
            CompilationProfileCategory::Work => {
                active_work_nanoseconds =
                    active_work_nanoseconds.saturating_add(aggregate.total_nanoseconds);

                same_thread_self_nanoseconds = same_thread_self_nanoseconds
                    .saturating_add(aggregate.self_nanoseconds);
            }
            CompilationProfileCategory::External => {
                external_work_nanoseconds =
                    external_work_nanoseconds.saturating_add(aggregate.total_nanoseconds);
            }
            CompilationProfileCategory::Wait
            | CompilationProfileCategory::Cache
            | CompilationProfileCategory::Measurement => {}
        }
    }

    CompilationProfileTimeBreakdown {
        active_work_nanoseconds,
        same_thread_self_nanoseconds,
        scheduler_queue_nanoseconds: aggregates[ProfileOperation::SchedulerQueue.index()]
            .total_nanoseconds,
        dependency_wait_nanoseconds: aggregates[ProfileOperation::DependencyWait.index()]
            .total_nanoseconds,
        external_work_nanoseconds,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};

    use rayon::prelude::{IntoParallelIterator, ParallelIterator};

    use super::{ProfileClock, ProfileSession};
    use crate::profile::{
        CompilationProfileConfiguration, CompilationProfileContext, CompilationProfileMode,
        CompilationProfileOutcome, ProfileMetricKind, ProfileOperation, ProfileQueryKind,
    };

    fn context() -> CompilationProfileContext {
        CompilationProfileContext {
            package: "test.package".to_owned(),
            product: "test".to_owned(),
            target: "test-target".to_owned(),
        }
    }

    struct TestClock(AtomicU64);

    impl TestClock {
        const fn new() -> Self {
            Self(AtomicU64::new(0))
        }

        fn advance(&self, nanoseconds: u64) {
            self.0.fetch_add(nanoseconds, Ordering::Relaxed);
        }
    }

    impl ProfileClock for TestClock {
        fn now_nanoseconds(&self) -> u64 {
            self.0.load(Ordering::Relaxed)
        }
    }

    #[test]
    fn summary_aggregates_operations_queries_and_metrics() {
        let clock = Arc::new(TestClock::new());

        let session = ProfileSession::with_clock(
            CompilationProfileConfiguration::new(CompilationProfileMode::Summary),
            1,
            context(),
            clock.clone(),
        );

        session.record_query_request(ProfileQueryKind::SyntaxTree);
        session.record_query_cache_hit(ProfileQueryKind::SyntaxTree);
        session.add_metric(ProfileMetricKind::SourceUnits, 2);

        let span = session.start(
            ProfileOperation::QueryEvaluation,
            Some(ProfileQueryKind::SyntaxTree),
        );

        clock.advance(17);
        span.finish(CompilationProfileOutcome::Completed);

        let report = session.report();

        assert_eq!(report.elapsed_nanoseconds, 17);
        assert_eq!(report.operations[0].total_nanoseconds, 17);
        assert_eq!(report.operations[0].self_nanoseconds, 17);
        assert_eq!(report.queries[0].requests, 1);
        assert_eq!(report.queries[0].cache_hits, 1);
        assert_eq!(report.queries[0].evaluations, 1);
        assert_eq!(report.metrics[0].value, 2);
        assert!(report.events.is_empty());
    }

    #[test]
    fn trace_buffers_are_bounded_and_report_drops() {
        let clock = Arc::new(TestClock::new());

        let session = ProfileSession::with_clock(
            CompilationProfileConfiguration::new(CompilationProfileMode::Trace)
                .with_trace_event_limit(1),
            0,
            context(),
            clock.clone(),
        );

        for _ in 0..2 {
            let span = session.start(ProfileOperation::CompilationLoad, None);

            clock.advance(1);
            span.finish(CompilationProfileOutcome::Completed);
        }

        let report = session.report();

        assert_eq!(report.events.len(), 1);
        assert_eq!(report.dropped_events, 1);
    }

    #[test]
    fn nested_spans_separate_inclusive_work_from_same_thread_self_time() {
        let clock = Arc::new(TestClock::new());

        let session = ProfileSession::with_clock(
            CompilationProfileConfiguration::new(CompilationProfileMode::Summary),
            1,
            context(),
            clock.clone(),
        );

        let outer = session.start(ProfileOperation::CompilationLoad, None);
        clock.advance(3);

        let inner = session.start(ProfileOperation::QueryEvaluation, None);
        clock.advance(5);
        inner.finish(CompilationProfileOutcome::Completed);

        clock.advance(7);
        outer.finish(CompilationProfileOutcome::Completed);

        let report = session.report();
        let load = &report.operations[0];
        let query = &report.operations[1];

        assert_eq!(load.total_nanoseconds, 15);
        assert_eq!(load.self_nanoseconds, 10);
        assert_eq!(query.total_nanoseconds, 5);
        assert_eq!(query.self_nanoseconds, 5);
        assert_eq!(report.time.active_work_nanoseconds, 20);
        assert_eq!(report.time.same_thread_self_nanoseconds, 15);
    }

    #[test]
    fn parallel_profile_aggregation_retains_every_observation() {
        let clock = Arc::new(TestClock::new());

        let session = ProfileSession::with_clock(
            CompilationProfileConfiguration::new(CompilationProfileMode::Summary),
            4,
            context(),
            clock.clone(),
        );

        rayon::ThreadPoolBuilder::new()
            .num_threads(4)
            .build()
            .unwrap_or_else(|error| panic!("test worker pool must build: {error:?}"))
            .install(|| {
                (0..256).into_par_iter().for_each(|_| {
                    let span = session.start(ProfileOperation::CodeGeneration, None);

                    clock.advance(1);
                    span.finish(CompilationProfileOutcome::Completed);
                });
            });

        let report = session.report();

        let codegen = report
            .operations
            .iter()
            .find(|operation| operation.name == "compiler.codegen.generate")
            .unwrap_or_else(|| panic!("code generation observations must be reported"));

        assert_eq!(codegen.executions, 256);
        assert_eq!(codegen.completed, 256);
    }
}
