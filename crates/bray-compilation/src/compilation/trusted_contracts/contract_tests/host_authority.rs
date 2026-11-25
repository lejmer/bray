use bray_diagnostics::{DiagnosticKind};
use crate::test_support::{compilation, source_function_body_key};
use super::fixtures::production_host_compilation;

#[test]
fn implicit_receiver_reborrows_do_not_escape_local_owners() {
    let compilation = crate::test_support::production_standard_library_compilation(
        crate::SelectedTarget::baseline(),
        r#"
        module probe;

        struct Owner
        {
            value: u32;

            func borrowed_value() -> &u32
            {
                return &self.value;
            }
        }

        func escape() -> &u32
        {
            let owner: Owner =
            {
                value = 1,
            };

            return owner.borrowed_value();
        }
        "#,
    );

    let checked = compilation.lowered_unit(source_function_body_key(&compilation, "escape"))
        .expect("receiver escape retains its checked lifetime contract");

    bray_testing::assert_goal_state_diagnostic_kind(
        checked.diagnostics(), DiagnosticKind::CheckingEscapingStorageDependency);
}

#[test]
#[ignore = "tier:proof"]
fn production_owner_transfers_do_not_run_disposal_inside_the_transfer() {
    let compilation = production_host_compilation("module app;");

    for name in ["replace_value", "swap_values"] {
        let checked = compilation
            .execution_properties(source_function_body_key(&compilation, name))
            .expect("production value transfer must publish its execution proof");

        assert!(!checked.diagnostics().has_errors(), "{name}: {checked:#?}");

        assert!(
            checked
                .value()
                .properties
                .contains(&bray_symbols::ExecutionProperty::Total)
        );
    }
}

#[test]
#[ignore = "tier:proof"]
fn production_host_borrow_remains_total() {
    let compilation = production_host_compilation(
        r#"module app;

using core.target;
"#,
    );

    {
        let name = "host_value";

        let checked = compilation
            .execution_properties(source_function_body_key(&compilation, name))
            .expect("production formation step must publish its execution proof");

        assert!(!checked.diagnostics().has_errors(), "{name}: {checked:#?}");

        assert!(
            checked
                .value()
                .properties
                .contains(&bray_symbols::ExecutionProperty::Total)
        );
    }
}

#[test]
#[ignore = "tier:proof"]
fn production_host_partial_lifecycle_cannot_escape_the_complete_operation() {
    for operation in [
        "let prepared = trusted prepare_host_storage();",
        "let retired = trusted take_host_storage();",
        "let retired = trusted retire_storage();",
        "let empty = trusted empty_host_storage();",
        "let complete = complete_host_formation;",
        "let begin = begin_host_formation_locked;",
        "let finish = finish_host_formation_locked;",
        "let begin = begin_host_shutdown_locked;",
        "let finish = finish_host_shutdown_locked;",
    ] {
        let caller = format!(
            r#"
                trusted internal module bray.runtime.bootstrap.host; func production_probe()
                {{
                    {operation}
                }}
            "#
        );

        let compilation = production_host_compilation(&caller);

        let checked = compilation
            .lowered_unit(source_function_body_key(&compilation, "production_probe"))
            .expect("fixture callable must lower successfully");

        bray_testing::assert_goal_state_diagnostic_kind(
            checked.diagnostics(),
            DiagnosticKind::BindingUnresolvedName,
        );
    }
}

#[test]
#[ignore = "tier:proof"]
fn a_live_payload_borrow_preserves_only_its_enclosing_projection() {
    for (body, valid) in [
        ("poke(payload); payload.second = 3;", true),
        ("payload.inner = none; payload.second = 3;", true),
        ("owner.state = none; payload.second = 3;", false),
        (
            r#"
                if let ?inner = payload.inner
                {
                    let view = &mut inner;
                    poke(payload);
                    view.value = 3;
                }
            "#,
            false,
        ),
    ] {
        let caller = format!(
            r#"
            module app;

            struct Inner
            {{
                mut value: u64;
            }}

            struct State
            {{
                mut first: u64;
                mut second: u64;
                mut inner: Inner?;
            }}

            struct Owner
            {{
                mut state: State?;
            }}

            func poke(pos state: &mut State)
            {{
                state.first = 2;
                state.inner = none;
            }}

            func production_probe(pos owner: &mut Owner)
            {{
                if let ?state = owner.state
                {{
                    let payload: &mut State = &mut state;
                    {body}
                }}
            }}
            "#
        );

        let compilation = compilation(&caller);

        let checked = compilation
            .lowered_unit(source_function_body_key(&compilation, "production_probe"))
            .expect("payload projection must retain its checked result");

        assert_eq!(
            !checked.diagnostics().has_errors(),
            valid,
            "{caller}: {checked:#?}"
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                checked.diagnostics(),
                DiagnosticKind::CheckingConflictingBorrow,
            );
        }
    }
}
