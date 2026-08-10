use crate::{
    COMPILATION_PROFILE_SCHEMA_REVISION, CompilationProfileAggregation, CompilationProfileCategory,
    CompilationProfileContext, CompilationProfileDescriptorCatalog, CompilationProfileMetric,
    CompilationProfileMetricDescriptor, CompilationProfileMode,
    CompilationProfileOperationDescriptor, CompilationProfileOperationStatistics,
    CompilationProfileQueryDescriptor, CompilationProfileQueryStatistics, CompilationProfileReport,
    CompilationProfileTimeBreakdown, CompilationProfileUnit,
};

pub(crate) fn report(elapsed_nanoseconds: u64) -> CompilationProfileReport {
    CompilationProfileReport {
        schema_revision: COMPILATION_PROFILE_SCHEMA_REVISION,
        mode: CompilationProfileMode::Summary,
        context: CompilationProfileContext {
            package: "example".to_owned(),
            product: "application".to_owned(),
            target: "x86_64-test".to_owned(),
        },
        trace_event_limit: None,
        elapsed_nanoseconds,
        time: CompilationProfileTimeBreakdown {
            active_work_nanoseconds: elapsed_nanoseconds,
            same_thread_self_nanoseconds: elapsed_nanoseconds,
            scheduler_queue_nanoseconds: 0,
            dependency_wait_nanoseconds: 0,
            external_work_nanoseconds: 0,
        },
        descriptors: CompilationProfileDescriptorCatalog {
            operations: vec![CompilationProfileOperationDescriptor {
                id: 1,
                name: "compiler.query.evaluate".to_owned(),
                category: CompilationProfileCategory::Work,
                unit: CompilationProfileUnit::Nanoseconds,
                aggregation: CompilationProfileAggregation::SumAndMaximum,
                allowed_subjects: Vec::new(),
            }],
            queries: vec![CompilationProfileQueryDescriptor {
                id: 1_000,
                name: "syntax_tree".to_owned(),
            }],
            metrics: vec![CompilationProfileMetricDescriptor {
                id: 2_000,
                name: "compiler.source.units".to_owned(),
                unit: CompilationProfileUnit::Count,
                category: CompilationProfileCategory::Measurement,
                aggregation: CompilationProfileAggregation::Sum,
                allowed_subjects: Vec::new(),
            }],
        },
        operations: vec![CompilationProfileOperationStatistics {
            id: 1,
            executions: 1,
            completed: 1,
            failed: 0,
            cancelled: 0,
            abandoned: 0,
            total_nanoseconds: elapsed_nanoseconds,
            self_nanoseconds: elapsed_nanoseconds,
            maximum_nanoseconds: elapsed_nanoseconds,
        }],
        queries: vec![CompilationProfileQueryStatistics {
            id: 1_000,
            requests: 2,
            cache_hits: 1,
            cache_misses: 1,
            cross_snapshot_reuses: 0,
            invalidations: 0,
            evaluations: 1,
            waits: 0,
            evaluation_nanoseconds: elapsed_nanoseconds,
            wait_nanoseconds: 0,
        }],
        metrics: vec![CompilationProfileMetric {
            id: 2_000,
            value: 1,
        }],
        events: Vec::new(),
        dropped_events: 0,
    }
}
