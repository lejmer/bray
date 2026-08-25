use std::collections::BTreeSet;

use crate::{
    CompilationProfileContext, CompilationProfileMetric, CompilationProfileMetricDescriptor,
    CompilationProfileOperationDescriptor, CompilationProfileOperationStatistics,
    CompilationProfileQueryDescriptor, CompilationProfileQueryStatistics, CompilationProfileReport,
};

/// Aggregate query behavior across one report.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CompilationProfileQueryTotals {
    /// Total query requests.
    pub requests: u64,
    /// Total query evaluations.
    pub evaluations: u64,
    /// Total requests served from an already-published value.
    pub cache_hits: u64,
    /// Total requests that found no published value.
    pub cache_misses: u64,
    /// Total waits for evaluation by another task.
    pub waits: u64,
    /// Total cross-snapshot reused values.
    pub cross_snapshot_reuses: u64,
    /// Total invalidated preceding-snapshot values.
    pub invalidations: u64,
    /// Total time spent reaching already-published values.
    pub ready_value_nanoseconds: u64,
    /// Total immutable values published by query cells.
    pub published_values: u64,
    /// Total inline bytes occupied by published query-cell values.
    pub published_inline_bytes: u64,
    /// Total query-result diagnostic collections observed at publication.
    pub diagnostic_collections: u64,
    /// Total diagnostic instances attached to computed query results.
    pub result_diagnostics: u64,
    /// Total query-result ownership handles copied for callers.
    pub cloned_values: u64,
    /// Total inline bytes occupied by copied query-result ownership handles.
    pub cloned_inline_bytes: u64,
    /// Total query-result diagnostic collections copied for callers.
    pub diagnostic_copies: u64,
    /// Total diagnostic instances carried by copied query results.
    pub cloned_diagnostics: u64,
    /// Total attributed diagnostic merge boundaries.
    pub diagnostic_merges: u64,
    /// Total diagnostic inputs supplied across attributed merge boundaries.
    pub merged_diagnostics: u64,
}

/// Presentation-neutral analysis of one compiler profile.
#[derive(Clone, Copy, Debug)]
pub struct CompilationProfileSummary<'profile> {
    report: &'profile CompilationProfileReport,
}

impl<'profile> CompilationProfileSummary<'profile> {
    /// Creates an analysis view over one report.
    pub const fn new(report: &'profile CompilationProfileReport) -> Self {
        Self { report }
    }

