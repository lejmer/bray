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

#[test]
fn custom_operator_output_does_not_constrain_its_operand_to_a_scalar() {
    let compilation = compilation(
        r#"
        module app;

        @copy
        struct Value
        {
            number: i32;
        }

        impl ValueNegate = Value(Negate)
        {
            type Output = i32;

            func negate() -> i32
            {
                return self.number;
            }
        }

        func selected(pos value: Value) -> i32
        {
            return -value;
        }
    "#,
    );

    let key = crate::test_support::source_function_body_key(&compilation, "selected");
    let lowered = compilation.lowered_unit(key).expect("fixture callable must lower successfully");

    assert!(
        !lowered.diagnostics().has_errors(),
        "{:#?}",
        lowered.diagnostics()
    );

    let mir = lowered.value().as_ref().expect("fixture optional value must be present").mir().expect("fixture instance must retain its MIR");

    assert!(mir.is_valid());

    assert!(
        mir.operations()
            .iter()
            .any(|operation| matches!(operation.kind(), bray_ir::MirOperationKind::Call(_)))
    );
}
