use super::support::fixtures::pattern_compilation;
use crate::test_support::{compilation, source_callable_body_key};
use bray_diagnostics::{
    DiagnosticArgValue, DiagnosticKind, DiagnosticPatternMissingCase, DiagnosticRelatedLocationKind,
};
use bray_testing::assert_goal_state_diagnostic_kind;

#[test]
fn patterns_reject_constant_paths_with_incompatible_types() {
    let compilation = compilation(concat!(
        "module app;\n",
        "const one: i32 = 1;\n",
        "func main(value: bool)\n",
        "{\n",
        "    match value\n",
        "    {\n",
        "        case one\n",
        "        {\n",
        "        }\n",
        "    }\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    let analysis = match compilation.patterns(key) {
        Ok(analysis) => analysis,
        Err(error) => panic!("incompatible constant pattern must recover: {error:?}"),
    };

    assert_eq!(
        crate::test_support::diagnostic_kinds(analysis.diagnostics()),
        [DiagnosticKind::CheckingIncompatiblePattern]
    );

    assert!(analysis.value().is_recovered());
}

#[test]
fn patterns_report_non_exhaustive_matches() {
    let compilation = pattern_compilation(concat!(
        "    let value: bool = true;\n",
        "    match value\n",
        "    {\n",
        "        case true\n",
        "        {\n",
        "        }\n",
        "    }\n",
    ));

    let key = source_callable_body_key(&compilation);

    let analysis = match compilation.patterns(key) {
        Ok(analysis) => analysis,
        Err(error) => panic!("pattern analysis must be available: {error:?}"),
    };

    assert_eq!(
        crate::test_support::diagnostic_kinds(analysis.diagnostics()),
        [DiagnosticKind::CheckingNonExhaustiveMatch]
    );

    assert_goal_state_diagnostic_kind(
        analysis.diagnostics(),
        DiagnosticKind::CheckingNonExhaustiveMatch,
    );

    let missing = analysis
        .diagnostics()
        .by_kind(DiagnosticKind::CheckingNonExhaustiveMatch)
        .next()
        .and_then(|diagnostic| {
            diagnostic.args().iter().find_map(|arg| match arg.value() {
                DiagnosticArgValue::PatternCoverage(coverage) => Some(coverage.missing()),
                _ => None,
            })
        })
        .unwrap_or_else(|| panic!("non-exhaustive match must retain missing cases"));

    assert_eq!(missing, &[DiagnosticPatternMissingCase::Boolean(false)]);
}

#[test]
fn patterns_report_nullable_union_and_open_domain_missing_cases() {
    let nullable = pattern_compilation(concat!(
        "    let value: i32? = none;\n",
        "    match value\n",
        "    {\n",
        "        case ?present {}\n",
        "    }\n",
    ));

    let nullable_analysis = nullable
        .patterns(source_callable_body_key(&nullable))
        .unwrap_or_else(|error| panic!("nullable coverage must publish: {error:?}"));

    let nullable_missing = nullable_analysis
        .diagnostics()
        .by_kind(DiagnosticKind::CheckingNonExhaustiveMatch)
        .next()
        .and_then(|diagnostic| {
            diagnostic.args().iter().find_map(|arg| match arg.value() {
                DiagnosticArgValue::PatternCoverage(coverage) => Some(coverage.missing()),
                _ => None,
            })
        })
        .unwrap_or_else(|| panic!("nullable match must retain its missing case"));

    assert_eq!(
        nullable_missing,
        &[DiagnosticPatternMissingCase::NullableAbsent]
    );

    let union = compilation(concat!(
        "module app;\n",
        "union Choice { First; Second; }\n",
        "func main(value: Choice)\n",
        "{\n",
        "    match value { case .First {} }\n",
        "}\n",
    ));

    let union_analysis = union
        .patterns(source_callable_body_key(&union))
        .unwrap_or_else(|error| panic!("union coverage must publish: {error:?}"));

    let union_missing = union_analysis
        .diagnostics()
        .by_kind(DiagnosticKind::CheckingNonExhaustiveMatch)
        .next()
        .and_then(|diagnostic| {
            diagnostic.args().iter().find_map(|arg| match arg.value() {
                DiagnosticArgValue::PatternCoverage(coverage) => Some(coverage.missing()),
                _ => None,
            })
        })
        .unwrap_or_else(|| panic!("union match must retain its missing case"));

    assert_eq!(
        union_missing,
        &[DiagnosticPatternMissingCase::UnionVariant(
            "Second".to_owned()
        )]
    );

    let open = pattern_compilation(concat!(
        "    let value: i32 = 1;\n",
        "    match value { case 1 {} }\n",
    ));

    let open_analysis = open
        .patterns(source_callable_body_key(&open))
        .unwrap_or_else(|error| panic!("open-domain coverage must publish: {error:?}"));

    let open_missing = open_analysis
        .diagnostics()
        .by_kind(DiagnosticKind::CheckingNonExhaustiveMatch)
        .next()
        .and_then(|diagnostic| {
            diagnostic.args().iter().find_map(|arg| match arg.value() {
                DiagnosticArgValue::PatternCoverage(coverage) => Some(coverage.missing()),
                _ => None,
            })
        })
        .unwrap_or_else(|| panic!("open match must retain catch-all context"));

    assert_eq!(
        open_missing,
        &[DiagnosticPatternMissingCase::RemainingValues]
    );
}

#[test]
fn patterns_bound_missing_union_cases_and_retain_every_covering_origin() {
    let bounded = compilation(concat!(
        "module app;\n",
        "union Choice { A; B; C; D; E; F; G; H; I; J; }\n",
        "func main(value: Choice)\n",
        "{\n",
        "    match value { case .A {} }\n",
        "}\n",
    ));

    let bounded_analysis = bounded
        .patterns(source_callable_body_key(&bounded))
        .unwrap_or_else(|error| panic!("bounded union coverage must publish: {error:?}"));

    let coverage = bounded_analysis
        .diagnostics()
        .by_kind(DiagnosticKind::CheckingNonExhaustiveMatch)
        .next()
        .and_then(|diagnostic| {
            diagnostic.args().iter().find_map(|arg| match arg.value() {
                DiagnosticArgValue::PatternCoverage(coverage) => Some(coverage),
                _ => None,
            })
        })
        .unwrap_or_else(|| panic!("bounded union match must retain coverage context"));

    assert_eq!(coverage.missing().len(), 8);
    assert_eq!(coverage.omitted_count(), 1);

    let covered = pattern_compilation(concat!(
        "    let value: bool = true;\n",
        "    match value\n",
        "    {\n",
        "        case true | false {}\n",
        "        case true | false {}\n",
        "    }\n",
    ));

    let covered_patterns = covered
        .patterns(source_callable_body_key(&covered))
        .unwrap_or_else(|error| panic!("covered alternatives must publish: {error:?}"));

    let diagnostic = covered_patterns
        .diagnostics()
        .by_kind(DiagnosticKind::CheckingUnreachableMatchArm)
        .next()
        .unwrap_or_else(|| panic!("covered arm must be diagnosed"));

    assert_eq!(
        diagnostic
            .related_locations()
            .iter()
            .filter(|related| { related.kind() == DiagnosticRelatedLocationKind::CoveredByPattern })
            .count(),
        2
    );
}

#[test]
fn patterns_report_unreachable_match_arms() {
    let compilation = pattern_compilation(concat!(
        "    let value: bool = true;\n",
        "    match value\n",
        "    {\n",
        "        case true\n",
        "        {\n",
        "        }\n",
        "\n",
        "        case true\n",
        "        {\n",
        "        }\n",
        "\n",
        "        case false\n",
        "        {\n",
        "        }\n",
        "    }\n",
    ));

    let key = source_callable_body_key(&compilation);

    let analysis = match compilation.patterns(key) {
        Ok(analysis) => analysis,
        Err(error) => panic!("pattern analysis must be available: {error:?}"),
    };

    let [coverage] = analysis.value().matches() else {
        panic!("test source must contain one match expression");
    };

    assert_eq!(coverage.unreachable_arms(), &[1]);

    assert_eq!(
        crate::test_support::diagnostic_kinds(analysis.diagnostics()),
        [DiagnosticKind::CheckingUnreachableMatchArm]
    );

    let diagnostic = analysis
        .diagnostics()
        .by_kind(DiagnosticKind::CheckingUnreachableMatchArm)
        .next()
        .unwrap_or_else(|| panic!("duplicate match arm must be diagnosed"));

    assert_eq!(
        diagnostic
            .related_locations()
            .iter()
            .filter(|related| { related.kind() == DiagnosticRelatedLocationKind::CoveredByPattern })
            .count(),
        1
    );

    assert_goal_state_diagnostic_kind(
        analysis.diagnostics(),
        DiagnosticKind::CheckingUnreachableMatchArm,
    );
}

#[test]
fn patterns_report_patterns_incompatible_with_the_subject() {
    let compilation = pattern_compilation(concat!(
        "    let value: bool = true;\n",
        "    match value\n",
        "    {\n",
        "        case none\n",
        "        {\n",
        "        }\n",
        "    }\n",
    ));

    let key = source_callable_body_key(&compilation);

    let analysis = match compilation.patterns(key) {
        Ok(analysis) => analysis,
        Err(error) => panic!("pattern analysis must be available: {error:?}"),
    };

    assert_eq!(
        crate::test_support::diagnostic_kinds(analysis.diagnostics()),
        [DiagnosticKind::CheckingIncompatiblePattern]
    );

    assert_goal_state_diagnostic_kind(
        analysis.diagnostics(),
        DiagnosticKind::CheckingIncompatiblePattern,
    );
}

#[test]
fn byte_literal_patterns_reject_mismatched_fixed_extents() {
    let compilation = pattern_compilation(concat!(
        "    let value: bytes = b\"abc\";\n",
        "    let mismatch: bool = value matches b\"abcd\";\n",
    ));

    let key = source_callable_body_key(&compilation);

    let analysis = compilation
        .patterns(key)
        .expect("byte literal pattern analysis must be available");

    assert_eq!(
        crate::test_support::diagnostic_kinds(analysis.diagnostics()),
        [DiagnosticKind::CheckingIncompatiblePattern]
    );

    assert!(analysis.value().is_recovered());
}

#[test]
fn integer_literal_patterns_reject_values_outside_the_subject_range() {
    let compilation = pattern_compilation(concat!(
        "    let value: u8 = 1;\n",
        "    let mismatch: bool = value matches 256;\n",
    ));

    let key = source_callable_body_key(&compilation);

    let analysis = compilation
        .patterns(key)
        .expect("integer literal pattern analysis must be available");

    assert_eq!(
        crate::test_support::diagnostic_kinds(analysis.diagnostics()),
        [DiagnosticKind::CheckingIncompatiblePattern]
    );

    assert!(analysis.value().is_recovered());
}

#[test]
fn patterns_reject_refutable_declaration_patterns() {
    let compilation = pattern_compilation("    let true: bool = true;\n");
    let key = source_callable_body_key(&compilation);

    let analysis = match compilation.patterns(key) {
        Ok(analysis) => analysis,
        Err(error) => panic!("pattern analysis must be available: {error:?}"),
    };

    assert_eq!(
        crate::test_support::diagnostic_kinds(analysis.diagnostics()),
        [DiagnosticKind::CheckingRefutablePattern]
    );

    assert_goal_state_diagnostic_kind(
        analysis.diagnostics(),
        DiagnosticKind::CheckingRefutablePattern,
    );
}