    /// Returns the underlying report.
    pub const fn report(self) -> &'profile CompilationProfileReport {
        self.report
    }

    /// Aggregates query counts across every observed query kind.
    pub fn query_totals(self) -> CompilationProfileQueryTotals {
        self.report.queries.iter().fold(
            CompilationProfileQueryTotals::default(),
            |mut totals, query| {
                totals.requests = totals.requests.saturating_add(query.requests);
                totals.evaluations = totals.evaluations.saturating_add(query.evaluations);
                totals.cache_hits = totals.cache_hits.saturating_add(query.cache_hits);
                totals.cache_misses = totals.cache_misses.saturating_add(query.cache_misses);
                totals.waits = totals.waits.saturating_add(query.waits);

                totals.cross_snapshot_reuses = totals
                    .cross_snapshot_reuses
                    .saturating_add(query.cross_snapshot_reuses);

                totals.invalidations = totals.invalidations.saturating_add(query.invalidations);

                totals.ready_value_nanoseconds = totals
                    .ready_value_nanoseconds
                    .saturating_add(query.ready_value_nanoseconds);

                totals.published_values = totals
                    .published_values
                    .saturating_add(query.published_values);

                totals.published_inline_bytes = totals
                    .published_inline_bytes
                    .saturating_add(query.published_inline_bytes);

                totals.diagnostic_collections = totals
                    .diagnostic_collections
                    .saturating_add(query.diagnostic_collections);

                totals.result_diagnostics = totals
                    .result_diagnostics
                    .saturating_add(query.result_diagnostics);

                totals.cloned_values = totals.cloned_values.saturating_add(query.cloned_values);

                totals.cloned_inline_bytes = totals
                    .cloned_inline_bytes
                    .saturating_add(query.cloned_inline_bytes);

                totals.diagnostic_copies = totals
                    .diagnostic_copies
                    .saturating_add(query.diagnostic_copies);

                totals.cloned_diagnostics = totals
                    .cloned_diagnostics
                    .saturating_add(query.cloned_diagnostics);

                totals.diagnostic_merges = totals
                    .diagnostic_merges
                    .saturating_add(query.diagnostic_merges);

                totals.merged_diagnostics = totals
                    .merged_diagnostics
                    .saturating_add(query.merged_diagnostics);

                totals
            },
        )
    }

    /// Returns observed operations ranked by descending same-thread self time.
    pub fn top_operations(
        self,
        limit: usize,
    ) -> Vec<(
        &'profile CompilationProfileOperationDescriptor,
        &'profile CompilationProfileOperationStatistics,
    )> {
        let mut operations = self
            .report
            .operations
            .iter()
            .filter_map(|statistics| {
                self.report
                    .operation_descriptor(statistics.id)
                    .map(|descriptor| (descriptor, statistics))
            })
            .collect::<Vec<_>>();

        operations.sort_by(|left, right| {
            right
                .1
                .self_nanoseconds
                .cmp(&left.1.self_nanoseconds)
                .then_with(|| right.1.total_nanoseconds.cmp(&left.1.total_nanoseconds))
                .then_with(|| left.0.id.cmp(&right.0.id))
        });

        operations.truncate(limit);

        operations
    }

    /// Returns observed queries ranked by descending same-thread evaluation self time.
    pub fn top_queries(
        self,
        limit: usize,
    ) -> Vec<(
        &'profile CompilationProfileQueryDescriptor,
        &'profile CompilationProfileQueryStatistics,
    )> {
        let mut queries = self
            .report
            .queries
            .iter()
            .filter_map(|statistics| {
                self.report
                    .query_descriptor(statistics.id)
                    .map(|descriptor| (descriptor, statistics))
            })
            .collect::<Vec<_>>();

        queries.sort_by(|left, right| {
            right
                .1
                .evaluation_self_nanoseconds
                .cmp(&left.1.evaluation_self_nanoseconds)
                .then_with(|| {
                    right
                        .1
                        .evaluation_nanoseconds
                        .cmp(&left.1.evaluation_nanoseconds)
                })
                .then_with(|| right.1.wait_nanoseconds.cmp(&left.1.wait_nanoseconds))
                .then_with(|| left.0.id.cmp(&right.0.id))
        });

        queries.truncate(limit);

        queries
    }

    /// Returns query kinds ranked by time spent reaching already-published values.
    pub fn top_ready_queries(
        self,
        limit: usize,
    ) -> Vec<(
        &'profile CompilationProfileQueryDescriptor,
        &'profile CompilationProfileQueryStatistics,
    )> {
        let mut queries = self
            .report
            .queries
            .iter()
            .filter(|statistics| statistics.cache_hits > 0)
            .filter_map(|statistics| {
                self.report
                    .query_descriptor(statistics.id)
                    .map(|descriptor| (descriptor, statistics))
            })
            .collect::<Vec<_>>();

        queries.sort_by(|left, right| {
            right
                .1
                .ready_value_nanoseconds
                .cmp(&left.1.ready_value_nanoseconds)
                .then_with(|| right.1.cache_hits.cmp(&left.1.cache_hits))
                .then_with(|| left.0.id.cmp(&right.0.id))
        });

        queries.truncate(limit);

        queries
    }

    /// Returns query kinds ranked by result publication and ownership-copy volume.
    pub fn top_query_propagation(
        self,
        limit: usize,
    ) -> Vec<(
        &'profile CompilationProfileQueryDescriptor,
        &'profile CompilationProfileQueryStatistics,
    )> {
        let mut queries = self
            .report
            .queries
            .iter()
            .filter(|statistics| statistics.published_values > 0 || statistics.cloned_values > 0)
            .filter_map(|statistics| {
                self.report
                    .query_descriptor(statistics.id)
                    .map(|descriptor| (descriptor, statistics))
            })
            .collect::<Vec<_>>();

        queries.sort_by(|left, right| {
            result_propagation_volume(right.1)
                .cmp(&result_propagation_volume(left.1))
                .then_with(|| left.0.id.cmp(&right.0.id))
        });

        queries.truncate(limit);

        queries
    }

    /// Returns query kinds ranked by diagnostic collection, copy, and merge volume.
    pub fn top_query_diagnostics(
        self,
        limit: usize,
    ) -> Vec<(
        &'profile CompilationProfileQueryDescriptor,
        &'profile CompilationProfileQueryStatistics,
    )> {
        let mut queries = self
            .report
            .queries
            .iter()
            .filter(|statistics| {
                statistics.diagnostic_collections > 0
                    || statistics.diagnostic_copies > 0
                    || statistics.diagnostic_merges > 0
            })
            .filter_map(|statistics| {
                self.report
                    .query_descriptor(statistics.id)
                    .map(|descriptor| (descriptor, statistics))
            })
            .collect::<Vec<_>>();

        queries.sort_by(|left, right| {
            diagnostic_instance_volume(right.1)
                .cmp(&diagnostic_instance_volume(left.1))
                .then_with(|| right.1.diagnostic_merges.cmp(&left.1.diagnostic_merges))
                .then_with(|| {
                    diagnostic_event_volume(right.1).cmp(&diagnostic_event_volume(left.1))
                })
                .then_with(|| left.0.id.cmp(&right.0.id))
        });

        queries.truncate(limit);

        queries
    }

    /// Returns all nonzero metrics in stable descriptor order.
    pub fn metrics(
        self,
    ) -> Vec<(
        &'profile CompilationProfileMetricDescriptor,
        &'profile CompilationProfileMetric,
    )> {
        let mut metrics = self
            .report
            .metrics
            .iter()
            .filter_map(|metric| {
                self.report
                    .metric_descriptor(metric.id)
                    .map(|descriptor| (descriptor, metric))
            })
            .collect::<Vec<_>>();

        metrics.sort_by_key(|(descriptor, _)| descriptor.id);

        metrics
    }
}

