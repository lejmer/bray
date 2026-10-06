use crate::DiagnosticLocale;
use crate::catalog::MessageCatalog;
use bray_profile::{CompilationProfileComparison, CompilationProfileReport};

/// Locale-aware renderer for concise compiler profile summaries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompilerProfileMessageRenderer {
    catalog: MessageCatalog,
}

impl CompilerProfileMessageRenderer {
    /// Creates a renderer for the selected locale.
    pub const fn new(locale: DiagnosticLocale) -> Self {
        Self {
            catalog: MessageCatalog::new(locale),
        }
    }

    /// Creates an English renderer.
    pub const fn english() -> Self {
        Self::new(DiagnosticLocale::English)
    }

    /// Renders a ranked human-readable summary of one compiler profile.
    pub fn summary(self, report: &CompilationProfileReport) -> String {
        self.catalog.compiler_profile_summary(report)
    }

    /// Renders a ranked human-readable comparison of two compiler profiles.
    pub fn comparison(self, comparison: CompilationProfileComparison<'_>) -> String {
        self.catalog.compiler_profile_comparison(comparison)
    }
}

#[cfg(test)]
mod tests {
    use super::CompilerProfileMessageRenderer;

    #[test]
    fn english_profile_summary_is_rendered_from_structured_report() {
        let renderer = CompilerProfileMessageRenderer::english();

        let mut report = bray_profile::CompilationProfileReport {
            schema_revision: bray_profile::COMPILATION_PROFILE_SCHEMA_REVISION,
            mode: bray_profile::CompilationProfileMode::Summary,
            context: bray_profile::CompilationProfileContext {
                package: "example".to_owned(),
                product: "application".to_owned(),
                target: "x86_64-test".to_owned(),
            },
            trace_event_limit: None,
            elapsed_nanoseconds: 1_250_000,
            time: bray_profile::CompilationProfileTimeBreakdown {
                active_work_nanoseconds: 1_000_000,
                same_thread_self_nanoseconds: 1_000_000,
                scheduler_queue_nanoseconds: 0,
                dependency_wait_nanoseconds: 0,
                external_work_nanoseconds: 0,
            },
            scheduler: bray_profile::CompilationProfileSchedulerStatistics {
                worker_budget: 1,
                active_worker_nanoseconds: 1_000_000,
                maximum_active_workers: 1,
                ready_waves: 0,
                ready_items: 0,
                maximum_ready_width: 0,
                wave_classes: Vec::new(),
                query_critical_path_nanoseconds: 1_000_000,
            },
            descriptors: bray_profile::CompilationProfileDescriptorCatalog {
                operations: Vec::new(),
                queries: Vec::new(),
                metrics: vec![bray_profile::CompilationProfileMetricDescriptor {
                    id: 1,
                    name: "total_wait".to_owned(),
                    unit: bray_profile::CompilationProfileUnit::Nanoseconds,
                    category: bray_profile::CompilationProfileCategory::Wait,
                    aggregation: bray_profile::CompilationProfileAggregation::Sum,
                    allowed_subjects: Vec::new(),
                }],
            },
            operations: Vec::new(),
            queries: Vec::new(),
            metrics: vec![bray_profile::CompilationProfileMetric {
                id: 1,
                value: 90_000_000_000,
            }],
            runtime_artifacts: vec![bray_profile::CompilationProfileRuntimeArtifact {
                identity: "bray.runtime.host".to_owned(),
                bytes: 4_096,
            }],
            runtime_roles: Vec::new(),
            native_callback_entries: Vec::new(),
            native_codegen: None,
            events: Vec::new(),
            dropped_events: 0,
        };

        let output = renderer.summary(&report);

        assert!(output.contains("Compiler profile: example/application"));
        assert!(output.contains("Elapsed"));
        assert!(output.contains("1.25 ms"));
        assert!(output.contains("Worker occupancy"));
        assert!(output.contains("80.0% of available worker time"));
        assert!(output.contains("0 hits (0.0%)"));
        assert!(output.contains("Query critical path"));
        assert!(output.contains("Selected runtime artifacts"));
        assert!(output.contains("bray.runtime.host"));
        assert!(output.contains("4.00 KiB"));
        assert!(output.lines().find(|line| line.contains("total wait")).expect("duration metric must render").ends_with("1.5 min"));

        for (nanoseconds, expected) in [
            (125_125, "125.125 us"),
            (999_999_999, "999.999 ms"),
            (1_234_567_890, "1.234 s"),
            (90_000_000_000, "1.5 min"),
        ] {
            report.elapsed_nanoseconds = nanoseconds;

            let output = renderer.summary(&report);

            assert!(output.lines().find(|line| line.starts_with("Elapsed")).expect("elapsed duration must render").ends_with(expected));
        }
    }
}
