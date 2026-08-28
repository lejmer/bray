use std::fmt::Write as _;

use super::format::{grouped, milliseconds, nanoseconds_title};
use super::html::{BoundedHtml, escape};
use super::model::{
    CompilationBuildReport, CompilationComparability, CompilationComparisonReport,
    CompilationIncomparability, CompilationLanguage, LinkerInvocationReport,
    PeerCompilerConfiguration, PeerLanguage, PerformanceReport, ReportIdentity, RuntimeLinkage,
    ToolInvocationReport, WorkloadBatching, WorkloadCompilationReport,
};

pub(super) fn workload_compilation_summary(
    html: &mut BoundedHtml,
    report: &PerformanceReport,
) -> Result<(), String> {
    let rows = [
        ("Bray", workload_compilation_totals(report, None)?),
        (
            "Rust",
            workload_compilation_totals(report, Some(PeerLanguage::Rust))?,
        ),
        (
            "C++",
            workload_compilation_totals(report, Some(PeerLanguage::Cpp))?,
        ),
    ];

    let process_winner = rows.iter().map(|(_, (process, _))| *process).min();

    let compiler_winner = rows.iter().map(|(_, (_, compiler))| *compiler).min();

    html.push_str(
        "<section><h2>Matched workload compilation</h2><p>Totals cover every selected workload.</p>\
        <div class=\"table-scroll\"><table><thead><tr><th>Language</th><th>Programs</th>\
        <th>Compiler process</th><th>Compiler work</th></tr></thead><tbody>",
    );

    for (language, (process, compiler)) in rows {
        let _ = write!(
            html,
            "<tr><th>{language}</th><td>{}</td><td {} {}>{}</td><td {} {}>{}</td></tr>",
            report.workloads.len(),
            compilation_winner_class(process, process_winner),
            nanoseconds_title(process),
            milliseconds(process),
            compilation_winner_class(compiler, compiler_winner),
            nanoseconds_title(compiler),
            milliseconds(compiler),
        );
    }

    html.push_str("</tbody></table></div></section>");

    Ok(())
}

fn workload_compilation_totals(
    report: &PerformanceReport,
    peer: Option<PeerLanguage>,
) -> Result<(u64, u64), String> {
    report.workloads.iter().try_fold(
        (0_u64, 0_u64),
        |(process_total, compiler_total), workload| {
            let timing = match peer {
                Some(language) => {
                    &workload
                        .peers
                        .get(&language)
                        .ok_or_else(|| format!("workload {} is missing {language:?}", workload.id))?
                        .compilation
                }
                None => &workload.compilation,
            };

            let process_total = process_total
                .checked_add(timing.process_elapsed_nanoseconds)
                .ok_or_else(|| "compiler process total exceeds the report bound".to_owned())?;

            let compiler_total = compiler_total
                .checked_add(timing.compiler_elapsed_nanoseconds)
                .ok_or_else(|| "compiler work total exceeds the report bound".to_owned())?;

            Ok((process_total, compiler_total))
        },
    )
}

pub(super) fn workload_compilation_details(
    html: &mut BoundedHtml,
    language: &str,
    report: &WorkloadCompilationReport,
) {
    let _ = write!(
        html,
        "<details><summary>{} compilation details</summary><dl>\
        <dt>Compiler process</dt><dd {}>{}</dd><dt>Compiler work</dt><dd {}>{}</dd>",
        language,
        nanoseconds_title(report.process_elapsed_nanoseconds),
        milliseconds(report.process_elapsed_nanoseconds),
        nanoseconds_title(report.compiler_elapsed_nanoseconds),
        milliseconds(report.compiler_elapsed_nanoseconds),
    );

    for (component, elapsed) in &report.compiler_components_nanoseconds {
        let _ = write!(
            html,
            "<dt>{}</dt><dd {}>{}</dd>",
            escape(component),
            nanoseconds_title(*elapsed),
            milliseconds(*elapsed),
        );
    }

    html.push_str("</dl>");

    tool_invocation(
        html,
        "Measured compiler process",
        &report.process_invocation,
    );

    tool_invocation(
        html,
        "Profiled compiler invocation",
        &report.profiled_invocation,
    );

    html.push_str("</details>");
}

