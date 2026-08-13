use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use super::model::{
    ArtifactKind, ChangeAssessment, ComparisonReport, MetricComparison, Observation,
    ObservationComparison, PerformanceReport,
};

const MAX_HTML_BYTES: usize = 8 * 1024 * 1024;

struct BoundedHtml {
    contents: String,
    overflowed: bool,
}

impl BoundedHtml {
    fn new() -> Self {
        Self {
            contents: String::new(),
            overflowed: false,
        }
    }

    fn push_str(&mut self, value: &str) {
        if self.contents.len().saturating_add(value.len()) > MAX_HTML_BYTES {
            self.overflowed = true;

            return;
        }

        self.contents.push_str(value);
    }
}

impl std::fmt::Write for BoundedHtml {
    fn write_str(&mut self, value: &str) -> std::fmt::Result {
        self.push_str(value);

        if self.overflowed {
            Err(std::fmt::Error)
        } else {
            Ok(())
        }
    }
}

pub(super) fn write_candidate(path: &Path, report: &PerformanceReport) -> Result<(), String> {
    write(path, render_candidate(report)?)
}

fn render_candidate(report: &PerformanceReport) -> Result<String, String> {
    let mut html = document_start("Standard library performance report");

    identity(&mut html, "Candidate", &report.identity);

    html.push_str(
        "<section><h2>How to read this report</h2>\
        <p>The primary duration is the measured Bray root execution. Process duration also includes \
        executable startup and teardown. Throughput uses the Bray duration.</p></section>",
    );

    html.push_str(
        "<section><h2>Workloads</h2><div class=\"table-scroll\"><table><thead><tr>\
        <th>Workload</th><th>Bray median</th><th>Bray MAD</th><th>Process median</th>\
        <th>Throughput</th><th>Executable</th><th>Compile time</th>\
        <th>Allocations</th><th>Allocated</th><th>Copied</th></tr></thead><tbody>",
    );

    for workload in &report.workloads {
        let executable = workload
            .artifacts
            .iter()
            .find(|artifact| artifact.kind == ArtifactKind::Executable);

        let _ = write!(
            html,
            "<tr><th>{}</th><td {}>{}</td><td {}>{}</td><td {}>{}</td>\
            <td>{} {}/s</td><td>{}</td><td {}>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            escape(&workload.id),
            nanoseconds_title(workload.bray_execution.median_nanoseconds),
            milliseconds(workload.bray_execution.median_nanoseconds),
            nanoseconds_title(
                workload
                    .bray_execution
                    .median_absolute_deviation_nanoseconds
            ),
            milliseconds(
                workload
                    .bray_execution
                    .median_absolute_deviation_nanoseconds
            ),
            nanoseconds_title(workload.process_execution.median_nanoseconds),
            milliseconds(workload.process_execution.median_nanoseconds),
            grouped(workload.bray_execution.median_units_per_second),
            escape(&workload.units),
            executable.map_or_else(|| "Unavailable".to_owned(), |artifact| kibibytes(artifact.bytes)),
            nanoseconds_title(workload.compilation.elapsed_nanoseconds),
            milliseconds(workload.compilation.elapsed_nanoseconds),
            observation_count(&workload.observations.allocation_count),
            observation_bytes(&workload.observations.allocated_bytes),
            observation_bytes(&workload.observations.copied_bytes),
        );
    }

    html.push_str("</tbody></table></div></section>");

    for workload in &report.workloads {
        workload_details(&mut html, workload);
    }

    finish(html)
}

pub(super) fn write_comparison(
    path: &Path,
    comparison: &ComparisonReport,
) -> Result<(), String> {
    write(path, render_comparison(comparison)?)
}

fn render_comparison(comparison: &ComparisonReport) -> Result<String, String> {
    let mut html = document_start("Standard library performance comparison");

    identity(&mut html, "Baseline", &comparison.baseline_identity);
    identity(&mut html, "Candidate", &comparison.candidate_identity);

    html.push_str(
        "<section><h2>Changes</h2><p>Negative duration and size changes are improvements. \
        Timing changes within the noise boundary are marked indeterminate.</p><div class=\"table-scroll\">\
        <table><thead><tr><th>Workload</th><th>Bray duration</th><th>Process duration</th>\
        <th>Executable size</th><th>Retained inputs</th></tr></thead><tbody>",
    );

    for workload in &comparison.workloads {
        let executable = workload
            .artifacts
            .iter()
            .find(|artifact| artifact.kind == ArtifactKind::Executable);

        let retained = executable.map_or_else(
            || "Unavailable".to_owned(),
            |artifact| {
                format!(
                    "+{} and -{}",
                    artifact.added_static_inputs.len(),
                    artifact.removed_static_inputs.len()
                )
            },
        );

        let _ = write!(
            html,
            "<tr><th>{}</th><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            escape(&workload.id),
            comparison_metric(workload.bray_execution, MetricUnit::Duration),
            comparison_metric(workload.process_execution, MetricUnit::Duration),
            executable.map_or_else(
                || "Unavailable".to_owned(),
                |artifact| comparison_metric(artifact.bytes, MetricUnit::Bytes)
            ),
            retained,
        );
    }

    html.push_str("</tbody></table></div></section>");

    for workload in &comparison.workloads {
        comparison_details(&mut html, workload);
    }

    finish(html)
}

