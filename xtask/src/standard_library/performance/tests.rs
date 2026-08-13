use std::collections::BTreeMap;

use bray_compilation::{
    CompilationProfileAggregation, CompilationProfileCategory, CompilationProfileContext,
    CompilationProfileDescriptorCatalog, CompilationProfileMetric,
    CompilationProfileMetricDescriptor, CompilationProfileMode, CompilationProfileOperationDescriptor,
    CompilationProfileOperationStatistics, CompilationProfileReport,
    CompilationProfileTimeBreakdown, CompilationProfileUnit,
};
use sha2::{Digest as _, Sha256};

use super::comparison::compare;
use super::command::parse_options_for_test;
use super::model::{
    ArtifactDependencies, ArtifactKind, ArtifactReport, Observation, PeerLanguage, PeerOutcome,
    PeerReport, PerformanceReport, ReportIdentity, SCHEMA_REVISION, WorkloadCategory,
    WorkloadObservations, WorkloadReport,
};
use super::retention::{
    bounded_retained_inputs_for_test, contains_retained_provenance, retained_inputs_for_test,
    sections_for_test,
};
use super::statistics::summarize;

#[test]
fn execution_statistics_use_median_and_median_absolute_deviation() {
    let statistics = summarize(
        vec![100, 101, 102, 500, 99],
        1_000,
        super::model::BRAY_EXECUTION_SCOPE,
    )
        .unwrap_or_else(|| panic!("nonempty samples must produce statistics"));

    assert_eq!(statistics.samples_nanoseconds, [99, 100, 101, 102, 500]);
    assert_eq!(statistics.median_nanoseconds, 101);
    assert_eq!(statistics.median_absolute_deviation_nanoseconds, 1);
    assert_eq!(statistics.minimum_nanoseconds, 99);
    assert_eq!(statistics.maximum_nanoseconds, 500);
}

#[test]
fn command_options_enforce_positive_bounded_samples_and_known_workloads() {
    assert!(parse_options_for_test(&["--output", "report", "--samples", "0"]).is_err());

    assert!(
        parse_options_for_test(&["--output", "report", "--samples", "10001"]).is_err()
    );

    assert!(
        parse_options_for_test(&["--output", "report", "--workload", "unknown"]).is_err()
    );

    assert!(
        parse_options_for_test(&[
            "--output",
            "report",
            "--warmup",
            "1",
            "--samples",
            "1",
            "--workload",
            "small_output",
        ])
        .is_ok()
    );
}

#[test]
fn output_validation_rejects_peer_output_mismatches() {
    let expected = bray_base::lowercase_hex(&Sha256::digest([]));

    assert!(super::validate_output(&[], &expected).is_ok());
    assert!(super::validate_output(b"unexpected", &expected).is_err());
}

#[test]
fn linker_map_inputs_preserve_archive_member_provenance() {
    let inputs = retained_inputs_for_test(
        "runtime.lib(memory.o) C:/toolchain/std.lib(buffer.o) application.obj kernel32.dll",
    );

    assert!(inputs.iter().any(|input| {
        input.artifact == "runtime.lib" && input.member.as_deref() == Some("memory.o")
    }));

    assert!(inputs.iter().any(|input| {
        input.artifact == "C:/toolchain/std.lib" && input.member.as_deref() == Some("buffer.o")
    }));

    assert!(inputs.iter().any(|input| input.artifact == "application.obj"));

    let microsoft = retained_inputs_for_test("bray_runtime_common:0120.o C:/work/application.obj");

    assert!(microsoft.iter().any(|input| {
        input.artifact == "bray_runtime_common" && input.member.as_deref() == Some("0120.o")
    }));

    assert!(microsoft.iter().any(|input| input.artifact == "C:/work/application.obj"));
}

#[test]
fn retention_provenance_ignores_unselected_archive_load_records() {
    let map = "LOAD libbray_platform_process.a\n\
        libbray_platform_filesystem.a(hash-filesystem.o)\n\
        0000 _run_output_context";

    assert!(!contains_retained_provenance(map, "bray_platform_process"));
    assert!(contains_retained_provenance(map, "bray_platform_filesystem"));
    assert!(contains_retained_provenance(map, "run_output_context"));
}

#[test]
fn retained_input_reports_are_bounded_and_disclose_omissions() {
    let map = (0..4_100)
        .map(|index| format!("archive.lib({index}.o)"))
        .collect::<Vec<_>>()
        .join(" ");

    let inputs = bounded_retained_inputs_for_test(&map);

    assert_eq!(inputs.entries.len(), 4_096);
    assert_eq!(inputs.omitted_count, 4);
}

#[test]
fn object_sections_use_raw_size_only_within_section_records() {
    let sections = sections_for_test(
        "Name: ignored\nSection {\n  Name: .text (1)\n  Size: 0x80\n  RawDataSize: 0x40\n}\n",
    );

    assert_eq!(sections.len(), 1);
    assert_eq!(sections[0].name, ".text");
    assert_eq!(sections[0].bytes, 64);
}

