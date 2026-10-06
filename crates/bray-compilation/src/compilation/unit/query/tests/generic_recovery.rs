use crate::test_support::compilation;
use bray_diagnostics::DiagnosticKind;
use bray_testing::assert_goal_state_diagnostic_kind;

#[test]
fn generic_reference_arity_recovers_without_disabling_call_inference() {
    const REFERENCE_TYPE: &str = "func(pos value: bool, pos other: bool) -> bool";

    for (expression, ty, valid) in [
        ("identity<bool>", REFERENCE_TYPE, false),
        ("identity<bool, bool, bool>", REFERENCE_TYPE, false),
        ("identity<bool, bool>", REFERENCE_TYPE, true),
        ("identity<bool>(true, true)", "bool", true),
        ("VALUE<1>", "i32", false),
        ("VALUE<1, 2, 3>", "i32", false),
        ("VALUE<1, 2>", "i32", true),
    ] {
        let source = format!(
            r#"
                module app;

                static VALUE<const N: i32, const M: i32>: i32 = N;

                func identity<T, U>(pos value: T, pos other: U) -> T
                {{
                    return value;
                }}

                func check()
                {{
                    let value: {ty} = {expression};
                }}
            "#
        );

        let application = compilation(&source);
        let diagnostics = application.check_diagnostics();

        if valid {
            assert!(diagnostics.is_empty(), "{source}\n{diagnostics:?}");
        } else {
            assert_goal_state_diagnostic_kind(
                diagnostics,
                DiagnosticKind::BindingGenericArgumentCountMismatch,
            );
        }
    }
}

#[test]
fn generic_calls_reject_excess_explicit_arguments() {
    let application = compilation(
        r#"
            module app;

            func identity<T>(pos value: T) -> T
            {
                return value;
            }

            func check()
            {
                let value = identity<bool, bool>(true);
            }
        "#,
    );

    assert_goal_state_diagnostic_kind(
        application.check_diagnostics(),
        DiagnosticKind::CheckingIncompatibleCandidate,
    );
}

#[test]
fn malformed_generic_call_type_arguments_preserve_the_binding_error() {
    let application = compilation(
        r#"
            module app;

            func identity<T>(pos value: T) -> T
            {
                return value;
            }

            func check()
            {
                let value = identity<1>(true);
            }
        "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        application.check_diagnostics(),
        DiagnosticKind::BindingGenericArgumentMustBeType,
    );
}

#[test]
fn malformed_generic_type_forms_report_the_application_base() {
    for ty in ["(bool)<bool>", "Result<bool, bool> <bool>", "(bool?)<bool>"] {
        let source = format!(
            r#"
                module app;

                func check(pos value: {ty})
                {{
                }}
            "#
        );

        let application = compilation(&source);
        let diagnostics = application.check_diagnostics();

        bray_testing::assert_goal_state_diagnostic_kind(
            diagnostics,
            DiagnosticKind::BindingGenericApplicationRequiresName,
        );
    }
}

#[test]
fn malformed_generic_applications_preserve_source_diagnostics() {
    for ty in [
        "Result",
        "Result<bool>",
        "Result<bool, bool, bool>",
        "Boxed",
        "Boxed<bool, bool>",
        "action",
        "action<bool, bool>",
        "&view Marker",
        "&view Marker<bool, bool>",
        "Boxed<Result<bool>>",
        "T<bool>",
        "bool<bool>",
        "T(Marker).Item",
    ] {
        let source = format!(
            r#"
                module app;

                struct Boxed<T>
                {{
                    value: T;
                }}

                callable action<T> = func(pos value: T) -> T;

                trait Marker<T>
                {{
                }}

                func check<T>(pos value: {ty})
                {{
                }}
            "#
        );

        let application = compilation(&source);
        let diagnostics = application.check_diagnostics();

        assert!(
            diagnostics.iter().any(|diagnostic| diagnostic.kind()
                == DiagnosticKind::BindingGenericArgumentCountMismatch),
            "{source}\n{diagnostics:?}"
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &diagnostics,
            DiagnosticKind::BindingGenericArgumentCountMismatch,
        );
    }
}

#[test]
fn malformed_generic_syntax_preserves_parser_diagnostics() {
    for ty in ["Result<, bool>"] {
        let source = format!(
            r#"
                module app;

                func check(pos value: {ty})
                {{
                }}
            "#
        );

        let application = compilation(&source);

        assert!(application.source_diagnostics().is_empty(), "{source}");

        let syntax = application.syntax_tree_result().diagnostics();

        assert!(syntax.has_errors(), "{source}");

        let diagnostics = application.check_diagnostics();

        for diagnostic in syntax.iter() {
            assert!(
                diagnostics
                    .iter()
                    .any(|published| published.kind() == diagnostic.kind()),
                "{source}\n{diagnostics:?}"
            );
        }
    }
}

#[test]
fn malformed_generic_tokens_do_not_abort_diagnostic_collection() {
    let source = r#"
            module app;

            func forward(pos input: Result<&bool, bool>) -> Result<bool, bool>
            {
                let value = try input;

                return Ok(true);
            }
        "#;

    let application = compilation(source);
    let diagnostics = application.check_diagnostics();

    bray_testing::assert_goal_state_diagnostic_kind(
        &diagnostics,
        DiagnosticKind::LexicalInvalidOperatorOrPunctuation,
    );

    let valid = compilation(&source.replace("Result<&bool, bool>", "Result<(&bool), bool>"));

    assert!(
        valid.check_diagnostics().is_empty(),
        "{:?}",
        valid.check_diagnostics()
    );
}

#[test]
fn malformed_generic_trait_applications_recover_across_consumers() {
    for body in [
        r#"
                struct Value
                {
                }

                impl Value(Marker)
                {
                    func example()
                    {
                    }
                }
            "#,
        r#"
                struct Value
                {
                }

                impl Value(Marker)
                {
                }
            "#,
        r#"
                func check<T>(pos value: T) with(T: Marker)
                {
                }
            "#,
        r#"
                func check(pos value: bool) with(bool: Marker<bool, bool>)
                {
                }
            "#,
    ] {
        let source = format!(
            r#"
                module app;

                trait Marker<T>
                {{
                }}

                {body}
            "#
        );

        let application = compilation(&source);
        let diagnostics = application.check_diagnostics();

        bray_testing::assert_goal_state_diagnostic_kind(
            &diagnostics,
            DiagnosticKind::BindingGenericArgumentCountMismatch,
        );
    }
}

#[test]
fn malformed_generic_type_arguments_recover_without_syntax_contract_failures() {
    for ty in ["Result<1, bool>", "Result<true, bool>", "Result<bool, 1>"] {
        let source = format!(
            r#"
                module app;

                func check(pos value: {ty})
                {{
                }}
            "#
        );

        let application = compilation(&source);
        let diagnostics = application.check_diagnostics();

        bray_testing::assert_goal_state_diagnostic_kind(
            &diagnostics,
            DiagnosticKind::BindingGenericArgumentMustBeType,
        );
    }
}