fn comparison_details(html: &mut BoundedHtml, workload: &super::model::WorkloadComparison) {
    let _ = write!(html, "<section><h2>{}</h2>", escape(&workload.id));

    if !workload.compiler_operations.is_empty() {
        html.push_str("<h3>Compiler operations</h3><dl>");

        for (name, metric) in &workload.compiler_operations {
            let _ = write!(
                html,
                "<dt>{}</dt><dd>{}</dd>",
                escape(name),
                comparison_metric(*metric, MetricUnit::Duration)
            );
        }

        html.push_str("</dl>");
    }

    if !workload.compiler_metrics.is_empty() {
        html.push_str("<h3>Compiler metrics</h3><dl>");

        for (name, metric) in &workload.compiler_metrics {
            let _ = write!(
                html,
                "<dt>{}</dt><dd>{:+} ({})</dd>",
                escape(name),
                metric.delta,
                assessment(metric.assessment)
            );
        }

        html.push_str("</dl>");
    }

    html.push_str("<h3>Observed work</h3><dl>");

    observation_comparison_detail(
        html,
        "Allocations",
        &workload.observations.allocation_count,
        MetricUnit::Count,
    );

    observation_comparison_detail(
        html,
        "Allocated bytes",
        &workload.observations.allocated_bytes,
        MetricUnit::Bytes,
    );

    observation_comparison_detail(
        html,
        "Copied bytes",
        &workload.observations.copied_bytes,
        MetricUnit::Bytes,
    );

    for (name, observation) in &workload.observations.platform_operations {
        observation_comparison_detail(html, name, observation, MetricUnit::Count);
    }

    html.push_str("</dl>");

    for artifact in &workload.artifacts {
        let _ = write!(
            html,
            "<details><summary>{:?} changes</summary><h4>Sections</h4><dl>",
            artifact.kind
        );

        for (name, metric) in &artifact.sections {
            let _ = write!(
                html,
                "<dt><code>{}</code></dt><dd>{}</dd>",
                escape(name),
                comparison_metric(*metric, MetricUnit::Bytes)
            );
        }

        html.push_str("</dl><h4>Added static inputs</h4><ul>");
        retained_inputs(html, &artifact.added_static_inputs);
        html.push_str("</ul><h4>Removed static inputs</h4><ul>");
        retained_inputs(html, &artifact.removed_static_inputs);
        html.push_str("</ul><h4>Added dynamic libraries</h4><ul>");
        escaped_list(html, &artifact.added_dynamic_libraries);
        html.push_str("</ul><h4>Removed dynamic libraries</h4><ul>");
        escaped_list(html, &artifact.removed_dynamic_libraries);
        html.push_str("</ul></details>");
    }

    html.push_str("</section>");
}

fn observation_comparison_detail(
    html: &mut BoundedHtml,
    label: &str,
    observation: &ObservationComparison,
    unit: MetricUnit,
) {
    let rendered = match observation {
        ObservationComparison::Measured { comparison, scope } => format!(
            "{}, measured in {}",
            comparison_metric(*comparison, unit),
            escape(scope)
        ),
        ObservationComparison::Incomparable { .. } => "Incomparable".to_owned(),
    };

    let _ = write!(
        html,
        "<dt>{}</dt><dd>{}</dd>",
        escape(label),
        rendered
    );
}

fn retained_inputs(html: &mut BoundedHtml, inputs: &[super::model::RetainedInput]) {
    for input in inputs {
        let identity = input.member.as_ref().map_or_else(
            || input.artifact.clone(),
            |member| format!("{} ({member})", input.artifact),
        );

        let _ = write!(html, "<li><code>{}</code></li>", escape(&identity));
    }
}