#[test]
fn reports_round_trip_with_explicit_unavailable_observations() {
    let report = report("corpus", 101, 2);

    let bytes = serde_json::to_vec(&report)
        .unwrap_or_else(|error| panic!("performance report must serialize: {error}"));

    let decoded: PerformanceReport = serde_json::from_slice(&bytes)
        .unwrap_or_else(|error| panic!("performance report must deserialize: {error}"));

    assert_eq!(decoded, report);

    assert!(matches!(
        decoded.workloads[0].observations.allocation_count,
        Observation::Unavailable { .. }
    ));
}

#[test]
fn comparison_rejects_non_equivalent_corpora_and_suppresses_noisy_claims() {
    let baseline = report("corpus", 100, 4);
    let candidate = report("corpus", 102, 4);

    let comparison = compare(&baseline, &candidate)
        .unwrap_or_else(|error| panic!("equivalent reports must compare: {error}"));

    assert_eq!(
        comparison.workloads[0].bray_execution.assessment,
        super::model::ChangeAssessment::Indeterminate
    );

    let different = report("other", 102, 4);

    assert!(compare(&baseline, &different).is_err());

    let mut different_toolchain = report("corpus", 102, 4);
    different_toolchain.identity.llvm_version = "other llvm".to_owned();

    assert!(compare(&baseline, &different_toolchain).is_err());
}

#[test]
fn comparison_rejects_reports_with_inconsistent_statistics_or_corpus_contracts() {
    let baseline = report("corpus", 100, 4);
    let mut invalid_statistics = report("corpus", 102, 4);

    invalid_statistics.workloads[0]
        .bray_execution
        .samples_nanoseconds
        .pop();

    assert!(compare(&baseline, &invalid_statistics).is_err());

    let mut invalid_output = report("corpus", 102, 4);
    invalid_output.workloads[0].expected_output_sha256 = "0".repeat(64);

    assert!(compare(&baseline, &invalid_output).is_err());

    let mut different_peer_configuration = report("corpus", 102, 4);

    if let PeerOutcome::Measured { report } = different_peer_configuration.workloads[0]
        .peers
        .get_mut(&PeerLanguage::Rust)
        .unwrap_or_else(|| panic!("Rust fixture peer must exist"))
    {
        report.build_configuration = "different flags".to_owned();
    }

    assert!(compare(&baseline, &different_peer_configuration).is_err());
}

#[test]
fn comparison_attributes_compiler_artifact_retention_and_observation_changes() {
    let mut baseline = report("corpus", 100, 0);
    let mut candidate = report("corpus", 120, 0);

    baseline.workloads[0].observations.allocation_count = Observation::Measured {
        value: 10,
        scope: "process".to_owned(),
    };

    let workload = &mut candidate.workloads[0];

    workload.compilation.metrics[0].value = 5;
    workload.artifacts[0].bytes = 120;
    workload.artifacts[0].sections.entries[0].bytes = 30;

    workload.artifacts[0]
        .dependencies
        .static_inputs
        .entries
        .push(retained("new.lib", "member.o"));

    workload.artifacts[0]
        .dependencies
        .static_inputs
        .entries
        .sort();

    workload.observations.allocation_count = Observation::Measured {
        value: 12,
        scope: "process".to_owned(),
    };

    let comparison = compare(&baseline, &candidate)
        .unwrap_or_else(|error| panic!("equivalent reports must compare: {error}"));

    let workload = &comparison.workloads[0];
    let artifact = &workload.artifacts[0];

    assert_eq!(workload.compiler_operations["compile"].delta, 20);

    assert_eq!(
        workload.compiler_operations["compile"].assessment,
        super::model::ChangeAssessment::Indeterminate
    );

    assert_eq!(workload.compiler_metrics["source_units"].delta, 2);

    assert_eq!(
        workload.compiler_metrics["source_units"].assessment,
        super::model::ChangeAssessment::Indeterminate
    );

    assert_eq!(artifact.bytes.delta, 20);
    assert_eq!(artifact.sections[".text"].delta, 10);
    assert_eq!(artifact.added_static_inputs, [retained("new.lib", "member.o")]);

    assert!(matches!(
        workload.observations.allocation_count,
        super::model::ObservationComparison::Measured {
            comparison,
            ref scope,
        } if comparison.delta == 2 && scope == "process"
    ));
}

fn retained(artifact: &str, member: &str) -> super::model::RetainedInput {
    super::model::RetainedInput {
        artifact: artifact.to_owned(),
        member: Some(member.to_owned()),
    }
}

