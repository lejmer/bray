use std::fmt::Write;
use std::time::Duration;

use bray_profile::{
    CompilationProfileComparison, CompilationProfileMetricDescriptor, CompilationProfileReport,
    CompilationProfileSummary, CompilationProfileUnit,
};

use super::metric::{bytes, duration, grouped};
use crate::units::percentage_ratio;

const RANKED_ENTRY_LIMIT: usize = 10;
const LABEL_WIDTH: usize = 28;
const NAME_WIDTH: usize = 34;

pub(crate) fn summary(report: &CompilationProfileReport) -> String {
    let summary = CompilationProfileSummary::new(report);
    let mut output = String::new();

    let _ = writeln!(
        output,
        "Compiler profile: {}/{} ({})",
        report.context.package, report.context.product, report.context.target
    );

    let _ = writeln!(
        output,
        "{:<LABEL_WIDTH$} {}",
        "Elapsed",
        duration(Duration::from_nanos(report.elapsed_nanoseconds))
    );

    write_time_breakdown(&mut output, report);
    write_scheduler_summary(&mut output, report);
    write_scheduling_wave_table(&mut output, report);
    write_cache_summary(&mut output, summary);
    write_operation_table(&mut output, summary);
    write_query_table(&mut output, summary);
    write_ready_query_table(&mut output, summary);
    write_query_propagation_table(&mut output, summary);
    write_runtime_artifacts(&mut output, report);
    write_metric_table(&mut output, summary);

    if report.mode == bray_profile::CompilationProfileMode::Trace {
        let _ = writeln!(
            output,
            "\nTrace: {} retained events, {} dropped",
            grouped(report.events.len() as u64),
            grouped(report.dropped_events)
        );
    }

    output
}

fn write_runtime_artifacts(output: &mut String, report: &CompilationProfileReport) {
    if report.runtime_artifacts.is_empty() {
        return;
    }

    let _ = writeln!(output, "\nSelected runtime artifacts");

    for artifact in &report.runtime_artifacts {
        let _ = writeln!(
            output,
            "  {:<NAME_WIDTH$} {:>12}",
            artifact.identity,
            bytes(artifact.bytes)
        );
    }
}

pub(crate) fn comparison(comparison: CompilationProfileComparison<'_>) -> String {
    let before = comparison.before();
    let after = comparison.after();
    let before_queries = CompilationProfileSummary::new(before).query_totals();
    let after_queries = CompilationProfileSummary::new(after).query_totals();
    let mut output = String::new();

    let _ = writeln!(
        output,
        "Compiler profile comparison: {}/{} ({})",
        after.context.package, after.context.product, after.context.target
    );

    let _ = writeln!(
        output,
        "{:<LABEL_WIDTH$} {:>12} {:>12} {:>20}",
        "Measure", "Before", "After", "Change"
    );

    write_duration_change(
        &mut output,
        "Elapsed",
        before.elapsed_nanoseconds,
        after.elapsed_nanoseconds,
    );

    write_duration_change(
        &mut output,
        "Summed worker self time",
        before.time.same_thread_self_nanoseconds,
        after.time.same_thread_self_nanoseconds,
    );

    write_duration_change(
        &mut output,
        "Active compiler work",
        before.time.active_work_nanoseconds,
        after.time.active_work_nanoseconds,
    );

    write_duration_change(
        &mut output,
        "Scheduler queue",
        before.time.scheduler_queue_nanoseconds,
        after.time.scheduler_queue_nanoseconds,
    );

    write_duration_change(
        &mut output,
        "Dependency wait",
        before.time.dependency_wait_nanoseconds,
        after.time.dependency_wait_nanoseconds,
    );

    write_duration_change(
        &mut output,
        "External tools",
        before.time.external_work_nanoseconds,
        after.time.external_work_nanoseconds,
    );

    write_duration_change(
        &mut output,
        "Scheduled worker activity",
        before.scheduler.active_worker_nanoseconds,
        after.scheduler.active_worker_nanoseconds,
    );

    write_duration_change(
        &mut output,
        "Query critical path",
        before.scheduler.query_critical_path_nanoseconds,
        after.scheduler.query_critical_path_nanoseconds,
    );

    write_duration_change(
        &mut output,
        "Ready-value access",
        before_queries.ready_value_nanoseconds,
        after_queries.ready_value_nanoseconds,
    );

    write_operation_changes(&mut output, comparison);
    write_query_changes(&mut output, comparison);
    write_metric_changes(&mut output, comparison);

    output
}

