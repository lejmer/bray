use std::fmt::Write as _;
use std::path::Path;

use super::model::{
    ArtifactKind, ChangeAssessment, ComparisonReport, CompilationLanguage, MetricComparison,
    Observation, ObservationComparison, PerformanceReport,
};

use super::format::{
    grouped, kibibytes, milliseconds, picoseconds_milliseconds, picoseconds_title,
    signed_kibibytes, signed_milliseconds, signed_picoseconds_milliseconds,
};
use super::html::{BoundedHtml, document_start, escape, finish, write};
use super::ranking::{CandidateWinners, executable_bytes, observation_value};

pub(super) fn write_candidate(path: &Path, report: &PerformanceReport) -> Result<(), String> {
    write(path, render_candidate(report)?)
}

fn render_candidate(report: &PerformanceReport) -> Result<String, String> {
    let mut html = document_start("Compiler performance report");

    super::presentation::identity(&mut html, "Candidate", &report.identity);

    html.push_str(
        "<section><h2>How to read this report</h2>\
        <p>Compiler process duration includes compiler startup and teardown. Compiler work uses each compiler's \
        own timing and excludes compiler process startup and teardown.</p>\
        <p>The primary execution duration is the language-controlled workload execution. Process duration also includes \
        executable startup and teardown. Throughput uses the controlled duration. Very small workloads repeat \
        inside one controlled interval, and the reported duration is adjusted to one workload execution.</p>\
        <p>Bray, Rust, and C++ embed their application and language runtimes in each executable. \
        Target operating-system libraries may remain dynamic.</p></section>",
    );

    super::presentation::compilation_comparison(
        &mut html,
        "Matched packaged-application compilation",
        &report.application_compilation,
    );

    super::presentation::compilation_comparison(
        &mut html,
        "Matched source-library compilation",
        &report.library_compilation,
    );

    html.push_str(
        "<section><h2>Packaged optimization artifacts</h2><div class=\"table-scroll\"><table><thead><tr>\
        <th>Partition</th><th>Artifact</th><th>Size</th><th>Fallback</th><th>Selected by workloads</th>\
        </tr></thead><tbody>",
    );

    for artifact in &report.optimization_artifacts {
        let selected = if artifact.selected_by_workloads.is_empty() {
            "None in this run".to_owned()
        } else {
            artifact.selected_by_workloads.join(", ")
        };

        let _ = write!(
            html,
            "<tr><th>{}</th><td><code>{}</code></td><td>{}</td><td><code>{}</code></td><td>{}</td></tr>",
            escape(&artifact.partition),
            escape(&artifact.path),
            kibibytes(artifact.bytes),
            escape(&artifact.fallback),
            escape(&selected),
        );
    }

    html.push_str("</tbody></table></div></section>");

    super::presentation::workload_compilation_summary(&mut html, report)?;

    html.push_str(
        "<section><h2>Workloads</h2><div class=\"table-scroll\"><table><thead><tr>\
        <th>Workload</th><th>Language</th><th>Compiler process</th><th>Compiler work</th>\
        <th>Controlled median</th><th>Controlled MAD</th><th>Process median</th>\
        <th>Throughput</th><th>Executable</th><th>Allocations</th><th>Allocated</th><th>Copied</th>
        </tr></thead><tbody>",
    );

    for workload in &report.workloads {
        let winners = CandidateWinners::for_workload(workload);

        candidate_row(
            &mut html,
            workload,
            "Bray",
            true,
            &workload.compilation,
            &workload.bray_execution,
            &workload.process_execution,
            &workload.artifacts,
            &workload.observations,
            &winners,
        );

        for (language, peer) in &workload.peers {
            candidate_row(
                &mut html,
                workload,
                peer_language(*language),
                false,
                &peer.compilation,
                &peer.controlled_execution,
                &peer.process_execution,
                &peer.artifacts,
                &peer.observations,
                &winners,
            );
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
    is_bray: bool,
    compilation: &super::model::WorkloadCompilationReport,
    controlled: &super::model::ExecutionStatistics,
    process: &super::model::ExecutionStatistics,
    artifacts: &[super::model::ArtifactReport],
    observations: &super::model::WorkloadObservations,
    winners: &CandidateWinners,
) {
    let executable = executable_bytes(artifacts);
    let allocation_count = observation_value(&observations.allocation_count);
    let allocated_bytes = observation_value(&observations.allocated_bytes);
    let copied_bytes = observation_value(&observations.copied_bytes);
    let row_class = if is_bray { " class=\"bray-row\"" } else { "" };

    let _ = write!(
        html,
        "<tr{row_class}><th>{}</th><td>{}</td><td {} {}>{}</td><td {} {}>{}</td>\
        <td {} {}>{}</td><td {} {}>{}</td><td {} {}>{}</td>\
        <td {}>{} {}/s</td><td {}>{}</td><td {}>{}</td><td {}>{}</td><td {}>{}</td></tr>",
        escape(&workload.id),
        language,
        winner_class(
            Some(compilation.process_elapsed_nanoseconds),
            winners.compilation_process
        ),
        super::format::nanoseconds_title(compilation.process_elapsed_nanoseconds),
        milliseconds(compilation.process_elapsed_nanoseconds),
        winner_class(
            Some(compilation.compiler_elapsed_nanoseconds),
            winners.compiler_work
        ),
        super::format::nanoseconds_title(compilation.compiler_elapsed_nanoseconds),
        milliseconds(compilation.compiler_elapsed_nanoseconds),
        winner_class(
            Some(controlled.median_picoseconds),
            winners.controlled_median
        ),
        picoseconds_title(controlled.median_picoseconds),
        picoseconds_milliseconds(controlled.median_picoseconds),
        winner_class(
            Some(controlled.median_absolute_deviation_picoseconds),
            winners.controlled_mad
        ),
        picoseconds_title(controlled.median_absolute_deviation_picoseconds),
        picoseconds_milliseconds(controlled.median_absolute_deviation_picoseconds),
        winner_class(Some(process.median_picoseconds), winners.process_median),
        picoseconds_title(process.median_picoseconds),
        picoseconds_milliseconds(process.median_picoseconds),
        winner_class(Some(controlled.median_units_per_second), winners.throughput),
        grouped(controlled.median_units_per_second),
        escape(&workload.units),
        winner_class(executable, winners.executable_bytes),
        executable.map_or_else(|| "Unavailable".to_owned(), kibibytes),
        winner_class(allocation_count, winners.allocation_count),
        observation_count(&observations.allocation_count),
        winner_class(allocated_bytes, winners.allocated_bytes),
        observation_bytes(&observations.allocated_bytes),
        winner_class(copied_bytes, winners.copied_bytes),
        observation_bytes(&observations.copied_bytes),
    );
}

const fn winner_class(value: Option<u64>, winner: Option<u64>) -> &'static str {
    if matches!((value, winner), (Some(value), Some(winner)) if value == winner) {
        "class=\"metric-best\""
    } else {
        ""
    }
}

pub(super) fn write_comparison(path: &Path, comparison: &ComparisonReport) -> Result<(), String> {
    write(path, render_comparison(comparison)?)
}

fn compilation_changes(
    html: &mut BoundedHtml,
    title: &str,
    changes: &std::collections::BTreeMap<CompilationLanguage, MetricComparison>,
) {
    let _ = write!(
        html,
        "<section><h2>{}</h2><div class=\"table-scroll\"><table><thead><tr>\
        <th>Language</th><th>Baseline</th><th>Candidate</th><th>Change</th></tr></thead><tbody>",
        escape(title),
    );

    for (language, change) in changes {
        let _ = write!(
            html,
            "<tr><th>{language:?}</th><td>{}</td><td>{}</td><td>{}</td></tr>",
            milliseconds(change.baseline),
            milliseconds(change.candidate),
            comparison_metric(*change, MetricUnit::Duration),
        );
    }

    html.push_str("</tbody></table></div></section>");
}

fn render_comparison(comparison: &ComparisonReport) -> Result<String, String> {
    let mut html = document_start("Compiler performance comparison");

    super::presentation::identity(&mut html, "Baseline", &comparison.baseline_identity);
    super::presentation::identity(&mut html, "Candidate", &comparison.candidate_identity);

    compilation_changes(
        &mut html,
        "Matched packaged-application compilation changes",
        &comparison.application_compilation,
    );

    compilation_changes(
        &mut html,
        "Matched source-library compilation changes",
        &comparison.library_compilation,
    );

    html.push_str(
        "<section><h2>Changes</h2><p>Negative duration and size changes are improvements. \
        Timing changes within the noise boundary are marked indeterminate.</p><div class=\"table-scroll\">\
        <table><thead><tr><th>Workload</th><th>Language</th><th>Compiler process</th><th>Compiler work</th>\
        <th>Controlled duration</th><th>Process duration</th>\
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
            "<tr class=\"bray-row\"><th>{}</th><td>Bray</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            escape(&workload.id),
            comparison_metric(workload.compilation_process, MetricUnit::Duration),
            comparison_metric(workload.compiler_work, MetricUnit::Duration),
            comparison_metric(workload.bray_execution, MetricUnit::PicosecondsDuration),
            comparison_metric(workload.process_execution, MetricUnit::PicosecondsDuration),
            executable.map_or_else(
                || "Unavailable".to_owned(),
                |artifact| comparison_metric(artifact.bytes, MetricUnit::Bytes)
            ),
            retained,
        );

        for (language, peer) in &workload.peers {
            comparison_row(
                &mut html,
                &workload.id,
                peer_language(*language),
                peer.compilation_process,
                peer.compiler_work,
                peer.controlled_execution,
                peer.process_execution,
                &peer.artifacts,
            );
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
    compilation_process: MetricComparison,
    compiler_work: MetricComparison,
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
        "<tr><th>{}</th><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
        escape(workload),
        language,
        comparison_metric(compilation_process, MetricUnit::Duration),
        comparison_metric(compiler_work, MetricUnit::Duration),
        comparison_metric(controlled, MetricUnit::PicosecondsDuration),
        comparison_metric(process, MetricUnit::PicosecondsDuration),
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

        let _ = write!(html, "<details><summary>{language} peer changes</summary>");

        artifact_comparison_details(html, &peer.artifacts);
        html.push_str("</details>");
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
        html.push_str("</ul><h4>Added logical provenance</h4><ul>");
        escaped_list(html, &artifact.added_logical_provenance);
        html.push_str("</ul><h4>Removed logical provenance</h4><ul>");
        escaped_list(html, &artifact.removed_logical_provenance);
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

    let _ = write!(html, "<dt>{}</dt><dd>{}</dd>", escape(label), rendered);
}

fn observation_summary(observation: &Observation, unit: MetricUnit) -> String {
    match observation {
        Observation::Measured { value, scope } => {
            let value = match unit {
                MetricUnit::Duration => milliseconds(*value),
                MetricUnit::PicosecondsDuration => picoseconds_milliseconds(*value),
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

    measurement_detail(html, "Bray", &workload.bray_execution);
    super::presentation::workload_compilation_details(html, "Bray", &workload.compilation);
    super::presentation::batching_detail(html, &workload.batching);

    let _ = write!(
        html,
        "<p><strong>Shared comparison contract:</strong> {}</p>",
        escape(&workload.peer_contract),
    );

    html.push_str("<h3>Observed work</h3><dl>");
    observation_detail(html, "Allocations", &workload.observations.allocation_count);

    observation_detail(
        html,
        "Allocated bytes",
        &workload.observations.allocated_bytes,
    );

    observation_detail(html, "Copied bytes", &workload.observations.copied_bytes);

    for (operation, observation) in &workload.observations.platform_operations {
        observation_detail(html, operation, observation);
    }

    html.push_str("</dl>");
    super::presentation::compiler_details(html, &workload.compiler_profile);

    artifact_details(html, &workload.artifacts);

    for (language, peer) in &workload.peers {
        let language = peer_language(*language);

        let _ = write!(
            html,
            "<details><summary>{language} execution details</summary><dl>\
            <dt>Controlled scope</dt><dd>{}</dd></dl>",
            escape(&peer.controlled_execution.scope),
        );

        measurement_detail(html, language, &peer.controlled_execution);
        super::presentation::workload_compilation_details(html, language, &peer.compilation);

        let configuration = &peer.build_configuration;

        super::presentation::compiler_configuration(html, "Production", &configuration.production);

        super::presentation::compiler_configuration(html, "Timed", &configuration.timed);

        html.push_str("<h5>Post-link actions</h5><ul>");
        escaped_list(html, &configuration.post_link_actions);
        html.push_str("</ul>");

        html.push_str("<h4>Observed work</h4><dl>");
        observation_detail(html, "Allocations", &peer.observations.allocation_count);
        observation_detail(html, "Allocated bytes", &peer.observations.allocated_bytes);
        observation_detail(html, "Copied bytes", &peer.observations.copied_bytes);
        html.push_str("</dl>");
        artifact_details(html, &peer.artifacts);
        html.push_str("</details>");
    }

    html.push_str("</section>");
}

fn measurement_detail(
    html: &mut BoundedHtml,
    language: &str,
    execution: &super::model::ExecutionStatistics,
) {
    let raw_median = super::statistics::median(&execution.raw_samples_nanoseconds);

    let _ = write!(
        html,
        "<dl><dt>{language} controlled measurement</dt><dd>{} inner iterations, raw median {} ns, \
        adjusted median {} ps, timer resolution {} ns</dd></dl>",
        grouped(execution.inner_iterations),
        grouped(raw_median),
        grouped(execution.median_picoseconds),
        grouped(execution.timer_resolution_nanoseconds),
    );
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

        html.push_str("</ul><h4>Logical provenance</h4><ul>");

        if let Some(map) = &artifact.linker_map {
            escaped_list(html, &map.logical_provenance.entries);
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
    PicosecondsDuration,
    Bytes,
    Count,
}

fn comparison_metric(metric: MetricComparison, unit: MetricUnit) -> String {
    let delta = match unit {
        MetricUnit::Duration => signed_milliseconds(metric.delta),
        MetricUnit::PicosecondsDuration => signed_picoseconds_milliseconds(metric.delta),
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
    use super::super::format::{grouped, kibibytes, signed_kibibytes};
    use super::super::html::{BoundedHtml, MAX_HTML_BYTES, escape, finish};
    use super::super::model::PeerLanguage;
    use super::super::ranking::CandidateWinners;
    use super::{render_candidate, render_comparison};

    #[test]
    fn html_escaping_covers_text_and_attribute_delimiters() {
        assert_eq!(escape("<&>\"'"), "&lt;&amp;&gt;&quot;&#39;");
    }

    #[test]
    fn grouped_integers_use_digit_separators() {
        assert_eq!(grouped(999), "999");
        assert_eq!(grouped(1_000), "1,000");
        assert_eq!(grouped(1_234_567), "1,234,567");
    }

    #[test]
    fn size_units_retain_exact_byte_values() {
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
        assert!(first.contains("Rust execution details"));
        assert!(first.contains("C++ execution details"));
        assert!(first.contains(&report.identity.corpus_sha256));
        assert!(first.contains("<tr class=\"bray-row\">"));
        assert_eq!(first.matches("class=\"metric-best\"").count(), 32);
        assert!(!first.contains("<artifact>"));
    }

    #[test]
    fn candidate_winners_use_direction_and_require_complete_measurements() {
        let mut report = super::super::tests::report("corpus", 2_500_000, 100_000);
        let workload = &mut report.workloads[0];

        workload.bray_execution.median_units_per_second = 20;

        let rust = workload
            .peers
            .get_mut(&PeerLanguage::Rust)
            .unwrap_or_else(|| panic!("Rust peer must exist"));

        rust.controlled_execution.median_picoseconds = 1_500_000_000;
        rust.controlled_execution.median_units_per_second = 30;

        let cpp = workload
            .peers
            .get_mut(&PeerLanguage::Cpp)
            .unwrap_or_else(|| panic!("C++ peer must exist"));

        cpp.controlled_execution.median_picoseconds = 3_500_000_000;
        cpp.controlled_execution.median_units_per_second = 10;

        let winners = CandidateWinners::for_workload(workload);

        assert_eq!(winners.controlled_median, Some(1_500_000_000));
        assert_eq!(winners.throughput, Some(30));
        assert_eq!(winners.executable_bytes, Some(80));
        assert_eq!(winners.allocation_count, None);
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
        assert!(first.contains("<tr class=\"bray-row\">"));

        assert!(first.contains(
            "Incomparable. Baseline unavailable because not observed. Candidate unavailable because not observed."
        ));

        assert!(first.contains(&comparison.candidate_identity.corpus_sha256));
    }
}
