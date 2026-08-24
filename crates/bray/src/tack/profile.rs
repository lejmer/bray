use std::path::{Path, PathBuf};
use std::process::ExitCode;

use bray_diagnostics::{
    DiagnosticBag, DiagnosticDocumentParseKind, DiagnosticIoErrorKind,
    DiagnosticProfileComparisonProblem, DiagnosticProfileContext, DiagnosticProfileDescriptorKind,
    DiagnosticProfileValidationProblem, DiagnosticProjectCommandFailure,
    DiagnosticProjectOperation,
};
use bray_messages::CompilerProfileMessageRenderer;
use bray_profile::{
    CompilationProfileComparison, CompilationProfileComparisonError, CompilationProfileReport,
    CompilationProfileValidationError,
};
use bray_tooling::OutputFormat;

use crate::tack::error::operation_diagnostics;
use crate::tack::output::failure;
use crate::tack::result::TackRunResult;

pub(super) fn run_profile_command(
    first: PathBuf,
    second: Option<PathBuf>,
    output_format: OutputFormat,
) -> TackRunResult {
    let first = match load_profile(&first) {
        Ok(profile) => profile,
        Err(diagnostics) => return failure(diagnostics, output_format),
    };

    let renderer = CompilerProfileMessageRenderer::english();

    let output = match second {
        Some(path) => {
            let second = match load_profile(&path) {
                Ok(profile) => profile,
                Err(diagnostics) => return failure(diagnostics, output_format),
            };

            let comparison = match CompilationProfileComparison::new(&first, &second) {
                Ok(comparison) => comparison,
                Err(error) => {
                    return failure(
                        operation_diagnostics(DiagnosticProjectCommandFailure::ProfileComparison(
                            match error {
                                CompilationProfileComparisonError::Context { before, after } => {
                                    DiagnosticProfileComparisonProblem::Context {
                                        before: diagnostic_profile_context(before),
                                        after: diagnostic_profile_context(after),
                                    }
                                }
                                CompilationProfileComparisonError::Descriptor { kind, id } => {
                                    DiagnosticProfileComparisonProblem::Descriptor {
                                        kind: diagnostic_descriptor_kind(kind),
                                        id,
                                    }
                                }
                            },
                        )),
                        output_format,
                    );
                }
            };

            renderer.comparison(comparison)
        }
        None => renderer.summary(&first),
    };

    TackRunResult::with_output(
        ExitCode::SUCCESS,
        DiagnosticBag::new(),
        output_format,
        output,
        String::new(),
    )
}

fn load_profile(path: &Path) -> Result<CompilationProfileReport, DiagnosticBag> {
    let bytes = std::fs::read(path).map_err(|error| {
        operation_diagnostics(DiagnosticProjectCommandFailure::Io {
            operation: DiagnosticProjectOperation::ProfileReportRead,
            path: path.to_owned(),
            error: DiagnosticIoErrorKind::from(error.kind()),
        })
    })?;

    let report = serde_json::from_slice::<CompilationProfileReport>(&bytes).map_err(|error| {
        operation_diagnostics(DiagnosticProjectCommandFailure::Document {
            operation: DiagnosticProjectOperation::ProfileReportDecode,
            path: Some(path.to_owned()),
            problem: if error.is_eof() {
                DiagnosticDocumentParseKind::UnexpectedEnd
            } else if error.is_syntax() {
                DiagnosticDocumentParseKind::Syntax
            } else if error.is_data() {
                DiagnosticDocumentParseKind::Schema
            } else {
                DiagnosticDocumentParseKind::Input
            },
        })
    })?;

    report.validate().map_err(|error| {
        operation_diagnostics(DiagnosticProjectCommandFailure::ProfileValidation {
            path: path.to_owned(),
            problem: match error {
                CompilationProfileValidationError::SchemaRevision { expected, actual } => {
                    DiagnosticProfileValidationProblem::SchemaRevision { expected, actual }
                }
                CompilationProfileValidationError::DuplicateDescriptor { kind, id } => {
                    DiagnosticProfileValidationProblem::DuplicateDescriptor {
                        kind: diagnostic_descriptor_kind(kind),
                        id,
                    }
                }
                CompilationProfileValidationError::DuplicateObservation { kind, id } => {
                    DiagnosticProfileValidationProblem::DuplicateObservation {
                        kind: diagnostic_descriptor_kind(kind),
                        id,
                    }
                }
                CompilationProfileValidationError::UnknownDescriptor { kind, id } => {
                    DiagnosticProfileValidationProblem::UnknownDescriptor {
                        kind: diagnostic_descriptor_kind(kind),
                        id,
                    }
                }
                CompilationProfileValidationError::InvalidRuntimeArtifactIdentity { index } => {
                    DiagnosticProfileValidationProblem::InvalidRuntimeArtifactIdentity { index }
                }
                CompilationProfileValidationError::NonCanonicalRuntimeArtifacts {
                    first,
                    second,
                } => DiagnosticProfileValidationProblem::NonCanonicalRuntimeArtifacts {
                    first,
                    second,
                },
                CompilationProfileValidationError::InvalidSchedulerStatistics => {
                    DiagnosticProfileValidationProblem::InvalidSchedulerStatistics
                }
                CompilationProfileValidationError::InvalidQueryStatistics { id } => {
                    DiagnosticProfileValidationProblem::InvalidQueryStatistics { id }
                }
            },
        })
    })?;

    Ok(report)
}

