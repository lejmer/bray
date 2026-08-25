use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::ThreadId;
use std::time::Instant;

use super::aggregate::{
    ActiveSpan, ProfileAggregate, ProfileEventRecord, ProfileQueryAggregate,
    ProfileSchedulerAggregate, ProfileSchedulingWaveAggregate, ProfileSchedulingWaveKey,
    ProfileShard, available_shard, finish_active_span, merge_aggregates, merge_metrics,
    merge_queries,
};
use super::concurrency::ProfileConcurrency;
use super::descriptor::{ProfileMetricKind, ProfileOperation, ProfileQueryKind, records_trace};
use super::report::{
    descriptor_catalog, event_reports, metric_reports, operation_reports, query_reports,
    scheduler_report, time_breakdown,
};
use super::subject::{ProfileSubjectRecord, profile_subject};
use crate::fact::CompilationFactKey;
use bray_diagnostics::DiagnosticBag;
use bray_profile::{
    COMPILATION_PROFILE_SCHEMA_REVISION, CompilationProfileConfiguration,
    CompilationProfileContext, CompilationProfileOutcome, CompilationProfileReport,
    CompilationProfileRuntimeArtifact,
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

pub(crate) fn merge_diagnostics<'diagnostic>(
    session: Option<&ProfileSession>,
    diagnostics: impl IntoIterator<Item = &'diagnostic DiagnosticBag>,
) -> DiagnosticBag {
    let Some(session) = session else {
        return DiagnosticBag::merged_all(diagnostics);
    };

    let diagnostics = diagnostics.into_iter().collect::<Vec<_>>();

    let volume = diagnostics.iter().fold(0_usize, |total, diagnostics| {
        total.saturating_add(diagnostics.len())
    });

    let merged = DiagnosticBag::merged_all(diagnostics);

    session.record_diagnostic_merge(volume);

    merged
}

#[inline(always)]
pub(crate) fn record_query_diagnostic_collection(
    profile: Option<(&ProfileSession, ProfileQueryKind)>,
    diagnostics: usize,
) {
    if let Some((session, query)) = profile {
        session.record_query_diagnostics(query, diagnostics);
    }
}

