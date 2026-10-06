use crate::test_support::{compilation, source_callable_body_key};
use bray_diagnostics::DiagnosticKind;
use bray_testing::assert_goal_state_diagnostic_kind;

#[test]
fn unavailable_custom_operators_report_diagnostics_before_lowering() {
    let compilation = compilation(
        r#"module app;

struct Value {}

func compare(pos left: Value, pos right: Value) -> bool
{
    return left != right;
}
"#,
    );

    let key = source_callable_body_key(&compilation);

    let lowered = compilation
        .lowered_unit(key)
        .unwrap_or_else(|error| panic!("invalid custom operator must remain checkable: {error:?}"));

    assert!(lowered.value().is_none());

    assert_goal_state_diagnostic_kind(
        lowered.diagnostics(),
        DiagnosticKind::CheckingNoApplicableCandidate,
    );
}

#[test]
fn structures_without_primary_constructors_are_not_callable() {
    let compilation = compilation(
        r#"module app;

struct Value
{
    number: i32;
}

func create() -> Value
{
    return Value(1);
}
"#,
    );

    let key = source_callable_body_key(&compilation);

    let selections = compilation
        .semantic_selections(key.clone())
        .unwrap_or_else(|error| panic!("invalid structure call must be selectable: {error:?}"));

    assert_goal_state_diagnostic_kind(
        selections.diagnostics(),
        DiagnosticKind::CheckingNoApplicableCandidate,
    );

    let lowered = compilation
        .lowered_unit(key)
        .unwrap_or_else(|error| panic!("invalid structure call must remain checkable: {error:?}"));

    assert!(lowered.value().is_none());

    assert_goal_state_diagnostic_kind(
        lowered.diagnostics(),
        DiagnosticKind::CheckingNoApplicableCandidate,
    );
}

#[test]
fn built_in_operators_remain_available_without_trait_candidates() {
    let compilation = compilation(
        r#"module app;

func compare(pos left: i32, pos right: i32) -> bool
{
    return left != right;
}
"#,
    );

    let lowered = compilation
        .lowered_unit(source_callable_body_key(&compilation))
        .unwrap_or_else(|error| panic!("built-in operator must lower: {error:?}"));

    assert!(lowered.value().is_some());

    assert!(
        lowered.diagnostics().is_empty(),
        "{:#?}",
        lowered.diagnostics()
    );
}