fn diagnostic_profile_context(
    context: bray_profile::CompilationProfileContext,
) -> DiagnosticProfileContext {
    DiagnosticProfileContext {
        package: context.package,
        product: context.product,
        target: context.target,
    }
}

const fn diagnostic_descriptor_kind(
    kind: bray_profile::CompilationProfileDescriptorKind,
) -> DiagnosticProfileDescriptorKind {
    match kind {
        bray_profile::CompilationProfileDescriptorKind::Operation => {
            DiagnosticProfileDescriptorKind::Operation
        }
        bray_profile::CompilationProfileDescriptorKind::Query => {
            DiagnosticProfileDescriptorKind::Query
        }
        bray_profile::CompilationProfileDescriptorKind::Metric => {
            DiagnosticProfileDescriptorKind::Metric
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use bray_profile::{
        COMPILATION_PROFILE_SCHEMA_REVISION, CompilationProfileAggregation,
        CompilationProfileCategory, CompilationProfileContext, CompilationProfileDescriptorCatalog,
        CompilationProfileDurationDistribution, CompilationProfileMetric,
        CompilationProfileMetricDescriptor, CompilationProfileMode,
        CompilationProfileOperationDescriptor, CompilationProfileOperationStatistics,
        CompilationProfileQueryDescriptor, CompilationProfileQueryStatistics,
        CompilationProfileReport, CompilationProfileSchedulerStatistics,
        CompilationProfileTimeBreakdown, CompilationProfileUnit,
    };

    use super::run_profile_command;

    fn report(elapsed_nanoseconds: u64) -> CompilationProfileReport {
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
            scheduler: CompilationProfileSchedulerStatistics {
                worker_budget: 1,
                active_worker_nanoseconds: elapsed_nanoseconds,
                maximum_active_workers: 1,
                ready_waves: 0,
                ready_items: 0,
                maximum_ready_width: 0,
                query_critical_path_nanoseconds: elapsed_nanoseconds,
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
                maximum_active_workers: 1,
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
                evaluation_self_nanoseconds: elapsed_nanoseconds,
                evaluation_latency: CompilationProfileDurationDistribution {
                    samples: 1,
                    minimum_nanoseconds: elapsed_nanoseconds,
                    median_upper_bound_nanoseconds: elapsed_nanoseconds,
                    p95_upper_bound_nanoseconds: elapsed_nanoseconds,
                    maximum_nanoseconds: elapsed_nanoseconds,
                },
                wait_nanoseconds: 0,
                ready_value_nanoseconds: 0,
                ready_value_maximum_nanoseconds: 0,
                published_values: 0,
                published_inline_bytes: 0,
                result_diagnostics: 0,
                cloned_values: 0,
                cloned_inline_bytes: 0,
                cloned_diagnostics: 0,
            }],
            metrics: vec![CompilationProfileMetric {
                id: 2_000,
                value: 1,
            }],
            runtime_artifacts: Vec::new(),
            events: Vec::new(),
            dropped_events: 0,
        }
    }

    fn report_file(report: &CompilationProfileReport) -> tempfile::NamedTempFile {
        let mut file = tempfile::NamedTempFile::new()
            .unwrap_or_else(|error| panic!("profile fixture must be created: {error:?}"));

        let bytes = serde_json::to_vec(report)
            .unwrap_or_else(|error| panic!("profile fixture must serialize: {error:?}"));

        file.write_all(&bytes)
            .unwrap_or_else(|error| panic!("profile fixture must be written: {error:?}"));

        file
    }

    #[test]
    fn stored_profiles_render_and_compare_without_loading_a_project() {
        let before = report_file(&report(1_000_000));
        let after = report_file(&report(1_500_000));

        let shown = run_profile_command(
            before.path().to_path_buf(),
            None,
            bray_tooling::OutputFormat::Text,
        );

        assert!(shown.diagnostics().is_empty());

        assert!(
            shown
                .stdout()
                .contains("Top queries by evaluation self time")
        );

        assert!(shown.stdout().contains("Compilation units and artifacts"));

        let compared = run_profile_command(
            before.path().to_path_buf(),
            Some(after.path().to_path_buf()),
            bray_tooling::OutputFormat::Text,
        );

        assert!(compared.diagnostics().is_empty());
        assert!(compared.stdout().contains("+500.000 us (+50.0%)"));

        assert!(
            compared
                .stdout()
                .contains("Largest query evaluation self-time changes")
        );
    }

    #[test]
    fn stored_profiles_reject_unsupported_schema_revisions() {
        let mut unsupported = report(1_000_000);
        unsupported.schema_revision = COMPILATION_PROFILE_SCHEMA_REVISION.saturating_add(1);

        let file = report_file(&unsupported);

        let result = run_profile_command(
            file.path().to_path_buf(),
            None,
            bray_tooling::OutputFormat::Text,
        );

        assert!(!result.diagnostics().is_empty());
    }
}
