use std::collections::BTreeMap;

use bray_compilation::{
    CompilationProfileAggregation, CompilationProfileCategory, CompilationProfileContext,
    CompilationProfileDescriptorCatalog, CompilationProfileMetric,
    CompilationProfileMetricDescriptor, CompilationProfileMode,
    CompilationProfileOperationDescriptor, CompilationProfileOperationStatistics,
    CompilationProfileReport, CompilationProfileTimeBreakdown, CompilationProfileUnit,
};
use sha2::{Digest as _, Sha256};

use super::command::{parse_options_for_test, validate_output_parts};
use super::comparison::compare;
use super::corpus::{
    CALIBRATION_SAMPLE_COUNT, CALIBRATION_SEED_INNER_ITERATIONS, CALIBRATION_TARGET_NANOSECONDS,
    ExpectedSideEffects,
};
use super::model::{
    ArtifactDependencies, ArtifactKind, ArtifactReport, CompilationBuildReport,
    CompilationComparability, CompilationIncomparability, CompilationKind, CompilationLanguage,
    LibraryReuse, LinkerInvocationReport, Observation, PeerBatching, PeerLanguage, PeerReport,
    PerformanceReport, ReportIdentity, RuntimeLinkage, SCHEMA_REVISION, ToolInvocationReport,
    WorkloadBatching, WorkloadCategory, WorkloadObservations, WorkloadReport,
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
        2,
        10,
    )
    .unwrap_or_else(|| panic!("nonempty samples must produce statistics"));

    assert_eq!(statistics.raw_samples_nanoseconds, [99, 100, 101, 102, 500]);

    assert_eq!(
        statistics.samples_picoseconds,
        [49_500, 50_000, 50_500, 51_000, 250_000]
    );

    assert_eq!(statistics.median_picoseconds, 50_500);
    assert_eq!(statistics.median_absolute_deviation_picoseconds, 500);
    assert_eq!(statistics.minimum_picoseconds, 49_500);
    assert_eq!(statistics.maximum_picoseconds, 250_000);
}

#[test]
fn batched_statistics_retain_sub_nanosecond_adjusted_durations() {
    let statistics = summarize(
        vec![300_000, 310_000, 320_000],
        1,
        super::model::BRAY_EXECUTION_SCOPE,
        1_000_000,
        100,
    )
    .unwrap_or_else(|| panic!("batched samples must produce statistics"));

    assert_eq!(
        statistics.raw_samples_nanoseconds,
        [300_000, 310_000, 320_000]
    );

    assert_eq!(statistics.samples_picoseconds, [300, 310, 320]);
    assert_eq!(statistics.median_picoseconds, 310);
    assert_eq!(statistics.median_units_per_second, 3_225_806_451);
}

#[test]
fn command_options_enforce_positive_bounded_samples_and_known_workloads() {
    assert!(parse_options_for_test(&["--output", "report", "--samples", "0"]).is_err());

    assert!(parse_options_for_test(&["--output", "report", "--samples", "10001"]).is_err());

    assert!(parse_options_for_test(&["--output", "report", "--workload", "unknown"]).is_err());

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

    let working_directory = tempfile::tempdir()
        .unwrap_or_else(|error| panic!("output validation directory must exist: {error}"));

    assert!(
        validate_output_parts(
            &[],
            &[],
            &expected,
            ExpectedSideEffects::None,
            working_directory.path(),
        )
        .is_ok()
    );

    assert!(
        validate_output_parts(
            b"unexpected",
            &[],
            &expected,
            ExpectedSideEffects::None,
            working_directory.path(),
        )
        .is_err()
    );

    assert!(
        validate_output_parts(
            &[],
            b"unexpected",
            &expected,
            ExpectedSideEffects::None,
            working_directory.path(),
        )
        .is_err()
    );

    let effect = working_directory.path().join("effect");

    std::fs::write(&effect, [])
        .unwrap_or_else(|error| panic!("output effect fixture must write: {error}"));

    assert!(
        validate_output_parts(
            &[],
            &[],
            &expected,
            ExpectedSideEffects::AbsentPath("effect"),
            working_directory.path(),
        )
        .is_err()
    );

    std::fs::remove_file(effect)
        .unwrap_or_else(|error| panic!("output effect fixture must remove: {error}"));

    assert!(
        validate_output_parts(
            &[],
            &[],
            &expected,
            ExpectedSideEffects::AbsentPath("effect"),
            working_directory.path(),
        )
        .is_ok()
    );
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

    assert!(
        inputs
            .iter()
            .any(|input| input.artifact == "application.obj")
    );

    let microsoft = retained_inputs_for_test("bray_runtime_common:0120.o C:/work/application.obj");

    assert!(microsoft.iter().any(|input| {
        input.artifact == "bray_runtime_common" && input.member.as_deref() == Some("0120.o")
    }));

    assert!(
        microsoft
            .iter()
            .any(|input| input.artifact == "C:/work/application.obj")
    );
}