fn write_time_breakdown(output: &mut String, report: &CompilationProfileReport) {
    let _ = writeln!(
        output,
        "{:<LABEL_WIDTH$} {}",
        "Summed worker self time",
        duration(Duration::from_nanos(report.time.same_thread_self_nanoseconds))
    );

    for (label, nanoseconds) in [
        (
            "  Active compiler work",
            report.time.active_work_nanoseconds,
        ),
        ("  Scheduler queue", report.time.scheduler_queue_nanoseconds),
        ("  Dependency wait", report.time.dependency_wait_nanoseconds),
        ("  External tools", report.time.external_work_nanoseconds),
    ] {
        if nanoseconds > 0 {
            let _ = writeln!(output, "{label:<LABEL_WIDTH$} {}", duration(Duration::from_nanos(nanoseconds)));
        }
    }

    let _ = writeln!(
        output,
        "  Summed across workers and operation totals can overlap through nesting"
    );
}

fn write_cache_summary(output: &mut String, summary: CompilationProfileSummary<'_>) {
    let totals = summary.query_totals();
    let hit_rate = percentage_ratio(totals.cache_hits, totals.requests);

    let _ = writeln!(
        output,
        "\nQueries: {} requests, {} evaluations, {} hits ({hit_rate}), {} misses, {} waits",
        grouped(totals.requests),
        grouped(totals.evaluations),
        grouped(totals.cache_hits),
        grouped(totals.cache_misses),
        grouped(totals.waits)
    );

    if totals.cross_snapshot_reuses > 0 || totals.invalidations > 0 {
        let _ = writeln!(
            output,
            "Snapshots: {} reused values, {} invalidations",
            grouped(totals.cross_snapshot_reuses),
            grouped(totals.invalidations)
        );
    }

    if totals.ready_value_nanoseconds > 0 {
        let _ = writeln!(
            output,
            "Ready-value access: {} across {} hits",
            duration(Duration::from_nanos(totals.ready_value_nanoseconds)),
            grouped(totals.cache_hits)
        );
    }
}

fn write_scheduler_summary(output: &mut String, report: &CompilationProfileReport) {
    let scheduler = &report.scheduler;

    let available = report
        .elapsed_nanoseconds
        .saturating_mul(scheduler.worker_budget);

    let _ = writeln!(
        output,
        "Worker occupancy             {} of {} active at peak, {} of available worker time",
        grouped(scheduler.maximum_active_workers),
        grouped(scheduler.worker_budget),
        percentage_ratio(scheduler.active_worker_nanoseconds, available)
    );

    let _ = writeln!(
        output,
        "Query critical path          {}",
        duration(Duration::from_nanos(scheduler.query_critical_path_nanoseconds))
    );

    if scheduler.ready_waves > 0 {
        let _ = writeln!(
            output,
            "Ready work                   {} waves, {} items, {} maximum width",
            grouped(scheduler.ready_waves),
            grouped(scheduler.ready_items),
            grouped(scheduler.maximum_ready_width)
        );
    }
}

fn write_scheduling_wave_table(output: &mut String, report: &CompilationProfileReport) {
    if report.scheduler.wave_classes.is_empty() {
        return;
    }

    let _ = writeln!(output, "\nScheduling waves by active query or phase");

    let _ = writeln!(
        output,
        "  {:<NAME_WIDTH$} {:>8} {:>10} {:>10} {:>10} {:>10}",
        "Context", "Waves", "Planned", "Ready", "P95 width", "Max workers"
    );

    for class in &report.scheduler.wave_classes {
        let context =
            if let Some(descriptor) = class.query_id.and_then(|id| report.query_descriptor(id)) {
                display_name(&descriptor.name)
            } else if let Some(descriptor) = class
                .operation_id
                .and_then(|id| report.operation_descriptor(id))
            {
                display_name(&descriptor.name)
            } else {
                "unscoped".to_owned()
            };

        let _ = writeln!(
            output,
            "  {:<NAME_WIDTH$} {:>8} {:>10} {:>10} {:>10} {:>10}",
            context,
            grouped(class.waves),
            grouped(class.planned_items),
            grouped(class.ready_items),
            grouped(class.ready_width.p95_upper_bound),
            grouped(class.active_workers.maximum)
        );
    }
}

fn write_operation_table(output: &mut String, summary: CompilationProfileSummary<'_>) {
    let operations = summary.top_operations(RANKED_ENTRY_LIMIT);

    if operations.is_empty() {
        return;
    }

    let _ = writeln!(output, "\nTop operations by worker self time");

    let _ = writeln!(
        output,
        "  {:<NAME_WIDTH$} {:>10} {:>12} {:>12} {:>12} {:>8}",
        "Operation", "Calls", "Total", "Self", "Maximum", "Workers"
    );

    for (descriptor, statistics) in operations {
        let _ = writeln!(
            output,
            "  {:<NAME_WIDTH$} {:>10} {:>12} {:>12} {:>12} {:>8}",
            display_name(&descriptor.name),
            grouped(statistics.executions),
            duration(Duration::from_nanos(statistics.total_nanoseconds)),
            duration(Duration::from_nanos(statistics.self_nanoseconds)),
            duration(Duration::from_nanos(statistics.maximum_nanoseconds)),
            grouped(statistics.maximum_active_workers)
        );
    }
}

