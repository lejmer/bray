use std::collections::BTreeMap;

use bray_profile::{
    CompilationProfileAggregation, CompilationProfileCategory, CompilationProfileDescriptorCatalog,
    CompilationProfileEvent, CompilationProfileMetric, CompilationProfileMetricDescriptor,
    CompilationProfileOperationDescriptor, CompilationProfileOperationStatistics,
    CompilationProfileQueryDescriptor, CompilationProfileQueryStatistics,
    CompilationProfileSchedulerStatistics, CompilationProfileSchedulingWaveStatistics,
    CompilationProfileSubject, CompilationProfileTimeBreakdown, CompilationProfileUnit,
};

use super::aggregate::{
    ProfileAggregate, ProfileEventRecord, ProfileQueryAggregate, ProfileSchedulerAggregate,
    ProfileSchedulingWaveAggregate, ProfileSchedulingWaveKey,
};
use super::concurrency::ProfileConcurrency;
use super::descriptor::{ProfileMetricKind, ProfileOperation, ProfileQueryKind};
pub(super) fn operation_reports(
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

pub(super) fn query_reports(
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
            diagnostic_collections: aggregate.diagnostic_collections,
            result_diagnostics: aggregate.result_diagnostics,
            cloned_values: aggregate.cloned_values,
            cloned_inline_bytes: aggregate.cloned_inline_bytes,
            diagnostic_copies: aggregate.diagnostic_copies,
            cloned_diagnostics: aggregate.cloned_diagnostics,
            diagnostic_merges: aggregate.diagnostic_merges,
            merged_diagnostics: aggregate.merged_diagnostics,
        })
        .collect()
}

pub(super) fn scheduler_report(
    aggregate: ProfileSchedulerAggregate,
    worker_budget: usize,
    maximum_active_workers: u64,
    queries: &[ProfileQueryAggregate; ProfileQueryKind::COUNT],
    wave_classes: BTreeMap<ProfileSchedulingWaveKey, ProfileSchedulingWaveAggregate>,
) -> CompilationProfileSchedulerStatistics {
    CompilationProfileSchedulerStatistics {
        worker_budget: u64::try_from(worker_budget).unwrap_or(u64::MAX),
        active_worker_nanoseconds: aggregate.active_worker_nanoseconds,
        maximum_active_workers,
        ready_waves: aggregate.ready_waves,
        ready_items: aggregate.ready_items,
        maximum_ready_width: aggregate.maximum_ready_width,
        wave_classes: wave_classes
            .into_iter()
            .map(
                |(key, aggregate)| CompilationProfileSchedulingWaveStatistics {
                    operation_id: key.operation_id,
                    query_id: key.query_id,
                    waves: aggregate.waves,
                    planned_items: aggregate.planned_items,
                    ready_items: aggregate.ready_items,
                    ready_width: aggregate.ready_width.report(),
                    active_workers: aggregate.active_workers.report(),
                },
            )
            .collect(),
        query_critical_path_nanoseconds: queries
            .iter()
            .map(|query| query.evaluation_latency.report().maximum_nanoseconds)
            .max()
            .unwrap_or(0),
    }
}

pub(super) fn metric_reports(
    values: &[u64; ProfileMetricKind::COUNT],
) -> Vec<CompilationProfileMetric> {
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

pub(super) fn event_reports(records: Vec<ProfileEventRecord>) -> Vec<CompilationProfileEvent> {
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

pub(super) fn descriptor_catalog() -> CompilationProfileDescriptorCatalog {
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

pub(super) fn time_breakdown(
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
