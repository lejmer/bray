use super::support::fixtures::pattern_compilation;
use crate::test_support::{compilation, source_callable_body_key};
use bray_bound_tree::{PatternOperation, PatternPredicate};
use bray_diagnostics::{DiagnosticKind, DiagnosticRelatedLocationKind};
use bray_testing::assert_goal_state_diagnostic_kind;
use std::sync::Arc;

#[test]
fn patterns_publish_exhaustive_boolean_match_coverage() {
    let compilation = pattern_compilation(concat!(
        "    let value: bool = true;\n",
        "    match value\n",
        "    {\n",
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

    assert_eq!(
        compilation.state.checked_patterns.is_published(&key),
        Ok(false)
    );

    let analysis = match compilation.patterns(key.clone()) {
        Ok(analysis) => analysis,
        Err(error) => panic!("pattern analysis must be available: {error:?}"),
    };

    assert_eq!(
        compilation.state.checked_patterns.is_published(&key),
        Ok(true)
    );

    let repeated = match compilation.patterns(key) {
        Ok(analysis) => analysis,
        Err(error) => panic!("repeated pattern analysis must be available: {error:?}"),
    };

    assert!(Arc::ptr_eq(&analysis, &repeated));

    let [coverage] = analysis.value().matches() else {
        panic!("test source must contain one match expression");
    };

    assert!(coverage.is_exhaustive(), "{analysis:?}");
    assert!(coverage.unreachable_arms().is_empty());

    assert!(
        analysis.diagnostics().is_empty(),
        "{:?}",
        analysis.diagnostics()
    );

    let literal_patterns = analysis
        .value()
        .patterns()
        .iter()
        .filter(|pattern| matches!(pattern.test(), Some(PatternPredicate::Literal(_))))
        .collect::<Vec<_>>();

    assert_eq!(literal_patterns.len(), 2);

    assert!(literal_patterns.iter().all(|pattern| {
        pattern.operation() == PatternOperation::Observe
            && matches!(pattern.refinement(), Some(PatternPredicate::Literal(_)))
    }));
}

#[test]
fn patterns_use_constant_paths_for_boolean_coverage() {
    let compilation = compilation(concat!(
        "module app;\n",
        "const yes: bool = true;\n",
        "const no: bool = false;\n",
        "func main(value: bool)\n",
        "{\n",
        "    match value\n",
        "    {\n",
        "        case yes\n",
        "        {\n",
        "        }\n",
        "\n",
        "        case no\n",
        "        {\n",
        "        }\n",
        "    }\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    let analysis = match compilation.patterns(key) {
        Ok(analysis) => analysis,
        Err(error) => panic!("constant-backed pattern analysis must be available: {error:?}"),
    };

    let [coverage] = analysis.value().matches() else {
        panic!("test source must contain one match expression");
    };

    assert!(coverage.is_exhaustive(), "{analysis:?}");
    assert!(coverage.unreachable_arms().is_empty());

    assert!(
        analysis.diagnostics().is_empty(),
        "{:?}",
        analysis.diagnostics()
    );

    assert_eq!(
        analysis
            .value()
            .patterns()
            .iter()
            .filter(|pattern| matches!(pattern.test(), Some(PatternPredicate::Constant(_))))
            .count(),
        2
    );
}

#[test]
fn patterns_report_subsumed_constant_alternatives() {
    let compilation = compilation(concat!(
        "module app;\n",
        "const yes: bool = true;\n",
        "func main(value: bool)\n",
        "{\n",
        "    match value\n",
        "    {\n",
        "        case yes | true\n",
        "        {\n",
        "        }\n",
        "\n",
        "        case false\n",
        "        {\n",
        "        }\n",
        "    }\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    let analysis = match compilation.patterns(key) {
        Ok(analysis) => analysis,
        Err(error) => panic!("constant alternative analysis must be available: {error:?}"),
    };

    let [coverage] = analysis.value().matches() else {
        panic!("test source must contain one match expression");
    };

    assert!(coverage.is_exhaustive(), "{analysis:?}");

    assert_eq!(
        crate::test_support::diagnostic_kinds(analysis.diagnostics()),
        [DiagnosticKind::CheckingUnreachablePatternAlternative]
    );

    let diagnostic = analysis
        .diagnostics()
        .by_kind(DiagnosticKind::CheckingUnreachablePatternAlternative)
        .next()
        .unwrap_or_else(|| panic!("subsumed alternative must be diagnosed"));

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
        DiagnosticKind::CheckingUnreachablePatternAlternative,
    );
}

#[test]
fn patterns_use_constant_guard_truth_for_coverage() {
    let compilation = compilation(concat!(
        "module app;\n",
        "const enabled: bool = true;\n",
        "const disabled: bool = false;\n",
        "func main(value: bool)\n",
        "{\n",
        "    match value\n",
        "    {\n",
        "        case true when disabled\n",
        "        {\n",
        "        }\n",
        "\n",
        "        case true when enabled\n",
        "        {\n",
        "        }\n",
        "\n",
        "        case false\n",
        "        {\n",
        "        }\n",
        "    }\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    let analysis = match compilation.patterns(key) {
        Ok(analysis) => analysis,
        Err(error) => panic!("constant guard analysis must be available: {error:?}"),
    };

    let [coverage] = analysis.value().matches() else {
        panic!("test source must contain one match expression");
    };

    assert!(coverage.is_exhaustive(), "{analysis:?}");
    assert_eq!(coverage.unreachable_arms(), &[0]);

    assert_eq!(
        crate::test_support::diagnostic_kinds(analysis.diagnostics()),
        [DiagnosticKind::CheckingUnreachableMatchArm]
    );

    assert_goal_state_diagnostic_kind(
        analysis.diagnostics(),
        DiagnosticKind::CheckingUnreachableMatchArm,
    );
}

#[test]
fn patterns_evaluate_closed_local_constant_patterns() {
    let compilation = pattern_compilation(concat!(
        "    const base: bool = true;\n",
        "    const yes: bool = base;\n",
        "    let value: bool = true;\n",
        "    match value\n",
        "    {\n",
        "        case yes\n",
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
        Err(error) => panic!("local constant pattern analysis must be available: {error:?}"),
    };

    let [coverage] = analysis.value().matches() else {
        panic!("test source must contain one match expression");
    };

    assert!(coverage.is_exhaustive(), "{analysis:?}");

    assert!(
        analysis.diagnostics().is_empty(),
        "{:?}",
        analysis.diagnostics()
    );
}
