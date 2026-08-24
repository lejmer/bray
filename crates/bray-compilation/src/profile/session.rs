use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::ThreadId;
use std::time::Instant;

use super::aggregate::{
    ActiveSpan, ProfileAggregate, ProfileEventRecord, ProfileQueryAggregate,
    ProfileSchedulerAggregate, ProfileShard, available_shard, finish_active_span, merge_aggregates,
    merge_metrics, merge_queries,
};
use super::concurrency::ProfileConcurrency;
use super::descriptor::{ProfileMetricKind, ProfileOperation, ProfileQueryKind, records_trace};
use super::subject::{ProfileSubjectRecord, profile_subject};
use crate::fact::CompilationFactKey;
use bray_profile::{
    COMPILATION_PROFILE_SCHEMA_REVISION, CompilationProfileAggregation, CompilationProfileCategory,
    CompilationProfileConfiguration, CompilationProfileContext,
    CompilationProfileDescriptorCatalog, CompilationProfileEvent, CompilationProfileMetric,
    CompilationProfileMetricDescriptor, CompilationProfileOperationDescriptor,
    CompilationProfileOperationStatistics, CompilationProfileOutcome,
    CompilationProfileQueryDescriptor, CompilationProfileQueryStatistics, CompilationProfileReport,
    CompilationProfileRuntimeArtifact, CompilationProfileSchedulerStatistics,
    CompilationProfileSubject, CompilationProfileTimeBreakdown, CompilationProfileUnit,
};

#[inline(always)]
pub(crate) fn profile_operation<T>(
    session: Option<&ProfileSession>,
    operation: ProfileOperation,
    action: impl FnOnce() -> T,
    outcome: impl FnOnce(&T) -> CompilationProfileOutcome,
) -> T {
    let Some(session) = session else {
        return action();
    };

    let span = session.start(operation, None);
    let result = action();

    span.finish(outcome(&result));

    result
}

pub(crate) struct ProfileSession {
    configuration: CompilationProfileConfiguration,
    context: CompilationProfileContext,
    worker_budget: usize,
    clock: Arc<dyn ProfileClock>,
    shards: Box<[Mutex<ProfileShard>]>,
    concurrency: ProfileConcurrency,
    runtime_artifacts: Mutex<BTreeMap<String, u64>>,
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
            worker_budget: worker_count.max(1),
            clock,
            concurrency: ProfileConcurrency::new(shard_count),
            shards,
            runtime_artifacts: Mutex::new(BTreeMap::new()),
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

        let subject = records_trace(self.configuration.mode()).then(|| profile_subject(query, key));

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

        self.concurrency.begin_operation(operation, worker);

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

    pub(crate) fn start_query_request(&self, query: ProfileQueryKind) -> ProfileQueryRequest<'_> {
        let mut shard = self.shard();
        let statistics = &mut shard.queries[query.index()];

        statistics.requests = statistics.requests.saturating_add(1);

        drop(shard);

