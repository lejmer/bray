use std::collections::BTreeSet;

use crate::{
    CompilationProfileMetric, CompilationProfileMetricDescriptor,
    CompilationProfileOperationDescriptor, CompilationProfileOperationStatistics,
    CompilationProfileQueryDescriptor, CompilationProfileQueryStatistics,
    CompilationProfileReport,
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

                totals.invalidations = totals
                    .invalidations
                    .saturating_add(query.invalidations);

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

    /// Returns observed queries ranked by descending evaluation time.
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
                .evaluation_nanoseconds
                .cmp(&left.1.evaluation_nanoseconds)
                .then_with(|| right.1.wait_nanoseconds.cmp(&left.1.wait_nanoseconds))
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
    /// Older wait time.
    pub before_wait_nanoseconds: u64,
    /// Newer wait time.
    pub after_wait_nanoseconds: u64,
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
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompilationProfileComparisonError {
    /// Package, product, or target identity differs.
    Context,
    /// A shared descriptor identity has different metadata.
    Descriptor,
}

impl<'profile> CompilationProfileComparison<'profile> {
    /// Creates a comparison view over an older and newer report.
    pub fn new(
        before: &'profile CompilationProfileReport,
        after: &'profile CompilationProfileReport,
    ) -> Result<Self, CompilationProfileComparisonError> {
        if before.context != after.context {
            return Err(CompilationProfileComparisonError::Context);
        }

        if !descriptors_are_compatible(before, after) {
            return Err(CompilationProfileComparisonError::Descriptor);
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

    /// Returns query changes ranked by descending absolute evaluation-time change.
    pub fn top_query_changes(
        self,
        limit: usize,
    ) -> Vec<CompilationProfileQueryChange<'profile>> {
        let mut changes = query_ids(self.before, self.after)
            .into_iter()
            .filter_map(|id| query_change(self.before, self.after, id))
            .filter(|change| {
                change.before_evaluation_nanoseconds != change.after_evaluation_nanoseconds
                    || change.before_wait_nanoseconds != change.after_wait_nanoseconds
            })
            .collect::<Vec<_>>();

        changes.sort_by(|left, right| {
            absolute_difference(
                right.before_evaluation_nanoseconds,
                right.after_evaluation_nanoseconds,
            )
            .cmp(&absolute_difference(
                left.before_evaluation_nanoseconds,
                left.after_evaluation_nanoseconds,
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

fn descriptors_are_compatible(
    before: &CompilationProfileReport,
    after: &CompilationProfileReport,
) -> bool {
    before.descriptors.operations.iter().all(|descriptor| {
        after
            .operation_descriptor(descriptor.id)
            .is_none_or(|candidate| candidate == descriptor)
    }) && before.descriptors.queries.iter().all(|descriptor| {
        after
            .query_descriptor(descriptor.id)
            .is_none_or(|candidate| candidate == descriptor)
    }) && before.descriptors.metrics.iter().all(|descriptor| {
        after
            .metric_descriptor(descriptor.id)
            .is_none_or(|candidate| candidate == descriptor)
    }) && after.descriptors.operations.iter().all(|descriptor| {
        before
            .operation_descriptor(descriptor.id)
            .is_none_or(|candidate| candidate == descriptor)
    }) && after.descriptors.queries.iter().all(|descriptor| {
        before
            .query_descriptor(descriptor.id)
            .is_none_or(|candidate| candidate == descriptor)
    }) && after.descriptors.metrics.iter().all(|descriptor| {
        before
            .metric_descriptor(descriptor.id)
            .is_none_or(|candidate| candidate == descriptor)
    })
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
    let descriptor = after.query_descriptor(id).or_else(|| before.query_descriptor(id))?;

    Some(CompilationProfileQueryChange {
        descriptor,
        before_evaluation_nanoseconds: before_statistics
            .map_or(0, |entry| entry.evaluation_nanoseconds),
        after_evaluation_nanoseconds: after_statistics
            .map_or(0, |entry| entry.evaluation_nanoseconds),
        before_wait_nanoseconds: before_statistics.map_or(0, |entry| entry.wait_nanoseconds),
        after_wait_nanoseconds: after_statistics.map_or(0, |entry| entry.wait_nanoseconds),
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
        before: before.metrics.iter().find(|entry| entry.id == id).map_or(0, |entry| entry.value),
        after: after.metrics.iter().find(|entry| entry.id == id).map_or(0, |entry| entry.value),
    })
}

const fn absolute_difference(left: u64, right: u64) -> u64 {
    left.abs_diff(right)
}

#[cfg(test)]
mod tests {
    use super::{CompilationProfileComparison, CompilationProfileComparisonError};
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

        let mut other_target = report(1_500_000);
        other_target.context.target = "aarch64-test".to_owned();

        assert!(matches!(
            CompilationProfileComparison::new(&before, &other_target),
            Err(CompilationProfileComparisonError::Context)
        ));
    }
}
