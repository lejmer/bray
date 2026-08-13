use std::fmt::Write as _;
use std::path::Path;

use super::model::{
    ArtifactKind, ChangeAssessment, ComparisonReport, MetricComparison, Observation,
    ObservationComparison, PeerComparison, PeerOutcome, PerformanceReport,
};

use super::format::{grouped, kibibytes, milliseconds, nanoseconds_title, signed_kibibytes, signed_milliseconds};
use super::html::{BoundedHtml, document_start, escape, finish, write};

pub(super) fn write_candidate(path: &Path, report: &PerformanceReport) -> Result<(), String> {
    write(path, render_candidate(report)?)
}

fn render_candidate(report: &PerformanceReport) -> Result<String, String> {
    let mut html = document_start("Standard library performance report");

    identity(&mut html, "Candidate", &report.identity);

    html.push_str(
        "<section><h2>How to read this report</h2>\
        <p>The primary duration is the language-controlled workload execution. Process duration also includes \
        executable startup and teardown. Throughput uses the controlled duration.</p></section>",
    );

    html.push_str(
        "<section><h2>Workloads</h2><div class=\"table-scroll\"><table><thead><tr>\
        <th>Workload</th><th>Language</th><th>Controlled median</th><th>Controlled MAD</th><th>Process median</th>\
        <th>Throughput</th><th>Executable</th><th>Compile and link time</th>\
        <th>Allocations</th><th>Allocated</th><th>Copied</th></tr></thead><tbody>",
    );

    for workload in &report.workloads {
        candidate_row(
            &mut html,
            workload,
            "Bray",
            &workload.bray_execution,
            &workload.process_execution,
            workload.compilation.elapsed_nanoseconds,
            &workload.artifacts,
            &workload.observations,
        );

        for (language, peer) in &workload.peers {
            match peer {
                PeerOutcome::Measured { report } => candidate_row(
                    &mut html,
                    workload,
                    peer_language(*language),
                    &report.controlled_execution,
                    &report.process_execution,
                    report.production_compile_link_nanoseconds,
                    &report.artifacts,
                    &report.observations,
                ),
                PeerOutcome::Unsupported { reason } => {
                    let _ = write!(
                        html,
                        "<tr><th>{}</th><td>{}</td><td colspan=\"9\">Unsupported because {}</td></tr>",
                        escape(&workload.id),
                        peer_language(*language),
                        escape(reason),
                    );
                }
            }
        }
    }

    html.push_str("</tbody></table></div></section>");

    for workload in &report.workloads {
        workload_details(&mut html, workload);
    }

    finish(html)
}

#[expect(
    clippy::too_many_arguments,
    reason = "one report row keeps the shared cross-language measurement contract visible"
)]
fn candidate_row(
    html: &mut BoundedHtml,
    workload: &super::model::WorkloadReport,
    language: &str,
    controlled: &super::model::ExecutionStatistics,
    process: &super::model::ExecutionStatistics,
    compile_link_nanoseconds: u64,
    artifacts: &[super::model::ArtifactReport],
    observations: &super::model::WorkloadObservations,
) {
    let executable = artifacts
        .iter()
        .find(|artifact| artifact.kind == ArtifactKind::Executable);

    let _ = write!(
        html,
        "<tr><th>{}</th><td>{}</td><td {}>{}</td><td {}>{}</td><td {}>{}</td>\
        <td>{} {}/s</td><td>{}</td><td {}>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
        escape(&workload.id),
        language,
        nanoseconds_title(controlled.median_nanoseconds),
        milliseconds(controlled.median_nanoseconds),
        nanoseconds_title(controlled.median_absolute_deviation_nanoseconds),
        milliseconds(controlled.median_absolute_deviation_nanoseconds),
        nanoseconds_title(process.median_nanoseconds),
        milliseconds(process.median_nanoseconds),
        grouped(controlled.median_units_per_second),
        escape(&workload.units),
        executable.map_or_else(|| "Unavailable".to_owned(), |artifact| kibibytes(artifact.bytes)),
        nanoseconds_title(compile_link_nanoseconds),
        milliseconds(compile_link_nanoseconds),
        observation_count(&observations.allocation_count),
        observation_bytes(&observations.allocated_bytes),
        observation_bytes(&observations.copied_bytes),
    );
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
        <table><thead><tr><th>Workload</th><th>Language</th><th>Controlled duration</th><th>Process duration</th>\
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
            "<tr><th>{}</th><td>Bray</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            escape(&workload.id),
            comparison_metric(workload.bray_execution, MetricUnit::Duration),
            comparison_metric(workload.process_execution, MetricUnit::Duration),
            executable.map_or_else(
                || "Unavailable".to_owned(),
                |artifact| comparison_metric(artifact.bytes, MetricUnit::Bytes)
            ),
            retained,
        );

        for (language, peer) in &workload.peers {
            match peer {
                PeerComparison::Measured {
                    process_execution,
                    controlled_execution,
                    artifacts,
                    ..
                } => comparison_row(
                    &mut html,
                    &workload.id,
                    peer_language(*language),
                    *controlled_execution,
                    *process_execution,
                    artifacts,
                ),
                PeerComparison::Unsupported { reason } => {
                    let _ = write!(
                        html,
                        "<tr><th>{}</th><td>{}</td><td colspan=\"4\">Unsupported because {}</td></tr>",
                        escape(&workload.id),
                        peer_language(*language),
                        escape(reason),
                    );
                }
            }
        }
    }

    html.push_str("</tbody></table></div></section>");

    for workload in &comparison.workloads {
        comparison_details(&mut html, workload);
    }

    finish(html)
}