fn escaped_list(html: &mut BoundedHtml, entries: &[String]) {
    for entry in entries {
        let _ = write!(html, "<li><code>{}</code></li>", escape(entry));
    }
}

fn workload_details(html: &mut BoundedHtml, workload: &super::model::WorkloadReport) {
    let _ = write!(
        html,
        "<section><h2>{}</h2><dl><dt>Bray timing scope</dt><dd>{}</dd>\
        <dt>Process timing scope</dt><dd>{}</dd><dt>Expected output SHA-256</dt><dd><code>{}</code></dd></dl>",
        escape(&workload.id),
        escape(&workload.bray_execution.scope),
        escape(&workload.process_execution.scope),
        escape(&workload.expected_output_sha256),
    );

    html.push_str("<h3>Observed work</h3><dl>");
    observation_detail(html, "Allocations", &workload.observations.allocation_count);
    observation_detail(html, "Allocated bytes", &workload.observations.allocated_bytes);
    observation_detail(html, "Copied bytes", &workload.observations.copied_bytes);

    for (operation, observation) in &workload.observations.platform_operations {
        observation_detail(html, operation, observation);
    }

    html.push_str("</dl>");
    compiler_details(html, &workload.compilation);

    for artifact in &workload.artifacts {
        let _ = write!(
            html,
            "<details><summary>{:?} artifact, {}</summary><dl><dt>Path</dt><dd><code>{}</code></dd>\
            <dt>Static inputs</dt><dd>{} shown, {} omitted</dd><dt>Dynamic libraries</dt>\
            <dd>{} shown, {} omitted</dd><dt>Sections</dt><dd>{} shown, {} omitted</dd></dl>",
            artifact.kind,
            kibibytes(artifact.bytes),
            escape(&artifact.path),
            artifact.dependencies.static_inputs.entries.len(),
            artifact.dependencies.static_inputs.omitted_count,
            artifact.dependencies.dynamic_libraries.entries.len(),
            artifact.dependencies.dynamic_libraries.omitted_count,
            artifact.sections.entries.len(),
            artifact.sections.omitted_count,
        );

        html.push_str("<h4>Retained static inputs</h4><ul>");

        for input in &artifact.dependencies.static_inputs.entries {
            let identity = input.member.as_ref().map_or_else(
                || input.artifact.clone(),
                |member| format!("{} ({member})", input.artifact),
            );

            let _ = write!(html, "<li><code>{}</code></li>", escape(&identity));
        }

        html.push_str("</ul><h4>Dynamic libraries</h4><ul>");

        for library in &artifact.dependencies.dynamic_libraries.entries {
            let _ = write!(html, "<li><code>{}</code></li>", escape(library));
        }

        html.push_str("</ul><h4>Sections</h4><ul>");

        for section in &artifact.sections.entries {
            let _ = write!(
                html,
                "<li><code>{}</code>, {}</li>",
                escape(&section.name),
                kibibytes(section.bytes)
            );
        }

        html.push_str("</ul></details>");
    }

    html.push_str("</section>");
}

fn compiler_details(
    html: &mut BoundedHtml,
    profile: &bray_compilation::CompilationProfileReport,
) {
    html.push_str(
        "<details><summary>Compiler breakdown</summary><h4>Operations</h4><table><thead><tr>\
        <th>Operation</th><th>Calls</th><th>Self time</th><th>Maximum</th></tr></thead><tbody>",
    );

    for operation in &profile.operations {
        let name = profile
            .operation_descriptor(operation.id)
            .map_or("Unknown operation", |descriptor| descriptor.name.as_str());

        let _ = write!(
            html,
            "<tr><th>{}</th><td>{}</td><td {}>{}</td><td {}>{}</td></tr>",
            escape(name),
            grouped(operation.executions),
            nanoseconds_title(operation.self_nanoseconds),
            milliseconds(operation.self_nanoseconds),
            nanoseconds_title(operation.maximum_nanoseconds),
            milliseconds(operation.maximum_nanoseconds),
        );
    }

    html.push_str("</tbody></table><h4>Metrics</h4><dl>");

    for metric in &profile.metrics {
        let name = profile
            .metric_descriptor(metric.id)
            .map_or("Unknown metric", |descriptor| descriptor.name.as_str());

        let _ = write!(
            html,
            "<dt>{}</dt><dd>{}</dd>",
            escape(name),
            grouped(metric.value)
        );
    }

    html.push_str("</dl></details>");
}

