use super::{CompilationProfileConfiguration, CompilationProfileMode};
use crate::test_support::{package_identity, source_input};
use crate::{Compilation, CompilationRequest};

#[test]
fn disabled_compilations_do_not_create_profile_state() {
    let compilation = Compilation::load(CompilationRequest::new(
        package_identity(),
        vec![source_input("module test.package;\n", 1)],
    ))
    .unwrap_or_else(|error| panic!("test compilation must load: {error:?}"));

    let _ = compilation.check_diagnostics();

    assert!(compilation.profile_report().is_none());
}

#[test]
fn profiling_preserves_compilation_diagnostics() {
    let baseline = Compilation::load(CompilationRequest::new(
        package_identity(),
        vec![source_input("module test.package;\nfn broken( {\n", 1)],
    ))
    .unwrap_or_else(|error| panic!("baseline compilation must load: {error:?}"));

    let profiled = Compilation::load(
        CompilationRequest::new(
            package_identity(),
            vec![source_input("module test.package;\nfn broken( {\n", 1)],
        )
        .with_profile(CompilationProfileConfiguration::new(
            CompilationProfileMode::Trace,
        )),
    )
    .unwrap_or_else(|error| panic!("profiled compilation must load: {error:?}"));

    assert_eq!(baseline.check_diagnostics(), profiled.check_diagnostics());
}

#[test]
fn enabled_compilations_report_queries_metrics_and_selected_detail() {
    let request = CompilationRequest::new(
        package_identity(),
        vec![source_input("module test.package;\n", 1)],
    )
    .with_profile(CompilationProfileConfiguration::new(
        CompilationProfileMode::Summary,
    ));

    let compilation = Compilation::load(request)
        .unwrap_or_else(|error| panic!("test compilation must load: {error:?}"));

    let _ = compilation.check_diagnostics();

    let report = compilation
        .profile_report()
        .unwrap_or_else(|| panic!("profiled compilation must retain a report"));

    assert_eq!(report.mode, CompilationProfileMode::Summary);
    assert!(report.queries.iter().any(|query| query.requests > 0));
    assert!(report.queries.iter().any(|query| query.cache_misses > 0));

    assert!(
        report
            .metrics
            .iter()
            .any(|metric| metric.name == "compiler.source.units" && metric.value == 1)
    );

    assert!(report.events.is_empty());
}

#[test]
fn updated_snapshots_report_reuse_and_invalidation() {
    let request = CompilationRequest::new(
        package_identity(),
        vec![source_input("module test.package;\n", 1)],
    )
    .with_profile(CompilationProfileConfiguration::new(
        CompilationProfileMode::Summary,
    ));

    let compilation = Compilation::load(request)
        .unwrap_or_else(|error| panic!("test compilation must load: {error:?}"));

    let _ = compilation.check_diagnostics();

    let updated = compilation
        .updated_sources(vec![source_input(
            "module test.package;\nfn added() {}\n",
            2,
        )])
        .unwrap_or_else(|error| panic!("updated compilation must load: {error:?}"));

    let report = updated
        .profile_report()
        .unwrap_or_else(|| panic!("updated compilation must retain profiling"));

    assert!(
        report
            .queries
            .iter()
            .any(|query| query.cross_snapshot_reuses > 0)
    );

    assert!(report.queries.iter().any(|query| query.invalidations > 0));
}

#[test]
fn profile_reports_round_trip_through_the_machine_schema() {
    let request = CompilationRequest::new(
        package_identity(),
        vec![source_input("module test.package;\n", 1)],
    )
    .with_profile(CompilationProfileConfiguration::new(
        CompilationProfileMode::Trace,
    ));

    let compilation = Compilation::load(request)
        .unwrap_or_else(|error| panic!("test compilation must load: {error:?}"));

    let _ = compilation.syntax_tree_result();

    let report = compilation
        .profile_report()
        .unwrap_or_else(|| panic!("profiled compilation must retain a report"));

    let encoded = serde_json::to_vec(&report)
        .unwrap_or_else(|error| panic!("profile report must serialize: {error:?}"));

    let decoded = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("profile report must deserialize: {error:?}"));

    assert_eq!(report, decoded);
}