pub(super) fn report(corpus: &str, median: u64, mad: u64) -> PerformanceReport {
    let process_execution = summarize(
        vec![median.saturating_sub(mad), median, median.saturating_add(mad)],
        1,
        super::model::PROCESS_EXECUTION_SCOPE,
    )
    .unwrap_or_else(|| panic!("fixture samples must produce statistics"));

    let bray_execution = summarize(
        vec![median.saturating_sub(mad), median, median.saturating_add(mad)],
        1,
        super::model::BRAY_EXECUTION_SCOPE,
    )
    .unwrap_or_else(|| panic!("fixture Bray samples must produce statistics"));

    let peer = || PeerOutcome::Measured {
        report: PeerReport {
            toolchain: "peer compiler".to_owned(),
            build_configuration: "release".to_owned(),
            source_sha256: bray_base::lowercase_hex(&Sha256::digest("peer source")),
            compile_link_nanoseconds: median,
            process_execution: process_execution.clone(),
            controlled_execution: bray_execution.clone(),
            artifacts: vec![ArtifactReport {
                kind: ArtifactKind::Executable,
                path: "peer".to_owned(),
                bytes: 80,
                sections: bounded(vec![super::model::SectionSize {
                    name: ".text".to_owned(),
                    bytes: 15,
                }]),
                dependencies: ArtifactDependencies {
                    static_inputs: bounded(vec![retained("peer-runtime.lib", "startup.o")]),
                    dynamic_libraries: bounded(vec!["system.dll".to_owned()]),
                },
                linker_map: None,
            }],
            observations: WorkloadObservations {
                allocation_count: unavailable(),
                allocated_bytes: unavailable(),
                copied_bytes: unavailable(),
                platform_operations: BTreeMap::new(),
            },
        },
    };

    let peers = [(PeerLanguage::Rust, peer()), (PeerLanguage::Cpp, peer())]
        .into_iter()
        .collect();

    PerformanceReport {
        schema_revision: SCHEMA_REVISION,
        identity: ReportIdentity {
            corpus_revision: 1,
            corpus_sha256: bray_base::lowercase_hex(&Sha256::digest(corpus.as_bytes())),
            target: "test-target".to_owned(),
            host: "test-host".to_owned(),
            build_configuration: "release".to_owned(),
            compiler_version: "compiler".to_owned(),
            source_revision: "revision".to_owned(),
            llvm_version: "llvm".to_owned(),
            warmup_iterations: 2,
            sample_iterations: 3,
        },
        workloads: vec![WorkloadReport {
            id: "small_output".to_owned(),
            peer_contract: Some("start and complete an empty program once".to_owned()),
            category: WorkloadCategory::Small,
            scale: 1,
            units: "executions".to_owned(),
            expected_output_sha256: bray_base::lowercase_hex(&Sha256::digest([])),
            compilation: profile(median),
            process_execution,
            bray_execution,
            artifacts: vec![ArtifactReport {
                kind: ArtifactKind::Executable,
                path: "application".to_owned(),
                bytes: 100,
                sections: bounded(vec![super::model::SectionSize {
                    name: ".text".to_owned(),
                    bytes: 20,
                }]),
                dependencies: ArtifactDependencies {
                    static_inputs: bounded(vec![retained("runtime.lib", "startup.o")]),
                    dynamic_libraries: bounded(vec!["system.dll".to_owned()]),
                },
                linker_map: None,
            }],
            observations: WorkloadObservations {
                allocation_count: unavailable(),
                allocated_bytes: unavailable(),
                copied_bytes: unavailable(),
                platform_operations: BTreeMap::new(),
            },
            peers,
        }],
    }
}

fn bounded<T>(entries: Vec<T>) -> super::model::BoundedList<T> {
    super::model::BoundedList {
        entries,
        omitted_count: 0,
    }
}

fn unavailable() -> Observation {
    Observation::Unavailable {
        reason: "not observed".to_owned(),
    }
}

fn profile(elapsed_nanoseconds: u64) -> CompilationProfileReport {
    CompilationProfileReport {
        schema_revision: bray_profile::COMPILATION_PROFILE_SCHEMA_REVISION,
        mode: CompilationProfileMode::Summary,
        context: CompilationProfileContext {
            package: "test".to_owned(),
            product: "application".to_owned(),
            target: "test-target".to_owned(),
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
                name: "compile".to_owned(),
                category: CompilationProfileCategory::Work,
                unit: CompilationProfileUnit::Nanoseconds,
                aggregation: CompilationProfileAggregation::SumAndMaximum,
                allowed_subjects: Vec::new(),
            }],
            queries: Vec::new(),
            metrics: vec![CompilationProfileMetricDescriptor {
                id: 2,
                name: "source_units".to_owned(),
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
        queries: Vec::new(),
        metrics: vec![CompilationProfileMetric { id: 2, value: 3 }],
        runtime_artifacts: Vec::new(),
        events: Vec::new(),
        dropped_events: 0,
    }
}
