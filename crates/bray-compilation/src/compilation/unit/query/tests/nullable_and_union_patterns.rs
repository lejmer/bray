use super::support::fixtures::pattern_compilation;
use crate::test_support::{compilation, source_callable_body_key};
use bray_diagnostics::DiagnosticKind;
use bray_testing::assert_goal_state_diagnostic_kind;

#[test]
fn patterns_publish_exhaustive_nullable_match_coverage() {
    let compilation = pattern_compilation(concat!(
        "    let value: i32? = none;\n",
        "    match value\n",
        "    {\n",
        "        case ?present\n",
        "        {\n",
        "        }\n",
        "\n",
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

    let [coverage] = analysis.value().matches() else {
        panic!("test source must contain one match expression");
    };

    assert!(coverage.is_exhaustive());
    assert!(coverage.unreachable_arms().is_empty());

    assert!(
        analysis.diagnostics().is_empty(),
        "{:?}",
        analysis.diagnostics()
    );
}

#[test]
fn patterns_compose_nested_nullable_coverage() {
    let compilation = pattern_compilation(concat!(
        "    let value: bool? = none;\n",
        "    match value\n",
        "    {\n",
        "        case none\n",
        "        {\n",
        "        }\n",
        "\n",
        "        case ?true\n",
        "        {\n",
        "        }\n",
        "\n",
        "        case ?false\n",
        "        {\n",
        "        }\n",
        "    }\n",
    ));

    let key = source_callable_body_key(&compilation);

    let analysis = match compilation.patterns(key) {
        Ok(analysis) => analysis,
        Err(error) => panic!("nullable pattern analysis must be available: {error:?}"),
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

#[test]
fn patterns_publish_exhaustive_closed_union_coverage() {
    let compilation = compilation(concat!(
        "module app;\n",
        "union Choice\n",
        "{\n",
        "    First;\n",
        "    Second;\n",
        "}\n",
        "func main(value: Choice)\n",
        "{\n",
        "    match value\n",
        "    {\n",
        "        case .First\n",
        "        {\n",
        "        }\n",
        "\n",
        "        case .Second\n",
        "        {\n",
        "        }\n",
        "    }\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    let analysis = match compilation.patterns(key) {
        Ok(analysis) => analysis,
        Err(error) => panic!("pattern analysis must be available: {error:?}"),
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
}

#[test]
fn borrowed_union_subjects_use_the_referent_for_pattern_semantics() {
    let compilation = compilation(concat!(
        "module app;\n",
        "union Choice\n",
        "{\n",
        "    First;\n",
        "    Second;\n",
        "}\n",
        "func main(pos value: &Choice)\n",
        "{\n",
        "    match value\n",
        "    {\n",
        "        case .First {}\n",
        "        case .Second {}\n",
        "    }\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    let analysis = match compilation.patterns(key) {
        Ok(analysis) => analysis,
        Err(error) => panic!("borrowed pattern analysis must be available: {error:?}"),
    };

    let [coverage] = analysis.value().matches() else {
        panic!("test source must contain one match expression");
    };

    assert!(coverage.is_exhaustive(), "{analysis:?}");
    assert!(!analysis.value().is_recovered());

    assert!(
        analysis.diagnostics().is_empty(),
        "{:?}",
        analysis.diagnostics()
    );
}

#[test]
fn patterns_resolve_bare_variants_through_the_expected_subject_type() {
    let compilation = compilation(concat!(
        "module app;\n",
        "union Choice\n",
        "{\n",
        "    First;\n",
        "}\n",
        "func main(value: Choice)\n",
        "{\n",
        "    match value\n",
        "    {\n",
        "        case First\n",
        "        {\n",
        "        }\n",
        "    }\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    let bound = match compilation.bound_unit(key.clone()) {
        Ok(bound) => bound,
        Err(error) => panic!("bare variant must bind: {error:?}"),
    };

    assert!(bound.value().local_symbols().bindings().is_empty());
    assert!(bound.diagnostics().is_empty(), "{:?}", bound.diagnostics());

    let analysis = match compilation.patterns(key) {
        Ok(analysis) => analysis,
        Err(error) => panic!("bare variant pattern analysis must be available: {error:?}"),
    };

    let [coverage] = analysis.value().matches() else {
        panic!("test source must contain one match expression");
    };

    assert!(coverage.is_exhaustive(), "{analysis:?}");
    assert!(analysis.value().binding_types().is_empty());
    assert!(!analysis.value().is_recovered());

    assert!(
        analysis.diagnostics().is_empty(),
        "{:?}",
        analysis.diagnostics()
    );
}

#[test]
fn single_variant_tagless_unions_preserve_exact_variant_knowledge() {
    let compilation = compilation(concat!(
        "module app;\n",
        "@layout(c, tag = none)\n",
        "union Choice\n",
        "{\n",
        "    Only;\n",
        "}\n",
        "func main()\n",
        "{\n",
        "    match Choice.Only\n",
        "    {\n",
        "        case .Only {}\n",
        "    }\n",
        "    let preserved: Choice = Choice.Only;\n",
        "    match preserved\n",
        "    {\n",
        "        case .Only {}\n",
        "    }\n",
        "}\n",
    ));

    let analysis = compilation
        .patterns(source_callable_body_key(&compilation))
        .unwrap_or_else(|error| panic!("tagless patterns must be available: {error:?}"));

    assert_eq!(analysis.value().matches().len(), 2);

    assert!(
        analysis
            .value()
            .matches()
            .iter()
            .all(bray_bound_tree::MatchCoverageEntry::is_exhaustive),
        "{analysis:?}"
    );

    assert!(!analysis.value().is_recovered(), "{analysis:?}");

    assert!(
        analysis.diagnostics().is_empty(),
        "{:#?}",
        analysis.diagnostics()
    );
}

#[test]
fn multi_variant_tagless_union_patterns_require_active_variant_facts() {
    let compilation = compilation(concat!(
        "module app;\n",
        "@layout(c, tag = none)\n",
        "union Choice\n",
        "{\n",
        "    First;\n",
        "    Second;\n",
        "}\n",
        "func inspect(value: Choice)\n",
        "{\n",
        "    match value\n",
        "    {\n",
        "        case .First {}\n",
        "        case .Second {}\n",
        "    }\n",
        "}\n",
    ));

    let analysis = compilation
        .patterns(source_callable_body_key(&compilation))
        .unwrap_or_else(|error| panic!("tagless patterns must be available: {error:?}"));

    assert_goal_state_diagnostic_kind(
        analysis.diagnostics(),
        DiagnosticKind::CheckingTaglessUnionPatternRequiresVariant,
    );
}