/// Change in one operation aggregate between two reports.
#[derive(Clone, Copy, Debug)]
pub struct CompilationProfileOperationChange<'profile> {
    /// Descriptor from the newer report, or the older report if the kind disappeared.
    pub descriptor: &'profile CompilationProfileOperationDescriptor,
    /// Older same-thread self time.
    pub before_self_nanoseconds: u64,
    /// Newer same-thread self time.
    pub after_self_nanoseconds: u64,
    /// Older execution count.
    pub before_executions: u64,
    /// Newer execution count.
    pub after_executions: u64,
}

/// Change in one query aggregate between two reports.
#[derive(Clone, Copy, Debug)]
pub struct CompilationProfileQueryChange<'profile> {
    /// Descriptor from the newer report, or the older report if the kind disappeared.
    pub descriptor: &'profile CompilationProfileQueryDescriptor,
    /// Older evaluation time.
    pub before_evaluation_nanoseconds: u64,
    /// Newer evaluation time.
    pub after_evaluation_nanoseconds: u64,
    /// Older same-thread evaluation self time.
    pub before_evaluation_self_nanoseconds: u64,
    /// Newer same-thread evaluation self time.
    pub after_evaluation_self_nanoseconds: u64,
    /// Older wait time.
    pub before_wait_nanoseconds: u64,
    /// Newer wait time.
    pub after_wait_nanoseconds: u64,
    /// Older time spent reaching already-published values.
    pub before_ready_value_nanoseconds: u64,
    /// Newer time spent reaching already-published values.
    pub after_ready_value_nanoseconds: u64,
}

/// Change in one metric between two reports.
#[derive(Clone, Copy, Debug)]
pub struct CompilationProfileMetricChange<'profile> {
    /// Descriptor from the newer report, or the older report if the kind disappeared.
    pub descriptor: &'profile CompilationProfileMetricDescriptor,
    /// Older metric value.
    pub before: u64,
    /// Newer metric value.
    pub after: u64,
}

/// Presentation-neutral comparison of two compiler profiles.
#[derive(Clone, Copy, Debug)]
pub struct CompilationProfileComparison<'profile> {
    before: &'profile CompilationProfileReport,
    after: &'profile CompilationProfileReport,
}