#[test]
fn retention_provenance_ignores_unselected_archive_load_records() {
    let map = "LOAD libbray_platform_process.a\n\
        libbray_platform_filesystem.a(hash-filesystem.o)\n\
        0000 _run_output_context";

    assert!(!contains_retained_provenance(map, "bray_platform_process"));

    assert!(contains_retained_provenance(
        map,
        "bray_platform_filesystem"
    ));

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
fn compilation_reuse_excludes_application_owned_objects() {
    let mut report = report("corpus", 101, 2);

    let build = report
        .application_compilation
        .builds
        .get_mut(&CompilationLanguage::Rust)
        .unwrap_or_else(|| panic!("application comparison must contain Rust"));

    build.reused_artifacts = super::compilation::reused_artifacts(
        &[ArtifactReport {
            kind: ArtifactKind::Executable,
            path: "application".to_owned(),
            bytes: 1,
            sections: bounded(Vec::new()),
            dependencies: ArtifactDependencies {
                static_inputs: bounded(vec![
                    super::model::RetainedInput {
                        artifact: "application.o".to_owned(),
                        member: None,
                    },
                    retained("std.lib", "runtime.o"),
                ]),
                dynamic_libraries: bounded(Vec::new()),
            },
            linker_map: None,
        }],
        [],
    );

    assert_eq!(build.reused_artifacts.entries, [retained("std.lib", "runtime.o")]);
}

#[test]
fn compilation_comparability_rejects_stale_claims_and_suppresses_winners() {
    let mut report = report("corpus", 101, 2);

    let rust = report
        .application_compilation
        .builds
        .get_mut(&CompilationLanguage::Rust)
        .unwrap_or_else(|| panic!("application comparison must contain Rust"));

    rust.authority.library_reuse = LibraryReuse::Source;

    assert!(super::validation::validate(&report).is_err());

    let builds = std::mem::take(&mut report.application_compilation.builds);

    report.application_compilation = super::compilation::comparison(
        CompilationKind::Application,
        super::compilation::MATCHED_APPLICATION_CONTRACT,
        builds,
    );

    super::validation::validate(&report)
        .unwrap_or_else(|error| panic!("incomparable authority must remain reportable: {error}"));

    let mut html = super::html::BoundedHtml::new();

    super::presentation::compilation_comparison(
        &mut html,
        "Application compilation",
        &report.application_compilation,
    );

    let html = super::html::finish(html)
        .unwrap_or_else(|error| panic!("incomparable comparison must render: {error}"));

    assert!(html.contains("application build compiled library source"));
    assert!(!html.contains("class=\"metric-best\""));
}

#[test]
fn compilation_comparability_explains_different_source_authority() {
    let mut report = report("corpus", 101, 2);

    let rust = report
        .library_compilation
        .builds
        .get_mut(&CompilationLanguage::Rust)
        .unwrap_or_else(|| panic!("library comparison must contain Rust"));

    rust.authority.source_units = 2;
    rust.authority.packages.push("support".to_owned());
    rust.authority.modules.push("support".to_owned());

    let builds = std::mem::take(&mut report.library_compilation.builds);

    report.library_compilation = super::compilation::comparison(
        CompilationKind::Library,
        super::compilation::MATCHED_LIBRARY_CONTRACT,
        builds,
    );

    super::validation::validate(&report)
        .unwrap_or_else(|error| panic!("different authority must remain reportable: {error}"));

    let CompilationComparability::Incomparable { reasons } =
        &report.library_compilation.comparability
    else {
        panic!("different authority must be incomparable");
    };

    assert_eq!(
        reasons[&CompilationLanguage::Rust],
        [
            CompilationIncomparability::DifferentSourceUnitCount,
            CompilationIncomparability::DifferentPackageInputCount,
            CompilationIncomparability::DifferentModuleInputCount,
        ]
    );
}

#[test]
fn matched_compilation_lanes_record_external_source_authority() {
    let report = report("corpus", 101, 2);

    for build in report.application_compilation.builds.values() {
        assert_eq!(build.authority.library_reuse, LibraryReuse::Packaged);
        assert!(!build.reused_artifacts.entries.is_empty());
        assert!(!build.compiler.program.is_empty());
    }

    for build in report.library_compilation.builds.values() {
        assert_eq!(build.authority.library_reuse, LibraryReuse::Source);
        assert!(build.reused_artifacts.entries.is_empty());
        assert!(!build.compiler.program.is_empty());
    }
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

    let mut recalibrated = report("corpus", 102, 4);
    let recalibrated_count = 50_000_000;

    recalibrated.workloads[0].batching = WorkloadBatching::Calibrated {
        seed_inner_iterations: CALIBRATION_SEED_INNER_ITERATIONS,
        target_interval_nanoseconds: CALIBRATION_TARGET_NANOSECONDS,
        bray_samples_nanoseconds: vec![2_000_000; 3],
        rust_samples_nanoseconds: vec![2_000_000; 3],
        cpp_samples_nanoseconds: vec![2_000_000; 3],
        selected_inner_iterations: recalibrated_count,
    };

    let recalibrated_execution = summarize(
        vec![98, 102, 106]
            .into_iter()
            .map(|sample| sample * recalibrated_count)
            .collect(),
        1,
        super::model::BRAY_EXECUTION_SCOPE,
        recalibrated_count,
        100,
    )
    .unwrap_or_else(|| panic!("recalibrated fixture must produce statistics"));

    recalibrated.workloads[0].bray_execution = recalibrated_execution.clone();

    for (language, peer) in &mut recalibrated.workloads[0].peers {
        peer.controlled_execution = recalibrated_execution.clone();

        let configuration = super::peer::fixture_build_configuration(
            *language,
            "small_output",
            bray_target::NativeTarget::X86_64WindowsMsvc,
            std::path::Path::new(&peer.artifacts[0].path),
            std::num::NonZeroU64::new(recalibrated_count)
                .unwrap_or_else(|| panic!("recalibrated fixture count must be nonzero")),
        );

        peer.build_configuration = configuration;
    }

    compare(&baseline, &recalibrated)
        .unwrap_or_else(|error| panic!("different valid batch counts must compare: {error}"));

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
        .raw_samples_nanoseconds
        .pop();

    assert!(compare(&baseline, &invalid_statistics).is_err());

    let mut invalid_output = report("corpus", 102, 4);
    invalid_output.workloads[0].expected_output_sha256 = "0".repeat(64);

    assert!(compare(&baseline, &invalid_output).is_err());

    let mut different_peer_configuration = report("corpus", 102, 4);

    different_peer_configuration.workloads[0]
        .peers
        .get_mut(&PeerLanguage::Rust)
        .unwrap_or_else(|| panic!("Rust fixture peer must exist"))
        .build_configuration
        .production
        .arguments
        .push("different flag".to_owned());

    assert!(compare(&baseline, &different_peer_configuration).is_err());

    let mut batched_production = report("corpus", 102, 4);

    batched_production.workloads[0]
        .peers
        .get_mut(&PeerLanguage::Rust)
        .unwrap_or_else(|| panic!("Rust fixture peer must exist"))
        .build_configuration
        .timed
        .batching = PeerBatching::SingleExecution;

    assert!(compare(&baseline, &batched_production).is_err());

    let mut mismatched_timed_batch = report("corpus", 102, 4);

    mismatched_timed_batch.workloads[0]
        .peers
        .get_mut(&PeerLanguage::Cpp)
        .unwrap_or_else(|| panic!("C++ fixture peer must exist"))
        .build_configuration
        .timed
        .arguments
        .retain(|argument| argument != "-DBRAY_INNER_ITERATIONS=100000000ULL");

    assert!(compare(&baseline, &mismatched_timed_batch).is_err());

    let mut inconsistent_calibration = report("corpus", 102, 4);

    let WorkloadBatching::Calibrated {
        selected_inner_iterations,
        ..
    } = &mut inconsistent_calibration.workloads[0].batching
    else {
        panic!("fixture workload must use calibrated batching")
    };

    *selected_inner_iterations = 50_000_000;

    assert!(compare(&baseline, &inconsistent_calibration).is_err());

    let mut dynamic_dependency = report("corpus", 102, 4);

    let dynamic_libraries = &mut dynamic_dependency.workloads[0]
        .peers
        .get_mut(&PeerLanguage::Rust)
        .unwrap_or_else(|| panic!("Rust fixture peer must exist"))
        .artifacts[0]
        .dependencies
        .dynamic_libraries
        .entries;

    dynamic_libraries.clear();
    dynamic_libraries.push("VCRUNTIME140.dll".to_owned());

    assert!(compare(&baseline, &dynamic_dependency).is_err());
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

    workload.compiler_profile.metrics[0].value = 5;
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

    assert_eq!(
        artifact.added_static_inputs,
        [retained("new.lib", "member.o")]
    );

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
    let calibration_sample_count = usize::try_from(CALIBRATION_SAMPLE_COUNT)
        .unwrap_or_else(|_| panic!("fixture calibration count must fit usize"));

    let process_execution = summarize(
        vec![
            median.saturating_sub(mad),
            median,
            median.saturating_add(mad),
        ],
        1,
        super::model::PROCESS_EXECUTION_SCOPE,
        1,
        100,
    )
    .unwrap_or_else(|| panic!("fixture samples must produce statistics"));

    let inner_iterations = 100_000_000;

    let bray_execution = summarize(
        vec![
            median.saturating_sub(mad),
            median,
            median.saturating_add(mad),
        ]
        .into_iter()
        .map(|sample| sample.saturating_mul(inner_iterations))
        .collect(),
        1,
        super::model::BRAY_EXECUTION_SCOPE,
        inner_iterations,
        100,
    )
    .unwrap_or_else(|| panic!("fixture Bray samples must produce statistics"));

    let peer = |language| {
        let path = std::path::Path::new(match language {
            PeerLanguage::Rust => "rust",
            PeerLanguage::Cpp => "cpp",
        })
        .join(crate::native_toolchain::executable_name("peer"))
        .display()
        .to_string();

        let configuration = super::peer::fixture_build_configuration(
            language,
            "small_output",
            bray_target::NativeTarget::X86_64WindowsMsvc,
            std::path::Path::new(&path),
            std::num::NonZeroU64::new(inner_iterations)
                .unwrap_or_else(|| panic!("fixture repetition count must be nonzero")),
        );

        let build = CompilationBuildReport {
            toolchain: "peer compiler".to_owned(),
            source_sha256: bray_base::lowercase_hex(&Sha256::digest("peer source")),
            elapsed_nanoseconds: median,
            authority: super::compilation::authority(
                1,
                11,
                [format!("{}.performance_peer", peer_language_name(language)), "precompiled_standard_library".to_owned()],
                [match language {
                    PeerLanguage::Rust => "crate".to_owned(),
                    PeerLanguage::Cpp => "translation_unit".to_owned(),
                }],
                LibraryReuse::Packaged,
            ),
            compiler: ToolInvocationReport {
                program: configuration.compiler.clone(),
                arguments: configuration.production.arguments.clone(),
                environment: configuration.production.environment.clone(),
                response_files: Vec::new(),
            },
            linker: LinkerInvocationReport::IntegratedCompilerDriver {
                driver: configuration.linker.clone(),
                arguments: configuration.production.arguments.clone(),
            },
            reused_artifacts: bounded(vec![retained("peer-runtime.lib", "startup.o")]),
            profile: None,
        };

        let report = PeerReport {
            toolchain: "peer compiler".to_owned(),
            build_configuration: configuration,
            source_sha256: bray_base::lowercase_hex(&Sha256::digest("peer source")),
            process_execution: process_execution.clone(),
            controlled_execution: bray_execution.clone(),
            artifacts: vec![ArtifactReport {
                kind: ArtifactKind::Executable,
                path,
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
        };

        (report, build)
    };

    let (rust_peer, rust_build) = peer(PeerLanguage::Rust);

    let (cpp_peer, cpp_build) = peer(PeerLanguage::Cpp);

    let peers = [
        (PeerLanguage::Rust, rust_peer),
        (PeerLanguage::Cpp, cpp_peer),
    ]
    .into_iter()
    .collect();

    let bray_build = CompilationBuildReport {
        toolchain: "Bray compiler".to_owned(),
        source_sha256: bray_base::lowercase_hex(&Sha256::digest("Bray source")),
        elapsed_nanoseconds: median,
        authority: super::compilation::authority(
            1,
            11,
            ["bray.performance.small_output".to_owned(), "std".to_owned()],
            ["small_output".to_owned()],
            LibraryReuse::Packaged,
        ),
        compiler: ToolInvocationReport {
            program: "brayc".to_owned(),
            arguments: vec!["build".to_owned()],
            environment: BTreeMap::new(),
            response_files: Vec::new(),
        },
        linker: LinkerInvocationReport::IntegratedCompilerDriver {
            driver: "brayc".to_owned(),
            arguments: vec!["build".to_owned()],
        },
        reused_artifacts: bounded(vec![retained("runtime.lib", "startup.o")]),
        profile: Some(profile(median)),
    };

    let application_compilation = super::compilation::comparison(
        CompilationKind::Application,
        super::compilation::MATCHED_APPLICATION_CONTRACT,
        [
            (CompilationLanguage::Bray, bray_build),
            (CompilationLanguage::Rust, rust_build),
            (CompilationLanguage::Cpp, cpp_build),
        ]
        .into_iter()
        .collect(),
    );

    let library_build = |language, profile| CompilationBuildReport {
        toolchain: format!("{language:?} compiler"),
        source_sha256: bray_base::lowercase_hex(&Sha256::digest(format!("{language:?} library"))),
        elapsed_nanoseconds: median,
        authority: super::compilation::authority(
            1,
            32,
            ["performance_library".to_owned()],
            ["performance_library".to_owned()],
            LibraryReuse::Source,
        ),
        compiler: ToolInvocationReport {
            program: format!("{language:?}-compiler"),
            arguments: vec!["compile-library".to_owned()],
            environment: BTreeMap::new(),
            response_files: Vec::new(),
        },
        linker: LinkerInvocationReport::NotApplicable,
        reused_artifacts: bounded(Vec::new()),
        profile,
    };

    let library_compilation = super::compilation::comparison(
        CompilationKind::Library,
        super::compilation::MATCHED_LIBRARY_CONTRACT,
        [
            (
                CompilationLanguage::Bray,
                library_build(CompilationLanguage::Bray, Some(profile(median))),
            ),
            (
                CompilationLanguage::Rust,
                library_build(CompilationLanguage::Rust, None),
            ),
            (
                CompilationLanguage::Cpp,
                library_build(CompilationLanguage::Cpp, None),
            ),
        ]
        .into_iter()
        .collect(),
    );

    PerformanceReport {
        schema_revision: SCHEMA_REVISION,
        identity: ReportIdentity {
            corpus_revision: 1,
            corpus_sha256: bray_base::lowercase_hex(&Sha256::digest(corpus.as_bytes())),
            target: "x86_64-pc-windows-msvc".to_owned(),
            host: "test-host".to_owned(),
            build_configuration: "release".to_owned(),
            compiler_version: "compiler".to_owned(),
            source_revision: "revision".to_owned(),
            llvm_version: "llvm".to_owned(),
            runtime_linkage: RuntimeLinkage::StaticApplicationRuntime,
            warmup_iterations: 2,
            sample_iterations: 3,
            timer_resolution_nanoseconds: 100,
        },
        library_compilation,
        application_compilation,
        workloads: vec![WorkloadReport {
            id: "small_output".to_owned(),
            peer_contract: "start and complete an empty program once".to_owned(),
            category: WorkloadCategory::Small,
            scale: 1,
            units: "executions".to_owned(),
            expected_output_sha256: bray_base::lowercase_hex(&Sha256::digest([])),
            batching: WorkloadBatching::Calibrated {
                seed_inner_iterations: CALIBRATION_SEED_INNER_ITERATIONS,
                target_interval_nanoseconds: CALIBRATION_TARGET_NANOSECONDS,
                bray_samples_nanoseconds: vec![1_000_000; calibration_sample_count],
                rust_samples_nanoseconds: vec![1_000_000; calibration_sample_count],
                cpp_samples_nanoseconds: vec![1_000_000; calibration_sample_count],
                selected_inner_iterations: inner_iterations,
            },
            compiler_profile: profile(median),
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

const fn peer_language_name(language: PeerLanguage) -> &'static str {
    match language {
        PeerLanguage::Rust => "rust",
        PeerLanguage::Cpp => "cpp",
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
            target: "x86_64-pc-windows-msvc".to_owned(),
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