fn write_query_table(output: &mut String, summary: CompilationProfileSummary<'_>) {
    let queries = summary.top_queries(RANKED_ENTRY_LIMIT);

    if queries.is_empty() {
        return;
    }

    let _ = writeln!(output, "\nTop queries by evaluation self time");

    let _ = writeln!(
        output,
        "  {:<NAME_WIDTH$} {:>10} {:>12} {:>12} {:>12} {:>12}",
        "Query", "Evals", "Evaluation", "Self", "Median <=", "P95 <="
    );

    for &(descriptor, statistics) in &queries {
        let _ = writeln!(
            output,
            "  {:<NAME_WIDTH$} {:>10} {:>12} {:>12} {:>12} {:>12}",
            display_name(&descriptor.name),
            grouped(statistics.evaluations),
            duration(Duration::from_nanos(statistics.evaluation_nanoseconds)),
            duration(Duration::from_nanos(statistics.evaluation_self_nanoseconds)),
            duration(Duration::from_nanos(statistics.evaluation_latency.median_upper_bound_nanoseconds)),
            duration(Duration::from_nanos(statistics.evaluation_latency.p95_upper_bound_nanoseconds))
        );
    }

    let _ = writeln!(
        output,
        "  Evaluation times are inclusive and can overlap through nesting and parallel work"
    );
}

fn write_ready_query_table(output: &mut String, summary: CompilationProfileSummary<'_>) {
    let queries = summary.top_ready_queries(RANKED_ENTRY_LIMIT);

    if queries.is_empty() {
        return;
    }

    let _ = writeln!(output, "\nTop queries by ready-value access time");

    let _ = writeln!(
        output,
        "  {:<NAME_WIDTH$} {:>12} {:>9} {:>12} {:>12}",
        "Query", "Hits", "Hit rate", "Ready time", "Maximum"
    );

    for &(descriptor, statistics) in &queries {
        let _ = writeln!(
            output,
            "  {:<NAME_WIDTH$} {:>12} {:>9} {:>12} {:>12}",
            display_name(&descriptor.name),
            grouped(statistics.cache_hits),
            percentage_ratio(statistics.cache_hits, statistics.requests),
            duration(Duration::from_nanos(statistics.ready_value_nanoseconds)),
            duration(Duration::from_nanos(statistics.ready_value_maximum_nanoseconds))
        );
    }
}

fn write_query_propagation_table(output: &mut String, summary: CompilationProfileSummary<'_>) {
    let queries = summary.top_query_propagation(RANKED_ENTRY_LIMIT);
    let diagnostic_queries = summary.top_query_diagnostics(RANKED_ENTRY_LIMIT);

    if queries.is_empty() && diagnostic_queries.is_empty() {
        return;
    }

    if !queries.is_empty() {
        let _ = writeln!(output, "\nTop query publication and result-copy volume");

        let _ = writeln!(
            output,
            "  {:<NAME_WIDTH$} {:>10} {:>14} {:>12} {:>14}",
            "Query", "Published", "Published bytes", "Copies", "Copied bytes"
        );

        for (descriptor, statistics) in queries {
            let _ = writeln!(
                output,
                "  {:<NAME_WIDTH$} {:>10} {:>14} {:>12} {:>14}",
                display_name(&descriptor.name),
                grouped(statistics.published_values),
                bytes(statistics.published_inline_bytes),
                grouped(statistics.cloned_values),
                bytes(statistics.cloned_inline_bytes)
            );
        }
    }

    if diagnostic_queries.is_empty() {
        return;
    }

    let _ = writeln!(output, "\nTop query diagnostic propagation volume");

    let _ = writeln!(
        output,
        "  {:<NAME_WIDTH$} {:>10} {:>10} {:>10} {:>10} {:>10} {:>10}",
        "Query", "Collects", "Diags", "Copies", "Diags", "Merges", "Inputs"
    );

    for (descriptor, statistics) in diagnostic_queries {
        let _ = writeln!(
            output,
            "  {:<NAME_WIDTH$} {:>10} {:>10} {:>10} {:>10} {:>10} {:>10}",
            display_name(&descriptor.name),
            grouped(statistics.diagnostic_collections),
            grouped(statistics.result_diagnostics),
            grouped(statistics.diagnostic_copies),
            grouped(statistics.cloned_diagnostics),
            grouped(statistics.diagnostic_merges),
            grouped(statistics.merged_diagnostics)
        );
    }
}

