use super::support::fixtures::pattern_compilation;
use crate::test_support::{compilation, source_callable_body_key};
use bray_diagnostics::DiagnosticKind;
use bray_testing::assert_goal_state_diagnostic_kind;

#[test]
fn pattern_conditions_resolve_alternative_names_before_checking_coherence() {
    for body in [
        "let input: Choice = Empty; assert(input matches Data(_) | Empty);",
        "let input: Choice = Empty; match input { case Data(_) | Empty {} }",
        "let input: Choice = Empty; if let Data(_) | Empty = input {}",
    ] {
        let source = format!(
            "module app; union Choice {{ Data(pos value: i32); Empty; }} func main() {{ {body} }}"
        );

        let compilation = compilation(&source);

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{body}: {:?}",
            compilation.check_diagnostics()
        );

        assert!(
            compilation
                .lowered_unit(source_callable_body_key(&compilation))
                .unwrap()
                .value()
                .is_some()
        );
    }

    let compilation = compilation(
        "module app; union Choice { Data(pos value: i32); Empty; } func main() { let input: Choice = Empty; if let Data(value) | Empty = input {} }",
    );

    assert!(
        compilation
            .check_diagnostics()
            .by_kind(DiagnosticKind::BindingIncoherentAlternativePattern)
            .next()
            .is_some(),
        "{:?}",
        compilation.check_diagnostics()
    );
}

#[test]
fn pattern_conditions_check_and_lower_structural_tests_and_bindings() {
    for body in [
        "let value: bool = true; assert(value matches true);",
        "const limit: i32 = 7; let value: i32 = 7; assert(value matches limit);",
        "let value: char = 'a'; assert(value matches 'a');",
        "let value: r64 = 1.5; assert(value matches 1.5);",
        "let value: string = \"ready\"; assert(value matches \"ready\");",
        "let value: i32? = none; if let ?number = value { assert(number == 1); } else { assert(value matches none); }",
        "let mut value: i32? = none; while let ?number = value { assert(number == 1); break; } else { assert(value matches none); }",
        "let value: i32? = none; if false {} else if let ?number = value { assert(number == 1); } else {}",
        "let value: i32? = 1; if let ?number = value && number > 0 { assert(number == 1); }",
        "let value: i32? = 1; if true && let ?number = value && (number > 0 || false) { assert(number == 1); }",
        "let value: i32? = 1; if let ?number = value && let next = number + 1 && next > number { assert(next == 2); }",
        "let value: i32? = 1; while true && let ?number = value && number > 0 { assert(number == 1); break; } else {}",
        "let value: i32? = 1; if false {} else if let ?number = value && number > 0 { assert(number == 1); }",
        "let value: i32? = 1; if let ?number = value && let number = number + 1 { assert(number == 2); }",
        "if let true = (false || true) && true {}",
    ] {
        let compilation = pattern_compilation(body);

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{body}: {:?}",
            compilation.check_diagnostics()
        );

        let key = source_callable_body_key(&compilation);
        let lowered = compilation.lowered_unit(key).unwrap();

        assert!(lowered.value().is_some(), "{body}: {lowered:?}");
    }
}

#[test]
fn pattern_conditions_observe_owned_subjects_and_retain_move_errors() {
    for body in [
        "let input: Owned = Owned { value = 1 }; assert(input matches { value = 1 }); let transferred: Owned = input; assert(transferred.value == 1);",
        "let input: Owned = Owned { value = 1 }; if let { value } = input { assert(value == 1); } let transferred: Owned = input; assert(transferred.value == 1);",
        "if let { value } = Owned { value = 1 } { assert(value == 1); }",
    ] {
        let source = format!("module app; struct Owned {{ value: i32; }} func main() {{ {body} }}");

        let compilation = compilation(&source);

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{body}: {:?}",
            compilation.check_diagnostics()
        );

        assert!(
            compilation
                .lowered_unit(source_callable_body_key(&compilation))
                .unwrap()
                .value()
                .is_some()
        );
    }

    let compilation = compilation(
        "module app; struct Owned { value: i32; } func main() { let input: Owned = Owned { value = 1 }; let transferred: Owned = input; assert(input matches { value = 1 }); }",
    );

    assert!(
        compilation
            .check_diagnostics()
            .by_kind(DiagnosticKind::CheckingUseOfMovedStorage)
            .next()
            .is_some(),
        "{:?}",
        compilation.check_diagnostics()
    );
}