pub(super) fn identity(html: &mut BoundedHtml, label: &str, identity: &ReportIdentity) {
    let _ = write!(
        html,
        "<section><h2>{label}</h2><dl class=\"identity\"><dt>Target</dt><dd>{}</dd>\
        <dt>Host</dt><dd>{}</dd><dt>Compiler</dt><dd>{}</dd><dt>Source revision</dt>\
        <dd><code>{}</code></dd><dt>LLVM</dt><dd>{}</dd><dt>Runtime linkage</dt><dd>{}</dd>\
        <dt>Corpus SHA-256</dt>\
        <dd><code>{}</code></dd><dt>Samples</dt><dd>{} warmup and {} measured</dd>\
        <dt>Timer resolution</dt><dd>{} ns</dd></dl></section>",
        escape(&identity.target),
        escape(&identity.host),
        escape(&identity.compiler_version),
        escape(&identity.source_revision),
        escape(&identity.llvm_version),
        runtime_linkage(identity.runtime_linkage),
        escape(&identity.corpus_sha256),
        identity.warmup_iterations,
        identity.sample_iterations,
        grouped(identity.timer_resolution_nanoseconds),
    );
}

pub(super) fn compiler_configuration(
    html: &mut BoundedHtml,
    label: &str,
    configuration: &PeerCompilerConfiguration,
) {
    let batching = match configuration.batching {
        super::model::PeerBatching::SingleExecution => "single execution".to_owned(),
        super::model::PeerBatching::Repeated { inner_iterations } => {
            format!("{} inner iterations", grouped(inner_iterations))
        }
    };

    let _ = write!(
        html,
        "<h5>{label} compiler invocation</h5><p>Batching: {batching}</p><ul>"
    );

    for argument in &configuration.arguments {
        let _ = write!(html, "<li><code>{}</code></li>", escape(argument));
    }

    for (name, value) in &configuration.environment {
        let _ = write!(
            html,
            "<li><code>{}={}</code></li>",
            escape(name),
            escape(value)
        );
    }

    html.push_str("</ul>");
}

pub(super) fn compiler_details(
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

    html.push_str("</dl><h4>Runtime roles</h4>");
    compiler_contract_list(html, &profile.runtime_roles);

    html.push_str("<h4>Native callback entries</h4>");
    compiler_contract_list(html, &profile.native_callback_entries);

    html.push_str("</details>");
}

fn compiler_contract_list(html: &mut BoundedHtml, entries: &[String]) {
    if entries.is_empty() {
        html.push_str("<p>None</p>");

        return;
    }

    html.push_str("<ul>");

    for entry in entries {
        let _ = write!(html, "<li><code>{}</code></li>", escape(entry));
    }

    html.push_str("</ul>");
}

const fn runtime_linkage(linkage: RuntimeLinkage) -> &'static str {
    match linkage {
        RuntimeLinkage::StaticApplicationRuntime => {
            "application and language runtimes linked into each executable"
        }
    }
}

pub(super) fn compilation_comparison(
    html: &mut BoundedHtml,
    title: &str,
    report: &CompilationComparisonReport,
) {
    let _ = write!(
        html,
        "<section><h2>{}</h2><p>{}</p>",
        escape(title),
        escape(&report.contract),
    );

    comparability(html, &report.comparability);

    let winner = matches!(report.comparability, CompilationComparability::Comparable)
        .then(|| {
            report
                .builds
                .values()
                .map(|build| build.elapsed_nanoseconds)
                .min()
        })
        .flatten();

    html.push_str(
        "<div class=\"table-scroll\"><table><thead><tr><th>Language</th>\
        <th>Duration</th><th>Source units</th><th>Source bytes</th><th>Library authority</th>\
        </tr></thead><tbody>",
    );

    for (language, build) in &report.builds {
        let _ = write!(
            html,
            "<tr><th>{}</th><td {} {}>{}</td><td>{}</td><td>{}</td><td>{:?}</td></tr>",
            language_name(*language),
            compilation_winner_class(build.elapsed_nanoseconds, winner),
            nanoseconds_title(build.elapsed_nanoseconds),
            milliseconds(build.elapsed_nanoseconds),
            grouped(build.authority.source_units),
            grouped(build.authority.source_bytes),
            build.authority.library_reuse,
        );
    }

    html.push_str("</tbody></table></div>");

    for (language, build) in &report.builds {
        compilation_build(html, *language, build);
    }

    html.push_str("</section>");
}