        ProfileQueryRequest {
            session: self,
            query,
            started_at: self.clock.now_nanoseconds(),
        }
    }

    fn record_query_cache_hit(&self, query: ProfileQueryKind, started_at: u64) {
        let duration = self.clock.now_nanoseconds().saturating_sub(started_at);
        let mut shard = self.shard();
        let statistics = &mut shard.queries[query.index()];

        statistics.cache_hits = statistics.cache_hits.saturating_add(1);

        statistics.ready_value_nanoseconds =
            statistics.ready_value_nanoseconds.saturating_add(duration);

        statistics.ready_value_maximum_nanoseconds =
            statistics.ready_value_maximum_nanoseconds.max(duration);
    }

    pub(crate) fn record_query_cache_miss(&self, query: ProfileQueryKind) {
        let mut shard = self.shard();
        let statistics = &mut shard.queries[query.index()];

        statistics.cache_misses = statistics.cache_misses.saturating_add(1);
    }

    pub(crate) fn record_cross_snapshot_reuse(&self, query: ProfileQueryKind) {
        let mut shard = self.shard();
        let statistics = &mut shard.queries[query.index()];

        statistics.cross_snapshot_reuses = statistics.cross_snapshot_reuses.saturating_add(1);
    }

    pub(crate) fn record_invalidation(&self, query: ProfileQueryKind) {
        let mut shard = self.shard();
        let statistics = &mut shard.queries[query.index()];

        statistics.invalidations = statistics.invalidations.saturating_add(1);
    }

    pub(crate) fn record_query_publication(&self, query: ProfileQueryKind, inline_bytes: usize) {
        let mut shard = self.shard();
        let statistics = &mut shard.queries[query.index()];

        statistics.published_values = statistics.published_values.saturating_add(1);

        statistics.published_inline_bytes = statistics
            .published_inline_bytes
            .saturating_add(u64::try_from(inline_bytes).unwrap_or(u64::MAX));
    }

    pub(crate) fn record_query_diagnostics(&self, query: ProfileQueryKind, diagnostics: usize) {
        let mut shard = self.shard();
        let statistics = &mut shard.queries[query.index()];

        statistics.result_diagnostics = statistics
            .result_diagnostics
            .saturating_add(u64::try_from(diagnostics).unwrap_or(u64::MAX));
    }

    pub(crate) fn record_query_clone(
        &self,
        query: ProfileQueryKind,
        inline_bytes: usize,
        diagnostics: usize,
    ) {
        let mut shard = self.shard();
        let statistics = &mut shard.queries[query.index()];

        statistics.cloned_values = statistics.cloned_values.saturating_add(1);

        statistics.cloned_inline_bytes = statistics
            .cloned_inline_bytes
            .saturating_add(u64::try_from(inline_bytes).unwrap_or(u64::MAX));

        statistics.cloned_diagnostics = statistics
            .cloned_diagnostics
            .saturating_add(u64::try_from(diagnostics).unwrap_or(u64::MAX));
    }

    pub(crate) fn record_ready_wave(&self, width: usize) {
        if width == 0 {
            return;
        }

        let mut shard = self.shard();
        let scheduler = &mut shard.scheduler;
        let width = u64::try_from(width).unwrap_or(u64::MAX);

        scheduler.ready_waves = scheduler.ready_waves.saturating_add(1);
        scheduler.ready_items = scheduler.ready_items.saturating_add(width);
        scheduler.maximum_ready_width = scheduler.maximum_ready_width.max(width);
    }

    pub(crate) fn start_worker_activity(&self) -> ProfileWorkerActivity<'_> {
        self.concurrency.begin_worker();

        ProfileWorkerActivity {
            session: self,
            worker: self.worker_index(),
            started_at: self.clock.now_nanoseconds(),
        }
    }

    pub(crate) fn record_metric(&self, metric: ProfileMetricKind, value: u64) {
        let mut shard = self.shard();
        let current = &mut shard.metrics[metric.index()];

        *current = metric.merge(*current, value);
    }

    pub(crate) fn record_optimization_inputs(
        &self,
        requests: &[bray_codegen::BackendArtifactRequest],
        contributions: &bray_emitter::BackendContributionSet,
    ) {
        if !requests.iter().any(|request| {
            request.serialization().bitcode_semantics()
                == bray_codegen::BackendBitcodeSemantics::ThinLto
        }) {
            return;
        }

        let (modules, bytes) = contributions
            .contributions()
            .iter()
            .filter(|contribution| {
                contribution.id().kind() == bray_emitter::ArtifactKind::BackendBitcode
            })
            .fold((0_u64, 0_u64), |(modules, bytes), contribution| {
                (
                    modules.saturating_add(1),
                    bytes.saturating_add(contribution.content().byte_len()),
                )
            });

        self.record_metric(ProfileMetricKind::OptimizationModules, modules);
        self.record_metric(ProfileMetricKind::OptimizationInputBytes, bytes);
    }

    pub(crate) fn add_runtime_artifact(&self, identity: &str, bytes: u64) {
        let mut artifacts = self
            .runtime_artifacts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        artifacts.insert(identity.to_owned(), bytes);
    }

    pub(crate) fn report(&self) -> CompilationProfileReport {
        let mut operations = [ProfileAggregate::default(); ProfileOperation::COUNT];
        let mut queries = [ProfileQueryAggregate::default(); ProfileQueryKind::COUNT];
        let mut metrics = [0_u64; ProfileMetricKind::COUNT];
        let mut scheduler = ProfileSchedulerAggregate::default();
        let mut events = Vec::new();
        let mut dropped_events = 0_u64;

        for shard in &self.shards {
            let shard = available_shard(shard);

            merge_aggregates(&mut operations, &shard.operations);
            merge_queries(&mut queries, &shard.queries);
            merge_metrics(&mut metrics, &shard.metrics);
            scheduler.merge(shard.scheduler);

            dropped_events = dropped_events.saturating_add(shard.dropped_events);
            events.extend(shard.events.iter().copied());
        }

        events.sort_by_key(|event| (event.started_at, event.worker, event.sequence));

        // Reports own stable context text independently of the active profiling session.
        let context = self.context.clone();

        let runtime_artifacts = self
            .runtime_artifacts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .iter()
            .map(|(identity, bytes)| CompilationProfileRuntimeArtifact {
                identity: identity.clone(),
                bytes: *bytes,
            })
            .collect();

        CompilationProfileReport {
            schema_revision: COMPILATION_PROFILE_SCHEMA_REVISION,
            mode: self.configuration.mode(),
            context,
            trace_event_limit: records_trace(self.configuration.mode())
                .then_some(self.configuration.trace_event_limit()),
            elapsed_nanoseconds: self.clock.now_nanoseconds(),
            time: time_breakdown(&operations),
            scheduler: scheduler_report(
                scheduler,
                self.worker_budget,
                self.concurrency.maximum_workers(),
                &queries,
            ),
            descriptors: descriptor_catalog(),
            operations: operation_reports(&operations, &self.concurrency),
            queries: query_reports(&queries),
            metrics: metric_reports(&metrics),
            runtime_artifacts,
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

                    statistics.evaluation_nanoseconds =
                        statistics.evaluation_nanoseconds.saturating_add(duration);

                    statistics.evaluation_self_nanoseconds = statistics
                        .evaluation_self_nanoseconds
                        .saturating_add(self_nanoseconds);

                    statistics.evaluation_latency.record(duration);

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

        self.concurrency.finish_operation(operation, worker);

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

pub(crate) struct ProfileQueryRequest<'session> {
    session: &'session ProfileSession,
    query: ProfileQueryKind,
    started_at: u64,
}

impl ProfileQueryRequest<'_> {
    pub(crate) fn finish_hit(self) {
        self.session
            .record_query_cache_hit(self.query, self.started_at);
    }

    pub(crate) fn finish_miss(self) {
        self.session.record_query_cache_miss(self.query);
    }
}