fn write_metric_table(output: &mut String, summary: CompilationProfileSummary<'_>) {
    let metrics = summary.metrics();

    if metrics.is_empty() {
        return;
    }

    let _ = writeln!(output, "\nCompilation units and artifacts");

    for (descriptor, metric) in metrics {
        let _ = writeln!(
            output,
            "  {:<NAME_WIDTH$} {:>12}",
            display_name(&descriptor.name),
            metric_value(descriptor, metric.value)
        );
    }
}

fn write_operation_changes(output: &mut String, comparison: CompilationProfileComparison<'_>) {
    let changes = comparison.top_operation_changes(RANKED_ENTRY_LIMIT);

    if changes.is_empty() {
        return;
    }

    let _ = writeln!(output, "\nLargest operation self-time changes");

    let _ = writeln!(
        output,
        "  {:<NAME_WIDTH$} {:>12} {:>12} {:>20}",
        "Operation", "Before", "After", "Change"
    );

    for change in changes {
        write_named_duration_change(
            output,
            &display_name(&change.descriptor.name),
            change.before_self_nanoseconds,
            change.after_self_nanoseconds,
        );
    }
}

fn write_query_changes(output: &mut String, comparison: CompilationProfileComparison<'_>) {
    let changes = comparison.top_query_changes(RANKED_ENTRY_LIMIT);

    if changes.is_empty() {
        return;
    }

    let _ = writeln!(output, "\nLargest query evaluation self-time changes");

    let _ = writeln!(
        output,
        "  {:<NAME_WIDTH$} {:>12} {:>12} {:>20}",
        "Query", "Before", "After", "Change"
    );

    for change in changes {
        write_named_duration_change(
            output,
            &display_name(&change.descriptor.name),
            change.before_evaluation_self_nanoseconds,
            change.after_evaluation_self_nanoseconds,
        );
    }
}

fn write_metric_changes(output: &mut String, comparison: CompilationProfileComparison<'_>) {
    let changes = comparison.metric_changes();

    if changes.is_empty() {
        return;
    }

    let _ = writeln!(output, "\nCompilation unit and artifact changes");

    for change in changes {
        let before = metric_value(change.descriptor, change.before);
        let after = metric_value(change.descriptor, change.after);

        let _ = writeln!(
            output,
            "  {:<NAME_WIDTH$} {:>12} -> {:>12}",
            display_name(&change.descriptor.name),
            before,
            after
        );
    }
}

fn write_duration_change(output: &mut String, label: &str, before: u64, after: u64) {
    let _ = writeln!(
        output,
        "{label:<LABEL_WIDTH$} {:>12} {:>12} {:>20}",
        duration(Duration::from_nanos(before)),
        duration(Duration::from_nanos(after)),
        duration_change(before, after)
    );
}

fn write_named_duration_change(output: &mut String, label: &str, before: u64, after: u64) {
    let _ = writeln!(
        output,
        "  {label:<NAME_WIDTH$} {:>12} {:>12} {:>20}",
        duration(Duration::from_nanos(before)),
        duration(Duration::from_nanos(after)),
        duration_change(before, after)
    );
}

fn duration_change(before: u64, after: u64) -> String {
    if before == after {
        return "no change".to_owned();
    }

    let magnitude = before.abs_diff(after);
    let sign = if after >= before { "+" } else { "-" };

    if before == 0 {
        return format!("{sign}{} (new)", duration(Duration::from_nanos(magnitude)));
    }

    format!(
        "{sign}{} ({sign}{})",
        duration(Duration::from_nanos(magnitude)),
        percentage_ratio(magnitude, before)
    )
}

fn metric_value(descriptor: &CompilationProfileMetricDescriptor, value: u64) -> String {
    match descriptor.unit {
        CompilationProfileUnit::Bytes => bytes(value),
        CompilationProfileUnit::Count => grouped(value),
        CompilationProfileUnit::Nanoseconds => duration(Duration::from_nanos(value)),
    }
}

fn display_name(canonical: &str) -> String {
    let name = canonical
        .strip_prefix("compiler.")
        .unwrap_or(canonical)
        .replace(['.', '_'], " ");

    if name.chars().count() <= NAME_WIDTH {
        return name;
    }

    let mut shortened = name.chars().take(NAME_WIDTH - 1).collect::<String>();

    shortened.push('…');

    shortened
}

#[cfg(test)]
mod tests {
    use super::duration_change;

    #[test]
    fn duration_changes_preserve_signed_percentages_and_zero_baselines() {
        for (before, after, expected) in [
            (1_000, 1_500, "+500 ns (+50.0%)"),
            (1_500, 1_000, "-500 ns (-33.3%)"),
            (1_000, 0, "-1 us (-100.0%)"),
            (0, 1_000, "+1 us (new)"),
            (0, 0, "no change"),
            (1_000, 1_000, "no change"),
        ] {
            assert_eq!(duration_change(before, after), expected);
        }
    }
}