#[inline(always)]
pub(crate) fn record_query_result_reference<T>(
    profile: Option<(&ProfileSession, ProfileQueryKind)>,
) {
    if let Some((session, query)) = profile {
        session.record_query_clone(query, std::mem::size_of::<Arc<T>>(), 0);
    }
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
            concurrency: ProfileConcurrency::new(),
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

        self.concurrency.begin_operation(operation);

        let span_id = {
            let mut shard = available_shard(&self.shards[worker]);
            let span_id = shard.next_span;

            shard.next_span = shard.next_span.saturating_add(1);

            shard.spans.push(ActiveSpan {
                id: span_id,
                thread,
                operation,
                query,
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

        statistics.diagnostic_collections = statistics.diagnostic_collections.saturating_add(1);

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

        if diagnostics > 0 {
            statistics.diagnostic_copies = statistics.diagnostic_copies.saturating_add(1);

            statistics.cloned_diagnostics = statistics
                .cloned_diagnostics
                .saturating_add(u64::try_from(diagnostics).unwrap_or(u64::MAX));
        }
    }

    pub(crate) fn record_diagnostic_merge(&self, diagnostics: usize) {
        let Some(query) = self.current_span_context().1 else {
            return;
        };

        let mut shard = self.shard();
        let statistics = &mut shard.queries[query.index()];

        statistics.diagnostic_merges = statistics.diagnostic_merges.saturating_add(1);

        statistics.merged_diagnostics = statistics
            .merged_diagnostics
            .saturating_add(u64::try_from(diagnostics).unwrap_or(u64::MAX));
    }

    fn current_span_context(&self) -> (Option<ProfileOperation>, Option<ProfileQueryKind>) {
        let thread = std::thread::current().id();
        let shard = self.shard();

        shard
            .spans
            .iter()
            .rev()
            .find(|span| span.thread == thread)
            .map_or((None, None), |span| (Some(span.operation), span.query))
    }

    pub(crate) fn start_scheduling_wave(&self, planned: usize) -> ProfileSchedulingWave<'_> {
        let (operation, query) = self.current_span_context();

        ProfileSchedulingWave {
            session: self,
            key: ProfileSchedulingWaveKey {
                operation_id: operation.map(ProfileOperation::id),
                query_id: query.map(ProfileQueryKind::id),
            },
            planned: u64::try_from(planned).unwrap_or(u64::MAX),
            ready: AtomicU64::new(0),
            active_workers: AtomicU64::new(0),
            maximum_active_workers: AtomicU64::new(0),
        }
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
        let mut scheduling_waves = BTreeMap::new();
        let mut events = Vec::new();
        let mut dropped_events = 0_u64;

        for shard in &self.shards {
            let shard = available_shard(shard);

            merge_aggregates(&mut operations, &shard.operations);
            merge_queries(&mut queries, &shard.queries);
            merge_metrics(&mut metrics, &shard.metrics);
            scheduler.merge(shard.scheduler);

            for (key, aggregate) in &shard.scheduling_waves {
                scheduling_waves
                    .entry(*key)
                    .or_insert_with(ProfileSchedulingWaveAggregate::default)
                    .merge(*aggregate);
            }

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
                scheduling_waves,
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

        self.concurrency.finish_operation(operation);

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

pub(crate) struct ProfileSchedulingWave<'session> {
    session: &'session ProfileSession,
    key: ProfileSchedulingWaveKey,
    planned: u64,
    ready: AtomicU64,
    active_workers: AtomicU64,
    maximum_active_workers: AtomicU64,
}

impl ProfileSchedulingWave<'_> {
    pub(crate) fn start_ready_item(&self) -> ProfileSchedulingWaveWorker<'_> {
        self.ready.fetch_add(1, Ordering::Relaxed);

        let active = self
            .active_workers
            .fetch_add(1, Ordering::Relaxed)
            .saturating_add(1);

        self.maximum_active_workers
            .fetch_max(active, Ordering::Relaxed);

        ProfileSchedulingWaveWorker {
            active_workers: &self.active_workers,
        }
    }
}

impl Drop for ProfileSchedulingWave<'_> {
    fn drop(&mut self) {
        let ready = self.ready.load(Ordering::Relaxed);
        let active_workers = self.maximum_active_workers.load(Ordering::Relaxed);
        let mut shard = self.session.shard();

        shard.scheduling_waves.entry(self.key).or_default().record(
            self.planned,
            ready,
            active_workers,
        );

        let scheduler = &mut shard.scheduler;

        scheduler.ready_waves = scheduler.ready_waves.saturating_add(1);
        scheduler.ready_items = scheduler.ready_items.saturating_add(ready);
        scheduler.maximum_ready_width = scheduler.maximum_ready_width.max(ready);
    }
}

pub(crate) struct ProfileSchedulingWaveWorker<'wave> {
    active_workers: &'wave AtomicU64,
}