fn observation_detail(html: &mut BoundedHtml, label: &str, observation: &Observation) {
    match observation {
        Observation::Measured { value, scope } => {
            let _ = write!(
                html,
                "<dt>{}</dt><dd>{}, measured in {}</dd>",
                escape(label),
                grouped(*value),
                escape(scope)
            );
        }
        Observation::Unavailable { reason } => {
            let _ = write!(
                html,
                "<dt>{}</dt><dd>Unavailable because {}</dd>",
                escape(label),
                escape(reason)
            );
        }
    }
}

fn identity(html: &mut BoundedHtml, label: &str, identity: &super::model::ReportIdentity) {
    let _ = write!(
        html,
        "<section><h2>{label}</h2><dl class=\"identity\"><dt>Target</dt><dd>{}</dd>\
        <dt>Host</dt><dd>{}</dd><dt>Compiler</dt><dd>{}</dd><dt>Source revision</dt>\
        <dd><code>{}</code></dd><dt>LLVM</dt><dd>{}</dd><dt>Corpus SHA-256</dt>\
        <dd><code>{}</code></dd><dt>Samples</dt><dd>{} warmup and {} measured</dd></dl></section>",
        escape(&identity.target),
        escape(&identity.host),
        escape(&identity.compiler_version),
        escape(&identity.source_revision),
        escape(&identity.llvm_version),
        escape(&identity.corpus_sha256),
        identity.warmup_iterations,
        identity.sample_iterations,
    );
}

fn document_start(title: &str) -> BoundedHtml {
    let mut html = BoundedHtml::new();

    let _ = write!(
        html,
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
        <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
        <title>{}</title><style>{}</style></head><body><main><h1>{}</h1>",
        escape(title),
        CSS,
        escape(title),
    );

    html
}

fn finish(mut html: BoundedHtml) -> Result<String, String> {
    html.push_str("</main></body></html>");

    if html.overflowed {
        return Err(format!(
            "HTML performance report exceeds its {} byte bound",
            MAX_HTML_BYTES
        ));
    }

    Ok(html.contents)
}

fn write(path: &Path, html: String) -> Result<(), String> {
    fs::write(path, html).map_err(|error| format!("could not write {}: {error}", path.display()))
}

fn observation_count(observation: &Observation) -> String {
    match observation {
        Observation::Measured { value, .. } => grouped(*value),
        Observation::Unavailable { .. } => "Unavailable".to_owned(),
    }
}

fn observation_bytes(observation: &Observation) -> String {
    match observation {
        Observation::Measured { value, .. } => kibibytes(*value),
        Observation::Unavailable { .. } => "Unavailable".to_owned(),
    }
}

#[derive(Clone, Copy)]
enum MetricUnit {
    Duration,
    Bytes,
    Count,
}

fn comparison_metric(metric: MetricComparison, unit: MetricUnit) -> String {
    let delta = match unit {
        MetricUnit::Duration => signed_milliseconds(metric.delta),
        MetricUnit::Bytes => signed_kibibytes(metric.delta),
        MetricUnit::Count => format!("{:+}", metric.delta),
    };

    let ratio = metric.delta_basis_points.map_or_else(
        || "no ratio".to_owned(),
        |basis_points| format!("{:+.2}%", basis_points as f64 / 100.0),
    );

    format!(
        "<span class=\"{}\">{} ({}, {})</span>",
        assessment_class(metric.assessment),
        assessment(metric.assessment),
        delta,
        ratio,
    )
}

fn milliseconds(nanoseconds: u64) -> String {
    let milliseconds = nanoseconds as f64 / 1_000_000.0;

    if milliseconds < 0.001 {
        format!("{milliseconds:.6} ms")
    } else {
        format!("{milliseconds:.3} ms")
    }
}

fn signed_milliseconds(nanoseconds: i128) -> String {
    let milliseconds = nanoseconds as f64 / 1_000_000.0;

    if milliseconds.abs() < 0.001 {
        format!("{milliseconds:+.6} ms")
    } else {
        format!("{milliseconds:+.3} ms")
    }
}

fn kibibytes(bytes: u64) -> String {
    format!("{:.2} KiB", bytes as f64 / 1024.0)
}

fn signed_kibibytes(bytes: i128) -> String {
    format!("{:+.2} KiB", bytes as f64 / 1024.0)
}

fn nanoseconds_title(nanoseconds: u64) -> String {
    format!("title=\"{} ns\"", grouped(nanoseconds))
}

fn grouped(value: u64) -> String {
    let digits = value.to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);

    for (index, character) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }

        grouped.push(character);
    }

    grouped
}