/// Reason two valid compiler profiles cannot be compared meaningfully.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CompilationProfileComparisonError {
    /// Package, product, or target identity differs.
    Context {
        before: Box<CompilationProfileContext>,
        after: Box<CompilationProfileContext>,
    },
    /// A shared descriptor identity has different metadata.
    Descriptor {
        kind: crate::CompilationProfileDescriptorKind,
        id: u16,
    },
}

impl<'profile> CompilationProfileComparison<'profile> {
    /// Creates a comparison view over an older and newer report.
    pub fn new(
        before: &'profile CompilationProfileReport,
        after: &'profile CompilationProfileReport,
    ) -> Result<Self, CompilationProfileComparisonError> {
        if before.context != after.context {
            return Err(CompilationProfileComparisonError::Context {
                before: Box::new(before.context.clone()),
                after: Box::new(after.context.clone()),
            });
        }

        if let Some((kind, id)) = incompatible_descriptor(before, after) {
            return Err(CompilationProfileComparisonError::Descriptor { kind, id });
        }

        Ok(Self { before, after })
    }

    /// Returns the older report.
    pub const fn before(self) -> &'profile CompilationProfileReport {
        self.before
    }

    /// Returns the newer report.
    pub const fn after(self) -> &'profile CompilationProfileReport {
        self.after
    }

    /// Returns operation changes ranked by descending absolute self-time change.
    pub fn top_operation_changes(
        self,
        limit: usize,
    ) -> Vec<CompilationProfileOperationChange<'profile>> {
        let mut changes = operation_ids(self.before, self.after)
            .into_iter()
            .filter_map(|id| operation_change(self.before, self.after, id))
            .filter(|change| {
                change.before_self_nanoseconds != change.after_self_nanoseconds
                    || change.before_executions != change.after_executions
            })
            .collect::<Vec<_>>();

        changes.sort_by(|left, right| {
            absolute_difference(right.before_self_nanoseconds, right.after_self_nanoseconds)
                .cmp(&absolute_difference(
                    left.before_self_nanoseconds,
                    left.after_self_nanoseconds,
                ))
                .then_with(|| left.descriptor.id.cmp(&right.descriptor.id))
        });

        changes.truncate(limit);

        changes
    }

    /// Returns query changes ranked by descending absolute evaluation self-time change.
    pub fn top_query_changes(self, limit: usize) -> Vec<CompilationProfileQueryChange<'profile>> {
        let mut changes = query_ids(self.before, self.after)
            .into_iter()
            .filter_map(|id| query_change(self.before, self.after, id))
            .filter(|change| {
                change.before_evaluation_self_nanoseconds
                    != change.after_evaluation_self_nanoseconds
                    || change.before_evaluation_nanoseconds != change.after_evaluation_nanoseconds
                    || change.before_wait_nanoseconds != change.after_wait_nanoseconds
                    || change.before_ready_value_nanoseconds != change.after_ready_value_nanoseconds
            })
            .collect::<Vec<_>>();

        changes.sort_by(|left, right| {
            absolute_difference(
                right.before_evaluation_self_nanoseconds,
                right.after_evaluation_self_nanoseconds,
            )
            .cmp(&absolute_difference(
                left.before_evaluation_self_nanoseconds,
                left.after_evaluation_self_nanoseconds,
            ))
            .then_with(|| left.descriptor.id.cmp(&right.descriptor.id))
        });

        changes.truncate(limit);

        changes
    }

    /// Returns changed metrics in stable descriptor order.
    pub fn metric_changes(self) -> Vec<CompilationProfileMetricChange<'profile>> {
        metric_ids(self.before, self.after)
            .into_iter()
            .filter_map(|id| metric_change(self.before, self.after, id))
            .filter(|change| change.before != change.after)
            .collect()
    }
}