const fn compilation_winner_class(value: u64, winner: Option<u64>) -> &'static str {
    if matches!(winner, Some(winner) if value == winner) {
        "class=\"metric-best\""
    } else {
        ""
    }
}

fn comparability(html: &mut BoundedHtml, comparability: &CompilationComparability) {
    match comparability {
        CompilationComparability::Comparable => {
            html.push_str("<p><strong>Comparable.</strong> Every row satisfies the source-authority contract.</p>");
        }
        CompilationComparability::Incomparable { reasons } => {
            html.push_str(
                "<p><strong>Incomparable.</strong> No compilation winner is reported.</p><ul>",
            );

            for (language, reasons) in reasons {
                let reasons = reasons
                    .iter()
                    .map(incomparability_reason)
                    .collect::<Vec<_>>()
                    .join(", ");

                let _ = write!(
                    html,
                    "<li>{}: {}</li>",
                    language_name(*language),
                    escape(&reasons),
                );
            }

            html.push_str("</ul>");
        }
    }
}

fn incomparability_reason(reason: &CompilationIncomparability) -> &'static str {
    match reason {
        CompilationIncomparability::MissingImplementation => "implementation is missing",
        CompilationIncomparability::MissingSourceAuthority => "source authority is missing",
        CompilationIncomparability::MissingPackageInputs => "package inputs are missing",
        CompilationIncomparability::MissingModuleInputs => "module inputs are missing",
        CompilationIncomparability::DifferentSourceUnitCount => {
            "source unit count differs from the other languages"
        }
        CompilationIncomparability::DifferentSourceByteScale => {
            "source byte count exceeds the cross-language syntax tolerance"
        }
        CompilationIncomparability::DifferentPackageInputCount => {
            "package input count differs from the other languages"
        }
        CompilationIncomparability::DifferentModuleInputCount => {
            "module input count differs from the other languages"
        }
        CompilationIncomparability::MissingPackagedLibraryArtifact => {
            "packaged library artifact is missing"
        }
        CompilationIncomparability::MissingRuntimeArtifact => "runtime artifact is missing",
        CompilationIncomparability::UnexpectedReusedArtifact => {
            "source-library build reused a packaged artifact"
        }
        CompilationIncomparability::CompilesLibrarySourceForApplication => {
            "application build compiled library source"
        }
        CompilationIncomparability::ReusesPackagedLibraryForLibraryBuild => {
            "library build reused a packaged library"
        }
    }
}

fn compilation_build(
    html: &mut BoundedHtml,
    language: CompilationLanguage,
    build: &CompilationBuildReport,
) {
    let _ = write!(
        html,
        "<details><summary>{} compilation details</summary><dl>\
        <dt>Toolchain</dt><dd>{}</dd><dt>Source SHA-256</dt><dd><code>{}</code></dd>\
        <dt>Packages</dt><dd>{}</dd><dt>Modules</dt><dd>{}</dd>\
        <dt>Packaged-library artifacts</dt><dd>{} shown, {} omitted</dd>\
        <dt>Runtime artifacts</dt><dd>{} shown, {} omitted</dd></dl>",
        language_name(language),
        escape(&build.toolchain),
        escape(&build.source_sha256),
        escaped_join(&build.authority.packages),
        escaped_join(&build.authority.modules),
        build.reuse.packaged_library.entries.len(),
        build.reuse.packaged_library.omitted_count,
        build.reuse.runtime.entries.len(),
        build.reuse.runtime.omitted_count,
    );

    tool_invocation(html, "Compiler invocation", &build.compiler);
    linker_invocation(html, &build.linker);

    reused_artifacts(
        html,
        "Packaged-library artifacts",
        &build.reuse.packaged_library,
    );

    reused_artifacts(html, "Runtime artifacts", &build.reuse.runtime);

    if let Some(evidence) = &build.evidence {
        tool_invocation(html, "Evidence compiler invocation", &evidence.compiler);

        if let Some(map) = &evidence.linker_map {
            let _ = write!(
                html,
                "<p>Linker evidence: {} bytes, SHA-256 <code>{}</code></p>",
                grouped(map.bytes),
                escape(&map.sha256),
            );
        }
    }

    html.push_str("</details>");
}

