use bray_diagnostics::{DiagnosticKind};
use crate::test_support::{source_function_body_key};
use super::fixtures::{body_belongs_to, production_sync_compilation};

#[test]
#[ignore = "tier:proof"]
fn production_once_declarations_check_their_own_bodies() {
    let application = production_sync_compilation("module app;");
    let mut failures = Vec::new();

    for key in application
        .declared_unit_keys_for_test()
        .expect("fixture declarations must resolve their units")
    {
        if key.kind() != bray_bound_tree::BoundUnitKind::CallableBody
            || !body_belongs_to(
                &application,
                &key,
                &[&["std", "sync", "once"]],
                &["Once", "Lazy"],
            )
        {
            continue;
        }

        let certified = application
            .execution_properties(key.clone())
            .expect("fixture callable must resolve its execution properties");

        if certified.diagnostics().has_errors() {
            failures.push((
                key.source().syntax().source_id().raw(),
                key.source().syntax().full_range(),
                format!("{:?}", certified.diagnostics()),
            ));
        }

        let checked = application
            .lowered_unit(key.clone())
            .expect("fixture callable must lower successfully");

        if checked.diagnostics().has_errors() {
            failures.push((
                key.source().syntax().source_id().raw(),
                key.source().syntax().full_range(),
                format!("{:?}", checked.diagnostics()),
            ));
        }
    }

    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
#[ignore = "tier:proof"]
fn production_once_and_lazy_owners_keep_cleanup_authority() {
    for body in [
        "let once = std.sync.Once<u32>.empty(); let value = once.get_or_init(callback);",
        "let lazy = std.sync.Lazy<u32>(callback); let value = lazy.get();",
    ] {
        let source = format!(
            r#"
        module app;

        using std.sync;
        func production_probe(pos callback: func() -> u32) requires(blocking_execution()) {{
            {body}
        }}
    "#
        );

        let application = production_sync_compilation(&source);

        let checked = application
            .lowered_unit(source_function_body_key(&application, "production_probe"))
            .expect("fixture callable must lower successfully");

        assert!(
            !checked.diagnostics().has_errors(),
            "{body}: {:?}",
            checked.diagnostics()
        );
    }
}

#[test]
#[ignore = "tier:proof"]
fn production_once_rejects_fabricated_instance_and_attempt_authority() {
    for body in [
        "let once: std.sync.Once<u32> = { prepared = { instance = std.sync.once.Marker {}, storage = std.memory.uninit<std.sync.once.State<u32>>() } }; let value = once.get_or_init(callback);",
        "let once = std.sync.Once<u32>.empty(); let attempt: std.sync.once.Attempt<u32> = { once = &once, thread = &std.sync.once.CURRENT, acquisition = std.sync.once.Marker {} }; let value = attempt.publish(7);",
    ] {
        let source = format!(
            r#"
            module app;

            using std.memory;
            using std.sync;
            func production_probe(pos callback: func() -> u32) requires(blocking_execution()) {{
                {body}
            }}
        "#
        );

        let application = production_sync_compilation(&source);

        let checked = application
            .lowered_unit(source_function_body_key(&application, "production_probe"))
            .expect("fixture callable must lower successfully");

        bray_testing::assert_goal_state_diagnostic_kind(
            checked.diagnostics(),
            DiagnosticKind::CheckingTrustedObligationNotProven,
        );
    }
}

#[test]
#[ignore = "tier:proof"]
fn static_once_keeps_initializer_authority() {
    let application = production_sync_compilation(
        r#"
        module app;

        using std.sync;
        static CELL: std.sync.Once<u32> = { prepared = std.sync.Once<u32>.prepare() };
        func caller(pos callback: func() -> u32) requires(blocking_execution()) {
            let value = CELL.get_or_init(callback);
        }
    "#,
    );

    let checked = application
        .lowered_unit(source_function_body_key(&application, "caller"))
        .expect("fixture callable must lower successfully");

    assert!(
        !checked.diagnostics().has_errors(),
        "{:?}",
        checked.diagnostics()
    );
}

#[test]
#[ignore = "tier:proof"]
fn production_constant_owner_factories_preserve_pairing_and_reject_fabrication() {
    for (owner, preparation, state, use_owner) in [
        (
            "SpinLock",
            "std.sync.SpinLock<u32>.prepare(7)",
            "spin_lock",
            "_ = moved.lock();",
        ),
        (
            "Once",
            "std.sync.Once<u32>.prepare()",
            "once",
            "_ = moved.get_or_init(callback);",
        ),
    ] {
        let replacement =
            format!("owner.prepared.storage = std.memory.uninit<std.sync.{state}.State<u32>>();");

        let fabricated = format!(
            "let owner: std.sync.{owner}<u32> = {{ prepared = {{ instance = std.sync.{state}.Marker {{}}, storage = std.memory.uninit<std.sync.{state}.State<u32>>() }} }};"
        );

        let reconstructed = format!(
            "let original = {preparation}; let owner: std.sync.{owner}<u32> = {{ prepared = {{ instance = original.instance, storage = std.memory.uninit<std.sync.{state}.State<u32>>() }} }};"
        );

        for (change, expected) in [
            (String::new(), None),
            (
                replacement,
                Some(DiagnosticKind::CheckingMissingMutationAuthority),
            ),
            (
                fabricated,
                Some(DiagnosticKind::CheckingTrustedObligationNotProven),
            ),
            (
                reconstructed,
                Some(DiagnosticKind::CheckingTrustedWitnessTransferNotProven),
            ),
        ] {
            let source = format!(
                r#"
                module app;

                using std.memory;
                using internal std.sync;

                func production_probe(pos callback: func() -> u32) requires(blocking_execution()) {{
                    let prepared = {preparation};
                    let transported = prepared;
                    let mut owner: std.sync.{owner}<u32> = {{ prepared = transported }};
                    {change}
                    let moved = owner;
                    {use_owner}
                }}
                "#
            );

            let application = production_sync_compilation(&source);

            let checked = application
                .lowered_unit(source_function_body_key(&application, "production_probe"))
                .expect("fixture callable must lower successfully");

            match expected {
                Some(kind) => {
                    bray_testing::assert_goal_state_diagnostic_kind(checked.diagnostics(), kind)
                }
                None => assert!(
                    !checked.diagnostics().has_errors(),
                    "{source}: {:?}",
                    checked.diagnostics()
                ),
            }
        }
    }
}