fn incompatible_descriptor(
    before: &CompilationProfileReport,
    after: &CompilationProfileReport,
) -> Option<(crate::CompilationProfileDescriptorKind, u16)> {
    for descriptor in before
        .descriptors
        .operations
        .iter()
        .chain(&after.descriptors.operations)
    {
        if before
            .operation_descriptor(descriptor.id)
            .zip(after.operation_descriptor(descriptor.id))
            .is_some_and(|(left, right)| left != right)
        {
            return Some((
                crate::CompilationProfileDescriptorKind::Operation,
                descriptor.id,
            ));
        }
    }

    for descriptor in before
        .descriptors
        .queries
        .iter()
        .chain(&after.descriptors.queries)
    {
        if before
            .query_descriptor(descriptor.id)
            .zip(after.query_descriptor(descriptor.id))
            .is_some_and(|(left, right)| left != right)
        {
            return Some((
                crate::CompilationProfileDescriptorKind::Query,
                descriptor.id,
            ));
        }
    }

    for descriptor in before
        .descriptors
        .metrics
        .iter()
        .chain(&after.descriptors.metrics)
    {
        if before
            .metric_descriptor(descriptor.id)
            .zip(after.metric_descriptor(descriptor.id))
            .is_some_and(|(left, right)| left != right)
        {
            return Some((
                crate::CompilationProfileDescriptorKind::Metric,
                descriptor.id,
            ));
        }
    }

    None
}

fn operation_ids(
    before: &CompilationProfileReport,
    after: &CompilationProfileReport,
) -> BTreeSet<u16> {
    before
        .operations
        .iter()
        .chain(&after.operations)
        .map(|entry| entry.id)
        .collect()
}

fn query_ids(before: &CompilationProfileReport, after: &CompilationProfileReport) -> BTreeSet<u16> {
    before
        .queries
        .iter()
        .chain(&after.queries)
        .map(|entry| entry.id)
        .collect()
}

fn metric_ids(
    before: &CompilationProfileReport,
    after: &CompilationProfileReport,
) -> BTreeSet<u16> {
    before
        .metrics
        .iter()
        .chain(&after.metrics)
        .map(|entry| entry.id)
        .collect()
}

fn operation_change<'profile>(
    before: &'profile CompilationProfileReport,
    after: &'profile CompilationProfileReport,
    id: u16,
) -> Option<CompilationProfileOperationChange<'profile>> {
    let before_statistics = before.operations.iter().find(|entry| entry.id == id);
    let after_statistics = after.operations.iter().find(|entry| entry.id == id);

    let descriptor = after
        .operation_descriptor(id)
        .or_else(|| before.operation_descriptor(id))?;

    Some(CompilationProfileOperationChange {
        descriptor,
        before_self_nanoseconds: before_statistics.map_or(0, |entry| entry.self_nanoseconds),
        after_self_nanoseconds: after_statistics.map_or(0, |entry| entry.self_nanoseconds),
        before_executions: before_statistics.map_or(0, |entry| entry.executions),
        after_executions: after_statistics.map_or(0, |entry| entry.executions),
    })
}

fn query_change<'profile>(
    before: &'profile CompilationProfileReport,
    after: &'profile CompilationProfileReport,
    id: u16,
) -> Option<CompilationProfileQueryChange<'profile>> {
    let before_statistics = before.queries.iter().find(|entry| entry.id == id);
    let after_statistics = after.queries.iter().find(|entry| entry.id == id);

    let descriptor = after
        .query_descriptor(id)
        .or_else(|| before.query_descriptor(id))?;

    Some(CompilationProfileQueryChange {
        descriptor,
        before_evaluation_nanoseconds: before_statistics
            .map_or(0, |entry| entry.evaluation_nanoseconds),
        after_evaluation_nanoseconds: after_statistics
            .map_or(0, |entry| entry.evaluation_nanoseconds),
        before_evaluation_self_nanoseconds: before_statistics
            .map_or(0, |entry| entry.evaluation_self_nanoseconds),
        after_evaluation_self_nanoseconds: after_statistics
            .map_or(0, |entry| entry.evaluation_self_nanoseconds),
        before_wait_nanoseconds: before_statistics.map_or(0, |entry| entry.wait_nanoseconds),
        after_wait_nanoseconds: after_statistics.map_or(0, |entry| entry.wait_nanoseconds),
        before_ready_value_nanoseconds: before_statistics
            .map_or(0, |entry| entry.ready_value_nanoseconds),
        after_ready_value_nanoseconds: after_statistics
            .map_or(0, |entry| entry.ready_value_nanoseconds),
    })
}