impl Drop for ProfileSchedulingWaveWorker<'_> {
    fn drop(&mut self) {
        self.active_workers.fetch_sub(1, Ordering::Relaxed);
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

        {
            let wave = session.start_scheduling_wave(4);

            for _ in 0..4 {
                let _worker = wave.start_ready_item();
            }
        }

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
        assert_eq!(report.queries[0].diagnostic_collections, 1);
        assert_eq!(report.queries[0].result_diagnostics, 2);
        assert_eq!(report.queries[0].cloned_values, 1);
        assert_eq!(report.queries[0].cloned_inline_bytes, 16);
        assert_eq!(report.queries[0].diagnostic_copies, 1);
        assert_eq!(report.queries[0].cloned_diagnostics, 2);
        assert_eq!(report.metrics[0].value, 2);
        assert_eq!(report.scheduler.active_worker_nanoseconds, 11);
        assert_eq!(report.scheduler.maximum_active_workers, 1);
        assert_eq!(report.scheduler.ready_waves, 1);
        assert_eq!(report.scheduler.ready_items, 4);
        assert_eq!(report.scheduler.maximum_ready_width, 4);
        assert_eq!(report.scheduler.wave_classes.len(), 1);
        assert_eq!(report.scheduler.wave_classes[0].planned_items, 4);
        assert_eq!(report.scheduler.wave_classes[0].ready_width.maximum, 4);
        assert_eq!(report.scheduler.wave_classes[0].active_workers.maximum, 1);
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

    mod integration {
        use bray_source::{SourceIdentity, SourceInput, SourceVersion};

        use crate::profile::{CompilationProfileConfiguration, CompilationProfileMode};
        use crate::test_support::{package_identity, source_input};
        use crate::{Compilation, CompilationRequest};

        #[test]
        fn disabled_compilations_do_not_create_profile_state() {
            let compilation = Compilation::load(CompilationRequest::new(
                package_identity(),
                vec![source_input("module test.package;\n", 1)],
            ))
            .unwrap_or_else(|error| panic!("test compilation must load: {error:?}"));

            let _ = compilation.check_diagnostics();

            assert!(compilation.profile_report().is_none());
        }

        #[test]
        fn profiling_preserves_compilation_diagnostics() {
            let source = concat!(
                "module test.package;\n",
                "func broken()\n",
                "{\n",
                "    missing;\n",
                "}\n",
            );

            let baseline = Compilation::load(CompilationRequest::new(
                package_identity(),
                vec![source_input(source, 1)],
            ))
            .unwrap_or_else(|error| panic!("baseline compilation must load: {error:?}"));

            let profiled = Compilation::load(
                CompilationRequest::new(
                    package_identity(),
                    vec![source_input(source, 1)],
                )
                .with_profile(CompilationProfileConfiguration::new(
                    CompilationProfileMode::Trace,
                )),
            )
            .unwrap_or_else(|error| panic!("profiled compilation must load: {error:?}"));

            assert_eq!(baseline.check_diagnostics(), profiled.check_diagnostics());

            let report = profiled
                .profile_report()
                .unwrap_or_else(|| panic!("profiled compilation must retain a report"));

            assert!(
                report
                    .queries
                    .iter()
                    .any(|query| query.result_diagnostics > 0)
            );

            assert!(report.queries.iter().all(|query| {
                query.diagnostic_copies == 0 && query.cloned_diagnostics == 0
            }));
        }

        #[test]
        fn enabled_compilations_report_queries_metrics_and_selected_detail() {
            let request = CompilationRequest::new(
                package_identity(),
                vec![source_input("module test.package;\n", 1)],
            )
            .with_profile(CompilationProfileConfiguration::new(
                CompilationProfileMode::Summary,
            ));

            let compilation = Compilation::load(request)
                .unwrap_or_else(|error| panic!("test compilation must load: {error:?}"));

            let _ = compilation.check_diagnostics();

            let report = compilation
                .profile_report()
                .unwrap_or_else(|| panic!("profiled compilation must retain a report"));

            assert_eq!(report.mode, CompilationProfileMode::Summary);
            assert!(report.queries.iter().any(|query| query.requests > 0));
            assert!(report.queries.iter().any(|query| query.cache_misses > 0));

            assert!(report.queries.iter().any(|query| {
                query.diagnostic_collections > 0 && query.cloned_values > 0
            }));

            assert!(report.queries.iter().all(|query| {
                query.diagnostic_copies == 0 && query.cloned_diagnostics == 0
            }));

            assert!(
                report
                    .queries
                    .iter()
                    .any(|query| query.diagnostic_merges > 0)
            );

            assert!(report.scheduler.wave_classes.iter().any(|class| {
                class.waves > 0
                    && class.planned_items >= class.ready_items
                    && class.ready_width.samples == class.waves
                    && class.active_workers.samples == class.waves
            }));

            assert!(report.metrics.iter().any(|metric| {
                metric.value == 1
                    && report
                        .metric_descriptor(metric.id)
                        .is_some_and(|descriptor| descriptor.name == "compiler.source.units")
            }));

            assert!(report.events.is_empty());
        }

        #[test]
        fn broad_diagnostics_expose_multiple_semantic_work_waves() {
            let source = concat!(
                "module test.package;\n",
                "func first()\n",
                "{\n",
                "}\n",
                "func second()\n",
                "{\n",
                "}\n",
                "func third()\n",
                "{\n",
                "}\n",
                "func fourth()\n",
                "{\n",
                "}\n",
            );

            let request =
                CompilationRequest::new(package_identity(), vec![source_input(source, 1)])
                    .with_profile(CompilationProfileConfiguration::new(
                        CompilationProfileMode::Summary,
                    ));

            let compilation = Compilation::load(request)
                .unwrap_or_else(|error| panic!("profiled compilation must load: {error:?}"));

            let _ = compilation.check_diagnostics();

            let report = compilation
                .profile_report()
                .unwrap_or_else(|| panic!("profiled compilation must retain a report"));

            let semantic_waves = report
                .scheduler
                .wave_classes
                .iter()
                .find(|class| {
                    class.query_id.is_some_and(|id| {
                        report
                            .query_descriptor(id)
                            .is_some_and(|descriptor| descriptor.name == "semantic_diagnostics")
                    })
                })
                .unwrap_or_else(|| panic!("semantic diagnostics must publish scheduling waves"));

            assert!(semantic_waves.waves >= 2);
            assert!(semantic_waves.planned_items >= 8);
            assert!(semantic_waves.ready_width.maximum >= 4);
        }

        #[test]
        fn updated_snapshots_report_reuse_and_invalidation() {
            let request = CompilationRequest::new(
                package_identity(),
                vec![
                    revision("module test.package;\n", 1, 1),
                    revision("module test.stable;\n", 2, 1),
                ],
            )
            .with_profile(CompilationProfileConfiguration::new(
                CompilationProfileMode::Summary,
            ));

            let compilation = Compilation::load(request)
                .unwrap_or_else(|error| panic!("test compilation must load: {error:?}"));

            let _ = compilation.check_diagnostics();
            let _ = compilation.source_unit_syntax(bray_source::SourceId::new(1));

            let updated = compilation
                .updated_sources(vec![
                    revision("module test.package;\nfn added() {}\n", 1, 2),
                    revision("module test.stable;\n", 2, 1),
                ])
                .unwrap_or_else(|error| panic!("updated compilation must load: {error:?}"));

            let report = updated
                .profile_report()
                .unwrap_or_else(|| panic!("updated compilation must retain profiling"));

            assert!(
                report
                    .queries
                    .iter()
                    .any(|query| query.cross_snapshot_reuses > 0)
            );

            assert!(report.queries.iter().any(|query| query.invalidations > 0));
        }

        #[test]
        fn immutable_inputs_and_frozen_graphs_avoid_ready_query_traffic() {
            let request = CompilationRequest::new(
                package_identity(),
                vec![source_input("module test.package;\n", 1)],
            )
            .with_profile(CompilationProfileConfiguration::new(
                CompilationProfileMode::Summary,
            ));

            let compilation = Compilation::load(request)
                .unwrap_or_else(|error| panic!("test compilation must load: {error:?}"));

            for _ in 0..8 {
                let _ = compilation.selected_target();
            }

            for _ in 0..2 {
                let _ = compilation.syntax_tree();

                compilation
                    .product_source_graph()
                    .unwrap_or_else(|error| panic!("source graph must build: {error:?}"));

                compilation
                    .symbol_graph()
                    .unwrap_or_else(|error| panic!("symbol graph must build: {error:?}"));
            }

            let report = compilation
                .profile_report()
                .unwrap_or_else(|| panic!("profiled compilation must retain a report"));

            for name in [
                "syntax_tree",
                "product_source_graph",
                "discovery_symbol_graph",
                "symbol_graph",
            ] {
                let requests = report
                    .queries
                    .iter()
                    .find(|query| {
                        report
                            .query_descriptor(query.id)
                            .is_some_and(|descriptor| descriptor.name == name)
                    })
                    .map(|query| query.requests);

                assert_eq!(requests, Some(1), "unexpected request count for {name}");
            }

            assert!(report.descriptors.queries.iter().all(|descriptor| {
                !matches!(
                    descriptor.name.as_str(),
                    "selected_target" | "compiler_known_symbols"
                )
            }));
        }

        fn revision(text: &str, identity: u32, version: u64) -> SourceInput {
            SourceInput::virtual_text(
                SourceIdentity::new(identity),
                format!("source-{identity}"),
                SourceVersion::new(version),
                text,
            )
        }

        #[test]
        fn profile_reports_round_trip_through_the_machine_schema() {
            for mode in [
                CompilationProfileMode::Summary,
                CompilationProfileMode::Trace,
            ] {
                let request = CompilationRequest::new(
                    package_identity(),
                    vec![source_input("module test.package;\n", 1)],
                )
                .with_profile(CompilationProfileConfiguration::new(mode));

                let compilation = Compilation::load(request)
                    .unwrap_or_else(|error| panic!("test compilation must load: {error:?}"));

                let _ = compilation.syntax_tree_result();

                let report = compilation
                    .profile_report()
                    .unwrap_or_else(|| panic!("profiled compilation must retain a report"));

                let encoded = serde_json::to_vec(&report)
                    .unwrap_or_else(|error| panic!("profile report must serialize: {error:?}"));

                let decoded = serde_json::from_slice(&encoded)
                    .unwrap_or_else(|error| panic!("profile report must deserialize: {error:?}"));

                assert_eq!(report, decoded);
            }
        }
    }
}