pub(crate) struct ProfileWorkerActivity<'session> {
    session: &'session ProfileSession,
    worker: usize,
    started_at: u64,
}

impl Drop for ProfileWorkerActivity<'_> {
    fn drop(&mut self) {
        let duration = self
            .session
            .clock
            .now_nanoseconds()
            .saturating_sub(self.started_at);

        let mut shard = available_shard(&self.session.shards[self.worker]);

        shard.scheduler.active_worker_nanoseconds = shard
            .scheduler
            .active_worker_nanoseconds
            .saturating_add(duration);

        drop(shard);

        self.session.concurrency.finish_worker();
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

fn operation_reports(
    aggregates: &[ProfileAggregate; ProfileOperation::COUNT],
    concurrency: &ProfileConcurrency,
) -> Vec<CompilationProfileOperationStatistics> {
    ProfileOperation::all()
        .into_iter()
        .zip(aggregates)
        .filter(|(_, aggregate)| aggregate.executions > 0)
        .map(
            |(operation, aggregate)| CompilationProfileOperationStatistics {
                id: operation.id(),
                executions: aggregate.executions,
                completed: aggregate.completed,
                failed: aggregate.failed,
                cancelled: aggregate.cancelled,
                abandoned: aggregate.abandoned,
                total_nanoseconds: aggregate.total_nanoseconds,
                self_nanoseconds: aggregate.self_nanoseconds,
                maximum_nanoseconds: aggregate.maximum_nanoseconds,
                maximum_active_workers: concurrency.operation_maximum_workers(operation),
            },
        )
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
            requests: aggregate.requests,
            cache_hits: aggregate.cache_hits,
            cache_misses: aggregate.cache_misses,
            cross_snapshot_reuses: aggregate.cross_snapshot_reuses,
            invalidations: aggregate.invalidations,
            evaluations: aggregate.evaluations,
            waits: aggregate.waits,
            evaluation_nanoseconds: aggregate.evaluation_nanoseconds,
            evaluation_self_nanoseconds: aggregate.evaluation_self_nanoseconds,
            evaluation_latency: aggregate.evaluation_latency.report(),
            wait_nanoseconds: aggregate.wait_nanoseconds,
            ready_value_nanoseconds: aggregate.ready_value_nanoseconds,
            ready_value_maximum_nanoseconds: aggregate.ready_value_maximum_nanoseconds,
            published_values: aggregate.published_values,
            published_inline_bytes: aggregate.published_inline_bytes,
            result_diagnostics: aggregate.result_diagnostics,
            cloned_values: aggregate.cloned_values,
            cloned_inline_bytes: aggregate.cloned_inline_bytes,
            cloned_diagnostics: aggregate.cloned_diagnostics,
        })
        .collect()
}