fn metric_change<'profile>(
    before: &'profile CompilationProfileReport,
    after: &'profile CompilationProfileReport,
    id: u16,
) -> Option<CompilationProfileMetricChange<'profile>> {
    let descriptor = after
        .metric_descriptor(id)
        .or_else(|| before.metric_descriptor(id))?;

    Some(CompilationProfileMetricChange {
        descriptor,
        before: before
            .metrics
            .iter()
            .find(|entry| entry.id == id)
            .map_or(0, |entry| entry.value),
        after: after
            .metrics
            .iter()
            .find(|entry| entry.id == id)
            .map_or(0, |entry| entry.value),
    })
}

const fn absolute_difference(left: u64, right: u64) -> u64 {
    left.abs_diff(right)
}

const fn result_propagation_volume(statistics: &CompilationProfileQueryStatistics) -> u64 {
    statistics
        .cloned_values
        .saturating_add(statistics.published_values)
}

const fn diagnostic_instance_volume(statistics: &CompilationProfileQueryStatistics) -> u64 {
    statistics
        .result_diagnostics
        .saturating_add(statistics.cloned_diagnostics)
        .saturating_add(statistics.merged_diagnostics)
}

const fn diagnostic_event_volume(statistics: &CompilationProfileQueryStatistics) -> u64 {
    statistics
        .diagnostic_collections
        .saturating_add(statistics.diagnostic_copies)
        .saturating_add(statistics.diagnostic_merges)
}

#[cfg(test)]
mod tests {
    use super::{
        CompilationProfileComparison, CompilationProfileComparisonError, CompilationProfileSummary,
    };
    use crate::test_support::report;

    #[test]
    fn comparisons_require_matching_context_and_rank_changed_work() {
        let before = report(1_000_000);
        let after = report(1_500_000);

        let comparison = CompilationProfileComparison::new(&before, &after)
            .unwrap_or_else(|error| panic!("matching profiles must compare: {error:?}"));

        let operation = comparison
            .top_operation_changes(1)
            .into_iter()
            .next()
            .unwrap_or_else(|| panic!("changed operation must be ranked"));

        assert_eq!(operation.before_self_nanoseconds, 1_000_000);
        assert_eq!(operation.after_self_nanoseconds, 1_500_000);

        let query = comparison
            .top_query_changes(1)
            .into_iter()
            .next()
            .unwrap_or_else(|| panic!("changed query must be ranked"));

        assert_eq!(query.before_evaluation_self_nanoseconds, 1_000_000);
        assert_eq!(query.after_evaluation_self_nanoseconds, 1_500_000);

        let mut other_target = report(1_500_000);
        other_target.context.target = "aarch64-test".to_owned();

        assert!(matches!(
            CompilationProfileComparison::new(&before, &other_target),
            Err(CompilationProfileComparisonError::Context { .. })
        ));
    }

    #[test]
    fn result_and_diagnostic_rankings_preserve_distinct_domains() {
        let mut report = report(1_000_000);
        report.queries[0].cloned_values = 100;
        report.queries[0].diagnostic_collections = 0;

        let mut diagnostic_descriptor = report.descriptors.queries[0].clone();
        diagnostic_descriptor.id = 1_001;
        diagnostic_descriptor.name = "check_diagnostics".to_owned();

        let mut diagnostic_query = report.queries[0].clone();
        diagnostic_query.id = 1_001;
        diagnostic_query.published_values = 0;
        diagnostic_query.cloned_values = 0;
        diagnostic_query.diagnostic_collections = 1;
        diagnostic_query.diagnostic_merges = 1;

        report.descriptors.queries.push(diagnostic_descriptor);
        report.queries.push(diagnostic_query);

        let summary = CompilationProfileSummary::new(&report);

        assert_eq!(summary.top_query_propagation(1)[0].0.id, 1_000);
        assert_eq!(summary.top_query_diagnostics(1)[0].0.id, 1_001);
    }
}
