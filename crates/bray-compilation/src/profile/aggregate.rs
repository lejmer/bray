use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard};
use std::thread::ThreadId;

use bray_profile::CompilationProfileOutcome;

use super::descriptor::{ProfileMetricKind, ProfileOperation, ProfileQueryKind};
use super::distribution::{CountHistogram, DurationHistogram};
use super::subject::ProfileSubjectRecord;

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ProfileAggregate {
    pub(super) executions: u64,
    pub(super) completed: u64,
    pub(super) failed: u64,
    pub(super) cancelled: u64,
    pub(super) abandoned: u64,
    pub(super) total_nanoseconds: u64,
    pub(super) self_nanoseconds: u64,
    pub(super) maximum_nanoseconds: u64,
}

impl ProfileAggregate {
    pub(super) fn record(
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

        self.self_nanoseconds = self.self_nanoseconds.saturating_add(other.self_nanoseconds);
        self.maximum_nanoseconds = self.maximum_nanoseconds.max(other.maximum_nanoseconds);
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ProfileQueryAggregate {
    pub(super) requests: u64,
    pub(super) cache_hits: u64,
    pub(super) cache_misses: u64,
    pub(super) cross_snapshot_reuses: u64,
    pub(super) invalidations: u64,
    pub(super) evaluations: u64,
    pub(super) waits: u64,
    pub(super) evaluation_nanoseconds: u64,
    pub(super) evaluation_self_nanoseconds: u64,
    pub(super) evaluation_latency: DurationHistogram,
    pub(super) wait_nanoseconds: u64,
    pub(super) ready_value_nanoseconds: u64,
    pub(super) ready_value_maximum_nanoseconds: u64,
    pub(super) published_values: u64,
    pub(super) published_inline_bytes: u64,
    pub(super) diagnostic_collections: u64,
    pub(super) result_diagnostics: u64,
    pub(super) cloned_values: u64,
    pub(super) cloned_inline_bytes: u64,
    pub(super) diagnostic_copies: u64,
    pub(super) cloned_diagnostics: u64,
    pub(super) diagnostic_merges: u64,
    pub(super) merged_diagnostics: u64,
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

        self.evaluation_self_nanoseconds = self
            .evaluation_self_nanoseconds
            .saturating_add(other.evaluation_self_nanoseconds);

        self.evaluation_latency.merge(&other.evaluation_latency);
        self.wait_nanoseconds = self.wait_nanoseconds.saturating_add(other.wait_nanoseconds);

        self.ready_value_nanoseconds = self
            .ready_value_nanoseconds
            .saturating_add(other.ready_value_nanoseconds);

        self.ready_value_maximum_nanoseconds = self
            .ready_value_maximum_nanoseconds
            .max(other.ready_value_maximum_nanoseconds);

        self.published_values = self.published_values.saturating_add(other.published_values);

        self.published_inline_bytes = self
            .published_inline_bytes
            .saturating_add(other.published_inline_bytes);

        self.diagnostic_collections = self
            .diagnostic_collections
            .saturating_add(other.diagnostic_collections);

        self.result_diagnostics = self
            .result_diagnostics
            .saturating_add(other.result_diagnostics);

        self.cloned_values = self.cloned_values.saturating_add(other.cloned_values);

        self.cloned_inline_bytes = self
            .cloned_inline_bytes
            .saturating_add(other.cloned_inline_bytes);

        self.diagnostic_copies = self
            .diagnostic_copies
            .saturating_add(other.diagnostic_copies);

        self.cloned_diagnostics = self
            .cloned_diagnostics
            .saturating_add(other.cloned_diagnostics);

        self.diagnostic_merges = self
            .diagnostic_merges
            .saturating_add(other.diagnostic_merges);

        self.merged_diagnostics = self
            .merged_diagnostics
            .saturating_add(other.merged_diagnostics);
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ProfileSchedulerAggregate {
    pub(super) active_worker_nanoseconds: u64,
    pub(super) ready_waves: u64,
    pub(super) ready_items: u64,
    pub(super) maximum_ready_width: u64,
}

impl ProfileSchedulerAggregate {
    pub(super) fn merge(&mut self, other: Self) {
        self.active_worker_nanoseconds = self
            .active_worker_nanoseconds
            .saturating_add(other.active_worker_nanoseconds);

        self.ready_waves = self.ready_waves.saturating_add(other.ready_waves);
        self.ready_items = self.ready_items.saturating_add(other.ready_items);
        self.maximum_ready_width = self.maximum_ready_width.max(other.maximum_ready_width);
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct ProfileSchedulingWaveKey {
    pub(super) operation_id: Option<u16>,
    pub(super) query_id: Option<u16>,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ProfileSchedulingWaveAggregate {
    pub(super) waves: u64,
    pub(super) planned_items: u64,
    pub(super) ready_items: u64,
    pub(super) ready_width: CountHistogram,
    pub(super) active_workers: CountHistogram,
}

impl ProfileSchedulingWaveAggregate {
    pub(super) fn record(&mut self, planned: u64, ready: u64, active_workers: u64) {
        self.waves = self.waves.saturating_add(1);
        self.planned_items = self.planned_items.saturating_add(planned);
        self.ready_items = self.ready_items.saturating_add(ready);
        self.ready_width.record(ready);
        self.active_workers.record(active_workers);
    }

    pub(super) fn merge(&mut self, other: Self) {
        self.waves = self.waves.saturating_add(other.waves);
        self.planned_items = self.planned_items.saturating_add(other.planned_items);
        self.ready_items = self.ready_items.saturating_add(other.ready_items);
        self.ready_width.merge(&other.ready_width);
        self.active_workers.merge(&other.active_workers);
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ProfileEventRecord {
    pub(super) started_at: u64,
    pub(super) duration: u64,
    pub(super) operation: ProfileOperation,
    pub(super) query: Option<ProfileQueryKind>,
    pub(super) subject: Option<ProfileSubjectRecord>,
    pub(super) outcome: CompilationProfileOutcome,
    pub(super) worker: u32,
    pub(super) sequence: u64,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ActiveSpan {
    pub(super) id: u64,
    pub(super) thread: ThreadId,
    pub(super) operation: ProfileOperation,
    pub(super) query: Option<ProfileQueryKind>,
    pub(super) child_nanoseconds: u64,
}

#[derive(Debug)]
pub(super) struct ProfileShard {
    pub(super) operations: [ProfileAggregate; ProfileOperation::COUNT],
    pub(super) queries: [ProfileQueryAggregate; ProfileQueryKind::COUNT],
    pub(super) metrics: [u64; ProfileMetricKind::COUNT],
    pub(super) scheduler: ProfileSchedulerAggregate,
    pub(super) scheduling_waves:
        BTreeMap<ProfileSchedulingWaveKey, ProfileSchedulingWaveAggregate>,
    pub(super) events: Vec<ProfileEventRecord>,
    pub(super) spans: Vec<ActiveSpan>,
    pub(super) next_span: u64,
    pub(super) next_sequence: u64,
    pub(super) dropped_events: u64,
}

impl ProfileShard {
    pub(super) fn new(trace_capacity: usize) -> Self {
        Self {
            operations: [ProfileAggregate::default(); ProfileOperation::COUNT],
            queries: [ProfileQueryAggregate::default(); ProfileQueryKind::COUNT],
            metrics: [0; ProfileMetricKind::COUNT],
            scheduler: ProfileSchedulerAggregate::default(),
            scheduling_waves: BTreeMap::new(),
            events: Vec::with_capacity(trace_capacity),
            spans: Vec::new(),
            next_span: 0,
            next_sequence: 0,
            dropped_events: 0,
        }
    }
}

pub(super) fn available_shard(shard: &Mutex<ProfileShard>) -> MutexGuard<'_, ProfileShard> {
    shard
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub(super) fn finish_active_span(
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

pub(super) fn merge_aggregates(
    destination: &mut [ProfileAggregate; ProfileOperation::COUNT],
    source: &[ProfileAggregate; ProfileOperation::COUNT],
) {
    for (destination, source) in destination.iter_mut().zip(source) {
        destination.merge(*source);
    }
}

pub(super) fn merge_queries(
    destination: &mut [ProfileQueryAggregate; ProfileQueryKind::COUNT],
    source: &[ProfileQueryAggregate; ProfileQueryKind::COUNT],
) {
    for (destination, source) in destination.iter_mut().zip(source) {
        destination.merge(*source);
    }
}

pub(super) fn merge_metrics(
    destination: &mut [u64; ProfileMetricKind::COUNT],
    source: &[u64; ProfileMetricKind::COUNT],
) {
    for metric in ProfileMetricKind::all() {
        let index = metric.index();

        destination[index] = metric.merge(destination[index], source[index]);
    }
}