fn comparison_row(
    html: &mut BoundedHtml,
    workload: &str,
    language: &str,
    controlled: MetricComparison,
    process: MetricComparison,
    artifacts: &[super::model::ArtifactComparison],
) {
    let executable = artifacts
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
        "<tr><th>{}</th><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
        escape(workload),
        language,
        comparison_metric(controlled, MetricUnit::Duration),
        comparison_metric(process, MetricUnit::Duration),
        executable.map_or_else(
            || "Unavailable".to_owned(),
            |artifact| comparison_metric(artifact.bytes, MetricUnit::Bytes)
        ),
        retained,
    );
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

    for (language, peer) in &workload.peers {
        let language = peer_language(*language);

        match peer {
            PeerComparison::Measured {
                compile_link,
                artifacts,
                ..
            } => {
                let _ = write!(
                    html,
                    "<details><summary>{language} peer changes</summary><dl><dt>Compile and link</dt><dd>{}</dd></dl>",
                    comparison_metric(*compile_link, MetricUnit::Duration),
                );

                artifact_comparison_details(html, artifacts);
                html.push_str("</details>");
            }
            PeerComparison::Unsupported { reason } => {
                let _ = write!(
                    html,
                    "<p><strong>{language}</strong> is unsupported because {}.</p>",
                    escape(reason),
                );
            }
        }
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

    artifact_comparison_details(html, &workload.artifacts);

    html.push_str("</section>");
}

fn artifact_comparison_details(
    html: &mut BoundedHtml,
    artifacts: &[super::model::ArtifactComparison],
) {
    for artifact in artifacts {
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
        ObservationComparison::Incomparable {
            baseline,
            candidate,
        } => format!(
            "Incomparable. Baseline {}. Candidate {}.",
            observation_summary(baseline, unit),
            observation_summary(candidate, unit),
        ),
    };

    let _ = write!(
        html,
        "<dt>{}</dt><dd>{}</dd>",
        escape(label),
        rendered
    );
}