#[test]
fn pattern_conditions_reject_bindings_in_matches() {
    for pattern in ["number", "?number", "(number, _)", "[number, ..]"] {
        let initializer = match pattern {
            "?number" => "let value: i32? = none;",
            "(number, _)" => "let value: (i32, i32) = (1, 2);",
            "[number, ..]" => "let value: [i32; 2] = [1, 2];",
            _ => "let value: i32 = 1;",
        };

        let compilation =
            pattern_compilation(&format!("{initializer} assert(value matches {pattern});"));

        let analysis = compilation
            .patterns(source_callable_body_key(&compilation))
            .unwrap();

        assert!(
            analysis
                .diagnostics()
                .by_kind(DiagnosticKind::CheckingBindingInPatternTest)
                .next()
                .is_some(),
            "{pattern}: {:?}",
            analysis.diagnostics()
        );

        assert_goal_state_diagnostic_kind(
            analysis.diagnostics(),
            DiagnosticKind::CheckingBindingInPatternTest,
        );
    }
}

#[test]
fn pattern_conditions_cover_shapes_and_variant_spellings_without_equatable() {
    let source = r#"module app;
struct Point { x: i32; y: i32; }
union Choice { Data(pos value: i32); Empty; }
func main(pos choice: Choice, pos point: Point, pos optional: i32?, pos pair: (i32, bool), pos array: [i32; 2], pos boxed: box i32)
{
    assert(choice matches Data(_) | Empty);
    assert(choice matches Choice.Data(_) | .Empty);
    if choice matches Empty {} else if let Data(value) = choice { assert(value matches 0 | 1); }
    assert(point matches Point { x = 0, y = _ });
    assert(point matches { x = _, .. });
    if point matches Point { x = 0, .. } {}
    if point matches { x = 0, .. } {}
    assert(optional matches ?_ | none);
    assert(pair matches (0, true));
    assert(array matches [0, ..]);
    assert(boxed matches box(_));
}
"#;

    let compilation = compilation(source);

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:?}",
        compilation.check_diagnostics()
    );

    let lowered = compilation
        .lowered_unit(source_callable_body_key(&compilation))
        .unwrap();

    assert!(lowered.value().is_some(), "{lowered:?}");
}

#[test]
fn pattern_conditions_keep_bindings_out_of_failure_and_outer_scopes() {
    for body in [
        "let value: i32? = none; if let ?number = value {} else { assert(number == 0); }",
        "let value: i32? = none; if let ?number = value {} assert(number == 0);",
        "let value: i32? = none; while let ?number = value { break; } else { assert(number == 0); }",
        "let value: i32? = none; if let ?number = value && number > 0 {} else { assert(number == 0); }",
        "let value: i32? = none; if number > 0 && let ?number = value {}",
        "let value: i32? = none; if let ?number = value && number > 0 {} else if number > 0 {}",
        "let value: i32? = none; while let ?number = value && number > 0 { break; } assert(number == 0);",
    ] {
        let compilation = pattern_compilation(body);

        assert!(
            compilation
                .check_diagnostics()
                .by_kind(DiagnosticKind::BindingUnresolvedName)
                .next()
                .is_some(),
            "{body}: {:?}",
            compilation.check_diagnostics()
        );
    }
}

#[test]
fn pattern_conditions_binding_diagnostics_point_to_the_introduced_name() {
    for pattern in ["number", "{ number }"] {
        let source = format!(
            "module app; struct Value {{ number: i32; }} func main(pos input: Value) {{ input matches {pattern}; }}"
        );

        let compilation = compilation(&source);
        let diagnostics = compilation.check_diagnostics();

        let diagnostic = diagnostics
            .by_kind(DiagnosticKind::CheckingBindingInPatternTest)
            .next()
            .unwrap();

        let span = diagnostic.primary_span().unwrap();

        assert_eq!(
            span.start().bytes(),
            u32::try_from(source.rfind("number").unwrap()).unwrap()
        );

        assert_eq!(span.range().slice_str(&source).unwrap().trim(), "number");
    }
}

#[test]
fn pattern_conditions_allow_initializers_that_terminate_execution() {
    for body in [
        "if panic(\"stop\") {}",
        "if let _ = panic(\"stop\") {}",
        "while let _ = panic(\"stop\") {}",
        "panic(\"stop\") matches _;",
        "if true {} else if let _ = panic(\"stop\") {}",
        "if true && let _ = panic(\"stop\") {} else {}",
        "if let value = 1 && panic(\"stop\") { assert(value == 1); } else {}",
        "while let value = 1 && panic(\"stop\") { assert(value == 1); } else {}",
        "if panic(\"stop\") && let value = 1 { assert(value == 1); }",
        "let value: bool = !panic(\"stop\");",
        "let value: bool = true && panic(\"stop\");",
        "let value: bool = panic(\"stop\") && true;",
        "let value: bool = false || panic(\"stop\");",
        "let value: bool = panic(\"stop\") || false;",
    ] {
        let compilation = pattern_compilation(body);

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{body}: {:?}",
            compilation.check_diagnostics()
        );

        let lowered = compilation
            .lowered_unit(source_callable_body_key(&compilation))
            .unwrap();

        assert!(lowered.value().is_some(), "{body}: {lowered:?}");
    }
}
