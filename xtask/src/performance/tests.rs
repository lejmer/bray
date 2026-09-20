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
    CompilationComparability, CompilationEvidenceReport, CompilationIncomparability,
    CompilationKind, CompilationLanguage, CompilationReuseEvidence, LibraryReuse,
    LinkerInvocationReport, LinkerMapReport, Observation, OptimizationArtifactReport, PeerBatching,
    PeerLanguage, PeerReport, PerformanceReport, ReportIdentity, RuntimeLinkage, SCHEMA_REVISION,
    ToolInvocationReport, WorkloadBatching, WorkloadCategory, WorkloadCompilationReport,
    WorkloadObservations, WorkloadReport,
};
use super::retention::{
    bounded_retained_inputs_for_test, retained_inputs_for_test, sections_for_test,
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
        "Name: ignored\nSection {\n  Name: .text (1)\n  Size: 0x80\n  RawDataSize: 0x40\n}\n\
         Section {\n  Name: .text (1)\n  Size: 0x20\n  RawDataSize: 0x10\n}\n",
    );

    assert_eq!(sections.len(), 1);
    assert_eq!(sections[0].name, ".text");
    assert_eq!(sections[0].bytes, 80);
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
fn windows_runtime_validation_accepts_crt_imports_and_rejects_static_crt_archives() {
    let valid = report("corpus", 101, 2);

    super::validation::validate(&valid)
        .unwrap_or_else(|error| panic!("dynamic Windows CRT imports must validate: {error}"));

    let mut with_object = report("corpus", 101, 2);
    let mut object = with_object.workloads[0].artifacts[0].clone();

    object.kind = ArtifactKind::RelocatableObject;
    object.path = "application.obj".to_owned();
    object.dependencies.dynamic_libraries.entries.clear();
    object.linker_map = None;
    with_object.workloads[0].artifacts.push(object);

    super::validation::validate(&with_object).unwrap_or_else(|error| {
        panic!("relocatable objects cannot be required to contain PE imports: {error}")
    });

    let mut mixed = valid;

    mixed.workloads[0].artifacts[0]
        .dependencies
        .static_archives
        .entries
        .insert(0, "libcmt.lib".to_owned());

    let error = super::validation::validate(&mixed)
        .expect_err("static Windows CRT archives must violate the dynamic runtime contract");

    assert!(error.contains("retains static Windows CRT input libcmt.lib"));

    let mut incomplete = report("corpus", 101, 2);

    incomplete.workloads[0].artifacts[0]
        .dependencies
        .dynamic_libraries
        .omitted_count = 1;

    let error = super::validation::validate(&incomplete)
        .expect_err("incomplete PE imports cannot prove the dynamic runtime contract");

    assert!(error.contains("omits dependencies required to prove dynamic Windows CRT linkage"));

    let mut missing_import = report("corpus", 101, 2);

    missing_import.workloads[0].artifacts[0]
        .dependencies
        .dynamic_libraries
        .entries = vec!["kernel32.dll".to_owned()];

    let error = super::validation::validate(&missing_import)
        .expect_err("a dynamic runtime report must identify its CRT import");

    assert!(error.contains("does not import the dynamic Windows CRT"));
}

#[test]
fn comparison_attributes_dynamic_windows_runtime_dependency_changes() {
    let baseline = report("corpus", 102, 4);
    let mut candidate = report("corpus", 102, 4);

    let dynamic_libraries = &mut candidate.workloads[0]
        .peers
        .get_mut(&PeerLanguage::Rust)
        .unwrap_or_else(|| panic!("Rust fixture peer must exist"))
        .artifacts[0]
        .dependencies
        .dynamic_libraries
        .entries;

    dynamic_libraries.clear();
    dynamic_libraries.push("VCRUNTIME140.dll".to_owned());

    let comparison = compare(&baseline, &candidate)
        .unwrap_or_else(|error| panic!("valid dynamic CRT dependencies must compare: {error}"));

    let artifact = &comparison.workloads[0].peers[&PeerLanguage::Rust].artifacts[0];

    assert_eq!(artifact.added_dynamic_libraries, ["VCRUNTIME140.dll"]);
    assert_eq!(artifact.removed_dynamic_libraries, ["ucrtbase.dll"]);
}