fn reused_artifacts(
    html: &mut BoundedHtml,
    title: &str,
    artifacts: &super::model::BoundedList<super::model::RetainedInput>,
) {
    let _ = write!(html, "<h4>{}</h4><ul>", escape(title));

    for artifact in &artifacts.entries {
        let identity = artifact.member.as_ref().map_or_else(
            || artifact.artifact.clone(),
            |member| format!("{} ({member})", artifact.artifact),
        );

        let _ = write!(html, "<li><code>{}</code></li>", escape(&identity));
    }

    html.push_str("</ul>");
}

fn linker_invocation(html: &mut BoundedHtml, linker: &LinkerInvocationReport) {
    match linker {
        LinkerInvocationReport::IntegratedCompilerDriver { driver, arguments } => {
            let _ = write!(
                html,
                "<h4>Linker invocation</h4><p>Integrated compiler driver: <code>{}</code></p>",
                escape(driver),
            );

            string_list(html, arguments);
        }
        LinkerInvocationReport::NotApplicable => {
            html.push_str("<h4>Linker invocation</h4><p>Not applicable.</p>");
        }
    }
}

fn tool_invocation(html: &mut BoundedHtml, title: &str, invocation: &ToolInvocationReport) {
    let _ = write!(
        html,
        "<h4>{}</h4><p><code>{}</code></p>",
        escape(title),
        escape(&invocation.program),
    );

    string_list(html, &invocation.arguments);

    if !invocation.environment.is_empty() {
        html.push_str("<h5>Environment</h5><ul>");

        for (name, value) in &invocation.environment {
            let _ = write!(
                html,
                "<li><code>{}={}</code></li>",
                escape(name),
                escape(value),
            );
        }

        html.push_str("</ul>");
    }

    if !invocation.response_files.is_empty() {
        html.push_str("<h5>Response files</h5><ul>");

        for file in &invocation.response_files {
            let _ = write!(
                html,
                "<li><code>{}</code>, {} encoded bytes</li>",
                escape(&file.path),
                grouped(u64::try_from(file.contents_hex.len() / 2).unwrap_or(u64::MAX)),
            );
        }

        html.push_str("</ul>");
    }
}

fn string_list(html: &mut BoundedHtml, entries: &[String]) {
    html.push_str("<ul>");

    for entry in entries {
        let _ = write!(html, "<li><code>{}</code></li>", escape(entry));
    }

    html.push_str("</ul>");
}

fn escaped_join(entries: &[String]) -> String {
    entries
        .iter()
        .map(|entry| format!("<code>{}</code>", escape(entry)))
        .collect::<Vec<_>>()
        .join(", ")
}

const fn language_name(language: CompilationLanguage) -> &'static str {
    match language {
        CompilationLanguage::Bray => "Bray",
        CompilationLanguage::Rust => "Rust",
        CompilationLanguage::Cpp => "C++",
    }
}

pub(super) fn batching_detail(html: &mut BoundedHtml, batching: &WorkloadBatching) {
    match batching {
        WorkloadBatching::SingleExecution => {
            html.push_str("<dl><dt>Batch selection</dt><dd>single execution</dd></dl>");
        }
        WorkloadBatching::Calibrated {
            seed_inner_iterations,
            target_interval_nanoseconds,
            bray_samples_nanoseconds,
            rust_samples_nanoseconds,
            cpp_samples_nanoseconds,
            selected_inner_iterations,
        } => {
            let samples = |values: &[u64]| {
                values
                    .iter()
                    .map(|value| grouped(*value))
                    .collect::<Vec<_>>()
                    .join(", ")
            };

            let _ = write!(
                html,
                "<dl><dt>Batch selection</dt><dd>calibrated outside measured samples</dd>\
                <dt>Calibration seed</dt><dd>{} iterations</dd><dt>Target interval</dt>\
                <dd>{}</dd><dt>Selected batch</dt><dd>{} iterations</dd>\
                <dt>Bray calibration intervals</dt><dd>{} ns</dd>\
                <dt>Rust calibration intervals</dt><dd>{} ns</dd>\
                <dt>C++ calibration intervals</dt><dd>{} ns</dd></dl>",
                grouped(*seed_inner_iterations),
                milliseconds(*target_interval_nanoseconds),
                grouped(*selected_inner_iterations),
                samples(bray_samples_nanoseconds),
                samples(rust_samples_nanoseconds),
                samples(cpp_samples_nanoseconds),
            );
        }
    }
}