fn escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());

    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(character),
        }
    }

    escaped
}

const fn assessment(value: ChangeAssessment) -> &'static str {
    match value {
        ChangeAssessment::Improved => "Improved",
        ChangeAssessment::Regressed => "Regressed",
        ChangeAssessment::Indeterminate => "Indeterminate",
    }
}

const fn assessment_class(value: ChangeAssessment) -> &'static str {
    match value {
        ChangeAssessment::Improved => "improved",
        ChangeAssessment::Regressed => "regressed",
        ChangeAssessment::Indeterminate => "indeterminate",
    }
}

const CSS: &str = r#"
:root { color-scheme: light dark; font-family: system-ui, sans-serif; line-height: 1.45; }
body { margin: 0; background: Canvas; color: CanvasText; }
main { max-width: 1200px; margin: 0 auto; padding: 2rem; }
section { margin: 2rem 0; }
table { border-collapse: collapse; width: 100%; font-variant-numeric: tabular-nums; }
th, td { border-bottom: 1px solid color-mix(in srgb, CanvasText 20%, transparent); padding: .6rem; text-align: right; white-space: nowrap; }
th:first-child, td:first-child { text-align: left; }
.table-scroll { overflow-x: auto; }
dl { display: grid; grid-template-columns: max-content 1fr; gap: .35rem 1rem; }
dt { font-weight: 650; }
dd { margin: 0; overflow-wrap: anywhere; }
code { font-family: ui-monospace, monospace; }
.improved { color: #16803c; }
.regressed { color: #c43b32; }
.indeterminate { color: #767676; }
@media (prefers-color-scheme: dark) { .improved { color: #65d58a; } .regressed { color: #ff8178; } .indeterminate { color: #aaa; } }
"#;

#[cfg(test)]
mod tests {
    use super::{
        BoundedHtml, MAX_HTML_BYTES, escape, finish, grouped, render_candidate,
        render_comparison,
    };

    #[test]
    fn html_escaping_covers_text_and_attribute_delimiters() {
        assert_eq!(escape("<&>\"'"), "&lt;&amp;&gt;&quot;&#39;");
    }

    #[test]
    fn grouped_integers_are_human_readable() {
        assert_eq!(grouped(999), "999");
        assert_eq!(grouped(1_000), "1,000");
        assert_eq!(grouped(1_234_567), "1,234,567");
    }

    #[test]
    fn html_report_bound_is_explicit_and_smaller_than_machine_input_bound() {
        assert_eq!(MAX_HTML_BYTES, 8 * 1024 * 1024);

        let mut html = BoundedHtml::new();

        html.push_str(&"x".repeat(MAX_HTML_BYTES));
        html.push_str("overflow");

        assert!(finish(html).is_err());
    }

    #[test]
    fn candidate_html_is_deterministic_escaped_and_agrees_with_the_report() {
        let mut report = super::super::tests::report("<corpus>", 2_500_000, 100_000);

        report.workloads[0].id = "workload<&>\"'".to_owned();
        report.workloads[0].artifacts[0].path = "<artifact>".to_owned();

        let first = render_candidate(&report)
            .unwrap_or_else(|error| panic!("candidate report must render: {error}"));

        let second = render_candidate(&report)
            .unwrap_or_else(|error| panic!("candidate report must render twice: {error}"));

        assert_eq!(first, second);
        assert!(first.contains("workload&lt;&amp;&gt;&quot;&#39;"));
        assert!(first.contains("&lt;artifact&gt;"));
        assert!(first.contains("2.500 ms"));
        assert!(first.contains(&report.identity.corpus_sha256));
        assert!(!first.contains("<artifact>"));
    }

    #[test]
    fn comparison_html_is_deterministic_and_agrees_with_structured_deltas() {
        let baseline = super::super::tests::report("corpus", 2_500_000, 100_000);
        let candidate = super::super::tests::report("corpus", 1_000_000, 100_000);

        let comparison = super::super::comparison::compare(&baseline, &candidate)
            .unwrap_or_else(|error| panic!("reports must compare: {error}"));

        let first = render_comparison(&comparison)
            .unwrap_or_else(|error| panic!("comparison must render: {error}"));

        let second = render_comparison(&comparison)
            .unwrap_or_else(|error| panic!("comparison must render twice: {error}"));

        assert_eq!(first, second);
        assert!(first.contains("-1.500 ms"));
        assert!(first.contains("Improved"));
        assert!(first.contains(&comparison.candidate_identity.corpus_sha256));
    }
}