fn scheduler_report(
    aggregate: ProfileSchedulerAggregate,
    worker_budget: usize,
    maximum_active_workers: u64,
    queries: &[ProfileQueryAggregate; ProfileQueryKind::COUNT],
) -> CompilationProfileSchedulerStatistics {
    CompilationProfileSchedulerStatistics {
        worker_budget: u64::try_from(worker_budget).unwrap_or(u64::MAX),
        active_worker_nanoseconds: aggregate.active_worker_nanoseconds,
        maximum_active_workers,
        ready_waves: aggregate.ready_waves,
        ready_items: aggregate.ready_items,
        maximum_ready_width: aggregate.maximum_ready_width,
        query_critical_path_nanoseconds: queries
            .iter()
            .map(|query| query.evaluation_latency.report().maximum_nanoseconds)
            .max()
            .unwrap_or(0),
    }
}

fn metric_reports(values: &[u64; ProfileMetricKind::COUNT]) -> Vec<CompilationProfileMetric> {
    ProfileMetricKind::all()
        .into_iter()
        .zip(values)
        .filter(|(_, value)| **value > 0)
        .map(|(metric, value)| CompilationProfileMetric {
            id: metric.id(),
            value: *value,
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

fn descriptor_catalog() -> CompilationProfileDescriptorCatalog {
    let operations = ProfileOperation::all()
        .into_iter()
        .map(|operation| CompilationProfileOperationDescriptor {
            id: operation.id(),
            name: operation.name().to_owned(),
            category: operation.category(),
            unit: CompilationProfileUnit::Nanoseconds,
            aggregation: CompilationProfileAggregation::SumAndMaximum,
            allowed_subjects: operation.allowed_subjects().to_vec(),
        })
        .collect();

    let queries = ProfileQueryKind::all()
        .into_iter()
        .map(|query| CompilationProfileQueryDescriptor {
            id: query.id(),
            name: query.name().to_owned(),
        })
        .collect();

    let metrics = ProfileMetricKind::all()
        .into_iter()
        .map(|metric| {
            let (name, unit) = metric.descriptor();

            CompilationProfileMetricDescriptor {
                id: metric.id(),
                name: name.to_owned(),
                unit,
                category: CompilationProfileCategory::Measurement,
                aggregation: metric.aggregation(),
                allowed_subjects: metric.allowed_subjects().to_vec(),
            }
        })
        .collect();

    CompilationProfileDescriptorCatalog {
        operations,
        queries,
        metrics,
    }
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
                    active_work_nanoseconds.saturating_add(aggregate.self_nanoseconds);
            }
            CompilationProfileCategory::External => {
                external_work_nanoseconds =
                    external_work_nanoseconds.saturating_add(aggregate.total_nanoseconds);
            }
            CompilationProfileCategory::Wait
            | CompilationProfileCategory::Cache
            | CompilationProfileCategory::Measurement => {}
        }

        same_thread_self_nanoseconds =
            same_thread_self_nanoseconds.saturating_add(aggregate.self_nanoseconds);
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

    use super::{ProfileClock, ProfileSession, descriptor_catalog, merge_metrics};
    use crate::profile::{
        CompilationProfileAggregation, CompilationProfileConfiguration, CompilationProfileContext,
        CompilationProfileMode, CompilationProfileOutcome, ProfileMetricKind, ProfileOperation,
        ProfileQueryKind,
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

        let ready = session.start_query_request(ProfileQueryKind::SyntaxTree);
        clock.advance(3);
        ready.finish_hit();

        session
            .start_query_request(ProfileQueryKind::SyntaxTree)
            .finish_miss();

        session.record_metric(ProfileMetricKind::SourceUnits, 2);
        session.record_ready_wave(4);

        {
            let _activity = session.start_worker_activity();
            clock.advance(11);
        }

        let span = session.start(
            ProfileOperation::QueryEvaluation,
            Some(ProfileQueryKind::SyntaxTree),
        );

        clock.advance(17);
        span.finish(CompilationProfileOutcome::Completed);
        session.record_query_publication(ProfileQueryKind::SyntaxTree, 24);
        session.record_query_diagnostics(ProfileQueryKind::SyntaxTree, 2);
        session.record_query_clone(ProfileQueryKind::SyntaxTree, 16, 2);

        let report = session.report();

        assert_eq!(report.elapsed_nanoseconds, 31);
        assert_eq!(report.operations[0].total_nanoseconds, 17);
        assert_eq!(report.operations[0].self_nanoseconds, 17);
        assert_eq!(report.queries[0].requests, 2);
        assert_eq!(report.queries[0].cache_hits, 1);
        assert_eq!(report.queries[0].cache_misses, 1);
        assert_eq!(report.queries[0].evaluations, 1);
        assert_eq!(report.queries[0].evaluation_self_nanoseconds, 17);
        assert_eq!(report.queries[0].evaluation_latency.samples, 1);
        assert_eq!(report.queries[0].evaluation_latency.minimum_nanoseconds, 17);
        assert_eq!(report.queries[0].evaluation_latency.maximum_nanoseconds, 17);
        assert_eq!(report.queries[0].ready_value_nanoseconds, 3);
        assert_eq!(report.queries[0].published_values, 1);
        assert_eq!(report.queries[0].published_inline_bytes, 24);
        assert_eq!(report.queries[0].result_diagnostics, 2);
        assert_eq!(report.queries[0].cloned_values, 1);
        assert_eq!(report.queries[0].cloned_inline_bytes, 16);
        assert_eq!(report.queries[0].cloned_diagnostics, 2);
        assert_eq!(report.metrics[0].value, 2);
        assert_eq!(report.scheduler.active_worker_nanoseconds, 11);
        assert_eq!(report.scheduler.maximum_active_workers, 1);
        assert_eq!(report.scheduler.ready_waves, 1);
        assert_eq!(report.scheduler.ready_items, 4);
        assert_eq!(report.scheduler.maximum_ready_width, 4);
        assert_eq!(report.scheduler.query_critical_path_nanoseconds, 17);
        assert!(report.validate().is_ok());
        assert!(report.events.is_empty());
    }

    #[test]
    fn peak_metrics_retain_maxima_within_and_across_profile_shards() {
        let session = ProfileSession::with_clock(
            CompilationProfileConfiguration::new(CompilationProfileMode::Summary),
            1,
            context(),
            Arc::new(TestClock::new()),
        );

        session.record_metric(ProfileMetricKind::OptimizationPeakResidentBytes, 4096);
        session.record_metric(ProfileMetricKind::OptimizationPeakResidentBytes, 2048);

        let report = session.report();

        let peak = report
            .metrics
            .iter()
            .find(|metric| metric.id == ProfileMetricKind::OptimizationPeakResidentBytes.id())
            .unwrap_or_else(|| panic!("peak metric must be present"));

        assert_eq!(peak.value, 4096);

        let mut destination = [0_u64; ProfileMetricKind::COUNT];
        let mut source = [0_u64; ProfileMetricKind::COUNT];

        destination[ProfileMetricKind::SourceUnits.index()] = 2;
        source[ProfileMetricKind::SourceUnits.index()] = 3;
        destination[ProfileMetricKind::OptimizationActiveWorkers.index()] = 4;
        source[ProfileMetricKind::OptimizationActiveWorkers.index()] = 6;

        merge_metrics(&mut destination, &source);

        assert_eq!(destination[ProfileMetricKind::SourceUnits.index()], 5);

        assert_eq!(
            destination[ProfileMetricKind::OptimizationActiveWorkers.index()],
            6
        );

        let catalog = descriptor_catalog();

        let descriptor = catalog
            .metrics
            .iter()
            .find(|metric| metric.id == ProfileMetricKind::OptimizationActiveWorkers.id())
            .unwrap_or_else(|| panic!("worker metric descriptor must be present"));

        assert_eq!(
            descriptor.aggregation,
            CompilationProfileAggregation::Maximum
        );
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
        assert_eq!(report.time.active_work_nanoseconds, 15);
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
            .find(|operation| {
                report
                    .operation_descriptor(operation.id)
                    .is_some_and(|descriptor| descriptor.name == "compiler.codegen.generate")
            })
            .unwrap_or_else(|| panic!("code generation observations must be reported"));

        assert_eq!(codegen.executions, 256);
        assert_eq!(codegen.completed, 256);
    }
}