#[test]
fn compilation_reuse_excludes_application_owned_objects() {
    let mut report = report("corpus", 101, 2);

    let build = report
        .application_compilation
        .builds
        .get_mut(&CompilationLanguage::Rust)
        .unwrap_or_else(|| panic!("application comparison must contain Rust"));

    build.reuse.runtime = super::compilation::reuse_evidence(
        &[ArtifactReport {
            kind: ArtifactKind::Executable,
            path: "application".to_owned(),
            bytes: 1,
            sections: bounded(Vec::new()),
            dependencies: ArtifactDependencies {
                static_archives: bounded(vec!["std.lib".to_owned()]),
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
        |_| Some(super::compilation::CompilationReuseRole::Runtime),
    )
    .runtime;

    assert_eq!(
        build.reuse.runtime.entries,
        [retained("std.lib", "runtime.o")]
    );
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
fn compilation_comparability_rejects_source_sizes_outside_the_syntax_tolerance() {
    let mut report = report("corpus", 101, 2);

    let rust = report
        .library_compilation
        .builds
        .get_mut(&CompilationLanguage::Rust)
        .unwrap_or_else(|| panic!("library comparison must contain Rust"));

    rust.authority.source_bytes = 257;

    let builds = std::mem::take(&mut report.library_compilation.builds);

    report.library_compilation = super::compilation::comparison(
        CompilationKind::Library,
        super::compilation::MATCHED_LIBRARY_CONTRACT,
        builds,
    );

    let CompilationComparability::Incomparable { reasons } =
        &report.library_compilation.comparability
    else {
        panic!("dissimilar source sizes must be incomparable");
    };

    assert!(reasons.values().all(|reasons| {
        reasons.contains(&CompilationIncomparability::DifferentSourceByteScale)
    }));
}

#[test]
fn compilation_validation_rejects_noncanonical_timed_invocations() {
    let mut invalid_report = report("corpus", 101, 2);

    let rust = invalid_report
        .application_compilation
        .builds
        .get_mut(&CompilationLanguage::Rust)
        .unwrap_or_else(|| panic!("application comparison must contain Rust"));

    let optimization = rust
        .compiler
        .arguments
        .iter_mut()
        .find(|argument| argument.as_str() == "opt-level=3")
        .unwrap_or_else(|| panic!("fixture Rust invocation must select release optimization"));

    *optimization = "opt-level=2".to_owned();

    rust.linker = LinkerInvocationReport::IntegratedCompilerDriver {
        driver: rust.compiler.program.clone(),
        arguments: rust.compiler.arguments.clone(),
    };

    assert!(super::validation::validate(&invalid_report).is_err());

    let mut target_report = report("corpus", 101, 2);

    let rust = target_report
        .application_compilation
        .builds
        .get_mut(&CompilationLanguage::Rust)
        .unwrap_or_else(|| panic!("application comparison must contain Rust"));

    let evidence = rust
        .evidence
        .as_mut()
        .unwrap_or_else(|| panic!("application comparison must contain evidence"));

    let target = evidence
        .compiler
        .arguments
        .iter_mut()
        .find(|argument| argument.as_str() == "x86_64-pc-windows-msvc")
        .unwrap_or_else(|| panic!("fixture Rust evidence must select the report target"));

    *target = "aarch64-unknown-linux-gnu".to_owned();

    assert!(super::validation::validate(&target_report).is_err());

    let mut profile_report = report("corpus", 101, 2);

    let bray = profile_report
        .application_compilation
        .builds
        .get_mut(&CompilationLanguage::Bray)
        .unwrap_or_else(|| panic!("application comparison must contain Bray"));

    let evidence = bray
        .evidence
        .as_mut()
        .unwrap_or_else(|| panic!("Bray application comparison must contain evidence"));

    let profile = evidence
        .compiler
        .arguments
        .iter()
        .position(|argument| argument == "--profile")
        .unwrap_or_else(|| panic!("Bray evidence must request a profile"));

    evidence.compiler.arguments.drain(profile..=profile + 3);

    assert!(super::validation::validate(&profile_report).is_err());
}

#[test]
fn matched_compilation_lanes_record_external_source_authority() {
    let report = report("corpus", 101, 2);

    for build in report.application_compilation.builds.values() {
        assert_eq!(build.authority.library_reuse, LibraryReuse::Packaged);
        assert!(!build.reuse.packaged_library.entries.is_empty());
        assert!(!build.reuse.runtime.entries.is_empty());
        assert!(!build.compiler.program.is_empty());

        assert!(
            build
                .evidence
                .as_ref()
                .is_some_and(|evidence| evidence.linker_map.is_some())
        );

        assert!(!build.compiler.arguments.iter().any(|argument| {
            argument == "--profile"
                || argument == "--profile-output"
                || argument == "--linker-map-output"
                || argument.to_ascii_lowercase().contains("/map:")
                || argument.to_ascii_lowercase().contains("-map,")
        }));
    }

    for build in report.library_compilation.builds.values() {
        assert_eq!(build.authority.library_reuse, LibraryReuse::Source);
        assert!(build.reuse.packaged_library.entries.is_empty());
        assert!(build.reuse.runtime.entries.is_empty());
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

    let mut relocated = report("corpus", 102, 4);

    for (language, peer) in &mut relocated.workloads[0].peers {
        peer.artifacts[0].path = format!("relocated/{}", peer.artifacts[0].path);

        peer.build_configuration = super::peer::fixture_build_configuration(
            *language,
            "small_output",
            bray_target::NativeTarget::X86_64WindowsMsvc,
            std::path::Path::new(&peer.artifacts[0].path),
            std::num::NonZeroU64::new(peer.controlled_execution.inner_iterations)
                .unwrap_or_else(|| panic!("fixture batch count must be nonzero")),
        );
    }

    compare(&baseline, &relocated)
        .unwrap_or_else(|error| panic!("different artifact directories must compare: {error}"));

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

    let logical_provenance = &mut workload.artifacts[0]
        .linker_map
        .as_mut()
        .unwrap_or_else(|| panic!("fixture executable must have a linker map"))
        .logical_provenance
        .entries;

    logical_provenance.push("provider".to_owned());
    logical_provenance.sort();

    workload.observations.allocation_count = Observation::Measured {
        value: 12,
        scope: "process".to_owned(),
    };

    let comparison = compare(&baseline, &candidate)
        .unwrap_or_else(|error| panic!("equivalent reports must compare: {error}"));

    let workload = &comparison.workloads[0];
    let artifact = &workload.artifacts[0];

    assert_eq!(workload.compilation_process.delta, 20);
    assert_eq!(workload.compiler_work.delta, 20);
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

    assert_eq!(artifact.added_logical_provenance, ["provider".to_owned()]);

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

        let compilation_language = match language {
            PeerLanguage::Rust => CompilationLanguage::Rust,
            PeerLanguage::Cpp => CompilationLanguage::Cpp,
        };

        let compiler = matched_compiler(compilation_language, CompilationKind::Application, false);
        let evidence = matched_compiler(compilation_language, CompilationKind::Application, true);

        let build = CompilationBuildReport {
            toolchain: "peer compiler".to_owned(),
            source_sha256: bray_base::lowercase_hex(&Sha256::digest("peer source")),
            elapsed_nanoseconds: median,
            authority: super::compilation::authority(
                1,
                11,
                [
                    format!("{}.performance_peer", peer_language_name(language)),
                    "precompiled_standard_library".to_owned(),
                ],
                [match language {
                    PeerLanguage::Rust => "crate".to_owned(),
                    PeerLanguage::Cpp => "translation_unit".to_owned(),
                }],
                LibraryReuse::Packaged,
            ),
            compiler: compiler.clone(),
            linker: LinkerInvocationReport::IntegratedCompilerDriver {
                driver: compiler.program.clone(),
                arguments: compiler.arguments.clone(),
            },
            evidence: Some(CompilationEvidenceReport {
                compiler: evidence,
                linker_map: Some(linker_map()),
            }),
            reuse: CompilationReuseEvidence {
                packaged_library: bounded(vec![retained("peer-library.lib", "library.o")]),
                runtime: bounded(vec![retained("peer-runtime.lib", "startup.o")]),
            },
            profile: None,
        };

        let report = PeerReport {
            toolchain: "peer compiler".to_owned(),
            build_configuration: configuration,
            source_sha256: bray_base::lowercase_hex(&Sha256::digest("peer source")),
            compilation: workload_compilation(
                median,
                match language {
                    PeerLanguage::Rust => "rustc",
                    PeerLanguage::Cpp => "cpp",
                },
            ),
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
                    static_archives: bounded(vec!["peer-runtime.lib".to_owned()]),
                    static_inputs: bounded(vec![retained("peer-runtime.lib", "startup.o")]),
                    dynamic_libraries: bounded(vec!["ucrtbase.dll".to_owned()]),
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

    let bray_compiler = matched_compiler(
        CompilationLanguage::Bray,
        CompilationKind::Application,
        false,
    );

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
        compiler: bray_compiler.clone(),
        linker: LinkerInvocationReport::IntegratedCompilerDriver {
            driver: "brayc".to_owned(),
            arguments: bray_compiler.arguments,
        },
        evidence: Some(CompilationEvidenceReport {
            compiler: matched_compiler(
                CompilationLanguage::Bray,
                CompilationKind::Application,
                true,
            ),
            linker_map: Some(linker_map()),
        }),
        reuse: CompilationReuseEvidence {
            packaged_library: bounded(vec![retained("std.lib", "library.o")]),
            runtime: bounded(vec![retained("runtime.lib", "startup.o")]),
        },
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
        compiler: matched_compiler(language, CompilationKind::Library, false),
        linker: LinkerInvocationReport::NotApplicable,
        evidence: (language == CompilationLanguage::Bray).then(|| CompilationEvidenceReport {
            compiler: matched_compiler(language, CompilationKind::Library, true),
            linker_map: None,
        }),
        reuse: CompilationReuseEvidence {
            packaged_library: bounded(Vec::new()),
            runtime: bounded(Vec::new()),
        },
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
            runtime_linkage: RuntimeLinkage::DynamicApplicationRuntime,
            warmup_iterations: 2,
            sample_iterations: 3,
            timer_resolution_nanoseconds: 100,
        },
        library_compilation,
        application_compilation,
        optimization_artifacts: vec![OptimizationArtifactReport {
            partition: "std".to_owned(),
            path: "targets/x86_64-pc-windows-msvc/1.0/std_optimization.lib".to_owned(),
            bytes: 2048,
            fallback: "targets/x86_64-pc-windows-msvc/1.0/std.lib".to_owned(),
            selected_by_workloads: vec!["small_output".to_owned()],
        }],
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
            compilation: workload_compilation(median, "brayc"),
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
                    static_archives: bounded(vec!["runtime.lib".to_owned()]),
                    static_inputs: bounded(vec![retained("runtime.lib", "startup.o")]),
                    dynamic_libraries: bounded(vec!["ucrtbase.dll".to_owned()]),
                },
                linker_map: Some(linker_map()),
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

fn workload_compilation(elapsed_nanoseconds: u64, compiler: &str) -> WorkloadCompilationReport {
    let invocation = ToolInvocationReport {
        program: compiler.to_owned(),
        arguments: vec!["source".to_owned()],
        environment: BTreeMap::new(),
        response_files: Vec::new(),
    };

    let compiler_components_nanoseconds = if compiler == "cpp" {
        let clang = elapsed_nanoseconds / 2;

        BTreeMap::from([
            ("clang".to_owned(), clang),
            ("lld".to_owned(), elapsed_nanoseconds - clang),
        ])
    } else {
        BTreeMap::from([(compiler.to_owned(), elapsed_nanoseconds)])
    };

    WorkloadCompilationReport {
        process_scope: super::model::COMPILER_PROCESS_SCOPE.to_owned(),
        process_elapsed_nanoseconds: elapsed_nanoseconds,
        compiler_scope: super::model::COMPILER_WORK_SCOPE.to_owned(),
        compiler_elapsed_nanoseconds: elapsed_nanoseconds,
        compiler_components_nanoseconds,
        process_invocation: invocation.clone(),
        profiled_invocation: invocation,
    }
}

fn matched_compiler(
    language: CompilationLanguage,
    kind: CompilationKind,
    evidence: bool,
) -> ToolInvocationReport {
    let target = "x86_64-pc-windows-msvc";

    let (program, arguments) = match language {
        CompilationLanguage::Bray => {
            let mut arguments = Vec::new();

            if evidence {
                arguments.extend([
                    "--profile".to_owned(),
                    "summary".to_owned(),
                    "--profile-output".to_owned(),
                    "profile.json".to_owned(),
                ]);
            }

            if kind == CompilationKind::Application {
                arguments.extend([
                    "--standard-library-root".to_owned(),
                    "standard-library".to_owned(),
                ]);
            }

            arguments.extend([
                "build".to_owned(),
                "--package".to_owned(),
                "bray.performance".to_owned(),
                "--product".to_owned(),
                "performance".to_owned(),
                "--product-kind".to_owned(),
                match kind {
                    CompilationKind::Application => "executable",
                    CompilationKind::Library => "library",
                }
                .to_owned(),
                "--target".to_owned(),
                target.to_owned(),
                "--release".to_owned(),
                "--artifact".to_owned(),
                match kind {
                    CompilationKind::Application => "executable",
                    CompilationKind::Library => "relocatable-object",
                }
                .to_owned(),
            ]);

            if kind == CompilationKind::Application {
                arguments.extend(["--runtime-artifact".to_owned(), "runtime.json".to_owned()]);

                if evidence {
                    arguments.extend([
                        "--linker-map-output".to_owned(),
                        "application.map".to_owned(),
                    ]);
                }
            }

            arguments.extend([
                "--output".to_owned(),
                "out".to_owned(),
                "source.bray".to_owned(),
            ]);

            ("brayc", arguments)
        }
        CompilationLanguage::Rust => {
            let mut arguments = vec![
                "source.rs".to_owned(),
                "--target".to_owned(),
                target.to_owned(),
                "-C".to_owned(),
                "opt-level=3".to_owned(),
                "-C".to_owned(),
                "debuginfo=0".to_owned(),
            ];

            if kind == CompilationKind::Library {
                arguments.extend([
                    "--crate-type".to_owned(),
                    "lib".to_owned(),
                    "--emit".to_owned(),
                    "obj".to_owned(),
                ]);
            } else if evidence {
                arguments.extend(["-C".to_owned(), "link-arg=/MAP:application.map".to_owned()]);
            }

            arguments.extend(["-o".to_owned(), "out".to_owned()]);

            ("rustc", arguments)
        }
        CompilationLanguage::Cpp => {
            let mut arguments = vec![
                "--driver-mode=g++".to_owned(),
                "-std=c++20".to_owned(),
                "-O3".to_owned(),
                "-DNDEBUG".to_owned(),
                format!("--target={target}"),
            ];

            if kind == CompilationKind::Library {
                arguments.push("-c".to_owned());
            } else if evidence {
                arguments.push("-Wl,/MAP:application.map".to_owned());
            }

            arguments.extend(["source.cpp".to_owned(), "-o".to_owned(), "out".to_owned()]);

            ("clang", arguments)
        }
    };

    ToolInvocationReport {
        program: program.to_owned(),
        arguments,
        environment: BTreeMap::new(),
        response_files: Vec::new(),
    }
}

fn linker_map() -> LinkerMapReport {
    LinkerMapReport {
        bytes: 1,
        sha256: bray_base::lowercase_hex(&Sha256::digest("linker map")),
        logical_provenance: bounded(vec!["std".to_owned()]),
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
        scheduler: bray_profile::CompilationProfileSchedulerStatistics::default(),
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
            maximum_active_workers: 1,
        }],
        queries: Vec::new(),
        metrics: vec![CompilationProfileMetric { id: 2, value: 3 }],
        runtime_artifacts: Vec::new(),
        runtime_roles: Vec::new(),
        native_callback_entries: Vec::new(),
        native_codegen: None,
        events: Vec::new(),
        dropped_events: 0,
    }
}
