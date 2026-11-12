use bray_diagnostics::{DiagnosticKind};
use crate::test_support::{source_function_body_key};
use super::fixtures::{production_host_compilation};

#[test]
#[ignore = "tier:proof"]
fn total_indexing_requires_a_live_bound_on_the_selected_sequence() {
    for (body, valid) in [
        (
            "if index < values.length() { return values[index]; } return 0;",
            true,
        ),
        (
            "if index >= values.length() { return 0; } return values[index];",
            true,
        ),
        (
            "if index <= values.length() { return values[index]; } return 0;",
            false,
        ),
        (
            "if index < other.length() { return values[index]; } return 0;",
            false,
        ),
        ("return values[index];", false),
        (
            "if index < values.length() { index = values.length(); return values[index]; } return 0;",
            false,
        ),
        (
            "if index < values.length() { values = other; return values[index]; } return 0;",
            false,
        ),
        (
            "let length: usize = values.length(); values = other; if index < length { return values[index]; } return 0;",
            false,
        ),
    ] {
        let body = body
            .replace("values", "selected")
            .replace("selected = other;", "selected = &other[..];");

        let caller = format!(
            r#"
            module app;

            func production_probe(pos values: &[u64], pos other: &[u64], pos mut index: usize) -> u64
                executes(total)
            {{
                let mut selected = &values[..];

                {body}
            }}
            "#
        );

        let compilation = production_host_compilation(&caller);

        let checked = compilation
            .execution_properties(source_function_body_key(&compilation, "production_probe"))
            .expect("bounded indexing proof must publish");

        assert_eq!(
            checked
                .value()
                .properties
                .contains(&bray_symbols::ExecutionProperty::Total),
            valid,
            "{body}: {checked:#?}"
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                checked.diagnostics(),
                DiagnosticKind::CheckingExecutionGuaranteeNotProven,
            );
        }
    }
}

#[test]
#[ignore = "tier:proof"]
fn total_indexing_preserves_bounds_across_optional_borrow_initialization() {
    for body in [
        "return &mut values[index];",
        "let mut result: (&mut u64)? = none; result = &mut values[index]; return result;",
    ] {
        let caller = format!(
            r#"
            module app;
                        func production_probe(pos values: &mut [u64], pos index: usize) -> (&mut u64)?
                            executes(total)
                        {{
                            let count = values.length();

                            if index < count
                            {{
                                {body}
                            }}

                            return none;
                        }}
            "#
        );

        let compilation = production_host_compilation(&caller);

        let checked = compilation
            .execution_properties(source_function_body_key(&compilation, "production_probe"))
            .expect("optional borrow initialization retains the selected bounds");

        assert!(!checked.diagnostics().has_errors(), "{body}: {checked:#?}");
    }
}

#[test]
#[ignore = "tier:proof"]
fn total_loop_bounds_keep_the_evaluated_condition_across_iterations() {
    let compilation = production_host_compilation(
        r#"
        module app;

        func production_probe(pos values: &mut [u64], pos wanted: u64) -> (&mut u64)?
            executes(total)
        {
            let count = values.length();
            let mut index: usize = 0;

            while index < count
            {
                if values[index] != 0 && (values[index] == wanted || wanted == 0)
                {
                    let mut result: (&mut u64)? = none;

                    result = &mut values[index];
                    return result;
                }

                index += 1;
            }

            return none;
        }
        "#,
    );

    let checked = compilation
        .execution_properties(source_function_body_key(&compilation, "production_probe"))
        .expect("loop conditions retain their evaluated identities");

    assert!(!checked.diagnostics().has_errors(), "{checked:#?}");
}

#[test]
#[ignore = "tier:proof"]
fn total_record_scans_require_a_bounded_unaliased_counter() {
    for (guard, body, increment, valid) in [
        ("index < count", "", "index += 1;", true),
        (
            "index < count",
            "if index == 1 { return index; }",
            "index += 1;",
            true,
        ),
        ("index <= count", "", "index += 1;", false),
        ("index < count", "", "index += 2;", false),
        ("index < count", "", "", false),
        ("index < count", "index = 0;", "index += 1;", false),
        ("index < count", "continue;", "index += 1;", false),
        (
            "index < count",
            "let alias = &mut index;",
            "index += 1;",
            true,
        ),
        // Index advances without wrapping on every back edge, even if count changes.
        ("index < count", "count += 1;", "index += 1;", true),
        (
            "index < count",
            "while index < count { index += 1; }",
            "index += 1;",
            false,
        ),
    ] {
        let caller = format!(
            r#"module app;
            func production_probe(pos limit: usize) -> usize
                executes(total)
            {{
                let mut count: usize = limit;
                let mut index: usize = 0;
                while {guard} {{ {body} {increment} }}
                return index;
            }}"#
        );

        let compilation = production_host_compilation(&caller);

        let checked = compilation
            .execution_properties(source_function_body_key(&compilation, "production_probe"))
            .expect("record scan proof must publish");

        assert_eq!(
            checked
                .value()
                .properties
                .contains(&bray_symbols::ExecutionProperty::Total),
            valid,
            "{caller}: {checked:#?}"
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                checked.diagnostics(),
                DiagnosticKind::CheckingExecutionGuaranteeNotProven,
            );
        }
    }
}