fn observation_summary(observation: &Observation, unit: MetricUnit) -> String {
    match observation {
        Observation::Measured { value, scope } => {
            let value = match unit {
                MetricUnit::Duration => milliseconds(*value),
                MetricUnit::Bytes => kibibytes(*value),
                MetricUnit::Count => grouped(*value),
            };

            format!("measured {value} in {}", escape(scope))
        }
        Observation::Unavailable { reason } => {
            format!("unavailable because {}", escape(reason))
        }
    }
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

    if let Some(contract) = &workload.peer_contract {
        let _ = write!(
            html,
            "<p><strong>Shared comparison contract:</strong> {}</p>",
            escape(contract),
        );
    }

    html.push_str("<h3>Observed work</h3><dl>");
    observation_detail(html, "Allocations", &workload.observations.allocation_count);
    observation_detail(html, "Allocated bytes", &workload.observations.allocated_bytes);
    observation_detail(html, "Copied bytes", &workload.observations.copied_bytes);

    for (operation, observation) in &workload.observations.platform_operations {
        observation_detail(html, operation, observation);
    }

    html.push_str("</dl>");
    compiler_details(html, &workload.compilation);
    artifact_details(html, &workload.artifacts);

    for (language, peer) in &workload.peers {
        let language = peer_language(*language);

        match peer {
            PeerOutcome::Measured { report } => {
                let _ = write!(
                    html,
                    "<details><summary>{language} peer details</summary><dl>\
                    <dt>Toolchain</dt><dd>{}</dd>\
                    <dt>Source SHA-256</dt><dd><code>{}</code></dd><dt>Compile and link</dt>\
                    <dd {}>{}</dd><dt>Controlled scope</dt><dd>{}</dd></dl>",
                    escape(&report.toolchain),
                    escape(&report.source_sha256),
                    nanoseconds_title(report.production_compile_link_nanoseconds),
                    milliseconds(report.production_compile_link_nanoseconds),
                    escape(&report.controlled_execution.scope),
                );

                peer_configuration(html, &report.build_configuration);

                html.push_str("<h4>Observed work</h4><dl>");
                observation_detail(html, "Allocations", &report.observations.allocation_count);
                observation_detail(html, "Allocated bytes", &report.observations.allocated_bytes);
                observation_detail(html, "Copied bytes", &report.observations.copied_bytes);
                html.push_str("</dl>");
                artifact_details(html, &report.artifacts);
                html.push_str("</details>");
            }
            PeerOutcome::Unsupported { reason } => {
                let _ = write!(
                    html,
                    "<p><strong>{language}</strong> is unsupported because {}.</p>",
                    escape(reason),
                );
            }
        }
    }

    html.push_str("</section>");
}

fn peer_configuration(
    html: &mut BoundedHtml,
    configuration: &super::model::PeerBuildConfiguration,
) {
    let _ = write!(
        html,
        "<h4>Build configuration</h4><dl><dt>Target</dt><dd><code>{}</code></dd>\
        <dt>Linker</dt><dd><code>{}</code></dd><dt>Runtime linkage</dt><dd>{}</dd></dl>",
        escape(&configuration.target),
        escape(&configuration.linker),
        escape(&configuration.runtime_linkage),
    );

    html.push_str("<h5>Production compiler arguments</h5><ul>");
    escaped_list(html, &configuration.production_arguments);
    html.push_str("</ul><h5>Timed compiler arguments</h5><ul>");
    escaped_list(html, &configuration.timed_arguments);
    html.push_str("</ul><h5>Post-link actions</h5><ul>");
    escaped_list(html, &configuration.post_link_actions);
    html.push_str("</ul>");
}

fn artifact_details(html: &mut BoundedHtml, artifacts: &[super::model::ArtifactReport]) {
    for artifact in artifacts {
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

const fn peer_language(language: super::model::PeerLanguage) -> &'static str {
    match language {
        super::model::PeerLanguage::Rust => "Rust",
        super::model::PeerLanguage::Cpp => "C++",
    }
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

#[cfg(test)]
mod tests {
    use super::{render_candidate, render_comparison};
    use super::super::format::{grouped, kibibytes, signed_kibibytes};
    use super::super::html::{BoundedHtml, MAX_HTML_BYTES, escape, finish};

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
    fn human_size_units_retain_exact_byte_values() {
        assert_eq!(kibibytes(1), "0.00 KiB (1 byte)");
        assert_eq!(kibibytes(1_024), "1.00 KiB (1,024 bytes)");
        assert_eq!(signed_kibibytes(-1), "-0.00 KiB (-1 byte)");
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
        assert!(first.contains("0.10 KiB (100 bytes)"));
        assert!(first.contains("Shared comparison contract"));
        assert!(first.contains("Rust peer details"));
        assert!(first.contains("C++ peer details"));
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

        assert!(first.contains(
            "Incomparable. Baseline unavailable because not observed. Candidate unavailable because not observed."
        ));

        assert!(first.contains(&comparison.candidate_identity.corpus_sha256));
    }
}
