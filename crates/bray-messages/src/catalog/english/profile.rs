use std::fmt::Write;

use bray_profile::{
    CompilationProfileComparison, CompilationProfileMetricDescriptor, CompilationProfileReport,
    CompilationProfileSummary, CompilationProfileUnit,
};

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
        duration(report.elapsed_nanoseconds)
    );

    write_time_breakdown(&mut output, report);
    write_cache_summary(&mut output, summary);
    write_operation_table(&mut output, summary);
    write_query_table(&mut output, summary);
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

pub(crate) fn comparison(comparison: CompilationProfileComparison<'_>) -> String {
    let before = comparison.before();
    let after = comparison.after();
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
        duration(report.time.same_thread_self_nanoseconds)
    );

    for (label, nanoseconds) in [
        ("  Active compiler work", report.time.active_work_nanoseconds),
        ("  Scheduler queue", report.time.scheduler_queue_nanoseconds),
        ("  Dependency wait", report.time.dependency_wait_nanoseconds),
        ("  External tools", report.time.external_work_nanoseconds),
    ] {
        if nanoseconds > 0 {
            let _ = writeln!(output, "{label:<LABEL_WIDTH$} {}", duration(nanoseconds));
        }
    }

    let _ = writeln!(
        output,
        "  Summed across workers and operation totals can overlap through nesting"
    );
}

fn write_cache_summary(output: &mut String, summary: CompilationProfileSummary<'_>) {
    let totals = summary.query_totals();
    let hit_rate = percentage(totals.cache_hits, totals.requests);

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
}

fn write_operation_table(output: &mut String, summary: CompilationProfileSummary<'_>) {
    let operations = summary.top_operations(RANKED_ENTRY_LIMIT);

    if operations.is_empty() {
        return;
    }

    let _ = writeln!(output, "\nTop operations by worker self time");

    let _ = writeln!(
        output,
        "  {:<NAME_WIDTH$} {:>10} {:>12} {:>12} {:>12}",
        "Operation", "Calls", "Total", "Self", "Maximum"
    );

    for (descriptor, statistics) in operations {
        let _ = writeln!(
            output,
            "  {:<NAME_WIDTH$} {:>10} {:>12} {:>12} {:>12}",
            display_name(&descriptor.name),
            grouped(statistics.executions),
            duration(statistics.total_nanoseconds),
            duration(statistics.self_nanoseconds),
            duration(statistics.maximum_nanoseconds)
        );
    }
}

fn write_query_table(output: &mut String, summary: CompilationProfileSummary<'_>) {
    let queries = summary.top_queries(RANKED_ENTRY_LIMIT);

    if queries.is_empty() {
        return;
    }

    let _ = writeln!(output, "\nTop queries by evaluation time");

    let _ = writeln!(
        output,
        "  {:<NAME_WIDTH$} {:>10} {:>10} {:>9} {:>12} {:>12}",
        "Query", "Requests", "Evals", "Hit rate", "Evaluation", "Wait"
    );

    for (descriptor, statistics) in queries {
        let _ = writeln!(
            output,
            "  {:<NAME_WIDTH$} {:>10} {:>10} {:>9} {:>12} {:>12}",
            display_name(&descriptor.name),
            grouped(statistics.requests),
            grouped(statistics.evaluations),
            percentage(statistics.cache_hits, statistics.requests),
            duration(statistics.evaluation_nanoseconds),
            duration(statistics.wait_nanoseconds)
        );
    }

    let _ = writeln!(
        output,
        "  Evaluation times are inclusive and can overlap through nesting and parallel work"
    );
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

    let _ = writeln!(output, "\nLargest query evaluation-time changes");

    let _ = writeln!(
        output,
        "  {:<NAME_WIDTH$} {:>12} {:>12} {:>20}",
        "Query", "Before", "After", "Change"
    );

    for change in changes {
        write_named_duration_change(
            output,
            &display_name(&change.descriptor.name),
            change.before_evaluation_nanoseconds,
            change.after_evaluation_nanoseconds,
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
        duration(before),
        duration(after),
        duration_change(before, after)
    );
}

fn write_named_duration_change(output: &mut String, label: &str, before: u64, after: u64) {
    let _ = writeln!(
        output,
        "  {label:<NAME_WIDTH$} {:>12} {:>12} {:>20}",
        duration(before),
        duration(after),
        duration_change(before, after)
    );
}

fn duration(nanoseconds: u64) -> String {
    if nanoseconds >= 1_000_000_000 {
        format!("{:.3} s", nanoseconds as f64 / 1_000_000_000.0)
    } else if nanoseconds >= 1_000_000 {
        format!("{:.3} ms", nanoseconds as f64 / 1_000_000.0)
    } else if nanoseconds >= 1_000 {
        format!("{:.3} us", nanoseconds as f64 / 1_000.0)
    } else {
        format!("{nanoseconds} ns")
    }
}

fn duration_change(before: u64, after: u64) -> String {
    if before == after {
        return "no change".to_owned();
    }

    let magnitude = before.abs_diff(after);
    let sign = if after >= before { "+" } else { "-" };

    if before == 0 {
        return format!("{sign}{} (new)", duration(magnitude));
    }

    format!(
        "{sign}{} ({sign}{:.1}%)",
        duration(magnitude),
        magnitude as f64 * 100.0 / before as f64
    )
}

fn metric_value(descriptor: &CompilationProfileMetricDescriptor, value: u64) -> String {
    match descriptor.unit {
        CompilationProfileUnit::Bytes => bytes(value),
        CompilationProfileUnit::Count | CompilationProfileUnit::Nanoseconds => grouped(value),
    }
}

fn bytes(value: u64) -> String {
    if value >= 1024 * 1024 {
        format!("{:.2} MiB", value as f64 / (1024.0 * 1024.0))
    } else if value >= 1024 {
        format!("{:.2} KiB", value as f64 / 1024.0)
    } else {
        format!("{} B", grouped(value))
    }
}

fn percentage(part: u64, total: u64) -> String {
    if total == 0 {
        return "0.0%".to_owned();
    }

    format!("{:.1}%", part as f64 * 100.0 / total as f64)
}

fn grouped(value: u64) -> String {
    let digits = value.to_string();
    let mut output = String::with_capacity(digits.len() + digits.len() / 3);

    for (index, byte) in digits.bytes().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            output.push(',');
        }

        output.push(char::from(byte));
    }

    output
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
