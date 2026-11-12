use bray_diagnostics::{DiagnosticKind};
use crate::test_support::{source_function_body_key};
use super::fixtures::{production_host_compilation};

#[test]
#[ignore = "tier:proof"]
fn production_retained_exit_execution_contracts_are_verified() {
    let compilation = production_host_compilation(
        r#"module app;

using core.target;
"#,
    );

    for name in [
        "host_anchor",
        "access_prepared_host",
        "load_attachment_identity",
        "current_attachment_locked",
        "grow",
        "insert_attachment",
        "enter_attachment_locked",
        "take_attachment_exit",
        "publish_attachment_cleanup",
        "store_attachment_cleanup",
        "release_attachment_exit",
    ] {
        let key = source_function_body_key(&compilation, name);

        let checked = compilation
            .execution_properties(key)
            .expect("fixture callable must resolve its execution properties");

        assert!(!checked.diagnostics().has_errors(), "{name}: {checked:#?}");
    }
}

#[test]
#[ignore = "tier:proof"]
fn native_exit_authority_is_required_by_actual_production_operations() {
    for (operation, valid) in [
        (
            "let scope = trusted bray.runtime.bootstrap.thread.destruction.explicit_exit_scope(); _ = trusted bray.runtime.bootstrap.host.finish_attachment(&scope);",
            false,
        ),
        (
            "let scope: bray.runtime.bootstrap.thread.destruction.ExitScope = { identity = 1, native = true }; _ = trusted bray.runtime.bootstrap.host.finish_attachment(&scope);",
            false,
        ),
        (
            "let scope = trusted bray.runtime.bootstrap.thread.destruction.explicit_exit_scope(); let fake: bray.runtime.bootstrap.thread.destruction.ExitScope = { identity = scope.identity, native = true }; _ = trusted bray.runtime.bootstrap.host.finish_attachment(&fake);",
            false,
        ),
        (
            "_ = trusted bray.runtime.bootstrap.thread.destruction.native_exit_scope(std.memory.null<u8>());",
            false,
        ),
        (
            "_ = trusted bray.runtime.bootstrap.host.thread_attachment_leave();",
            false,
        ),
        (
            "_ = trusted bray.runtime.bootstrap.host.thread_attachment_enter();",
            false,
        ),
        (
            "let owner = trusted bray.runtime.bootstrap.host.attachment.acquire();",
            false,
        ),
        (
            "_ = trusted bray.runtime.bootstrap.host.thread_attachment_mark_main();",
            false,
        ),
        (
            "_ = trusted bray.runtime.bootstrap.entry.bridge.install(std.memory.null<bray.runtime.bootstrap.entry.bridge.NativeCallbacks>(), std.memory.null<bray.runtime.bootstrap.entry.bridge.NativeCallbacks>());",
            false,
        ),
        (
            "_ = trusted bray.runtime.bootstrap.entry.bridge.current();",
            false,
        ),
        (
            "let owner: bray.runtime.bootstrap.host.attachment.Owner = { status = 0, identity = 1 };",
            false,
        ),
        (
            "let mut owner = trusted bray.runtime.bootstrap.host.attachment.acquire(); owner.identity = 1;",
            false,
        ),
        (
            "let owner = trusted bray.runtime.bootstrap.host.attachment.acquire(); let fake: bray.runtime.bootstrap.host.attachment.Owner = { status = owner.status, identity = owner.identity };",
            false,
        ),
        (
            "trusted bray.runtime.bootstrap.thread.destruction.thread_storage_exit(std.memory.null<u8>());",
            false,
        ),
        (
            "trusted bray.runtime.bootstrap.thread.destruction.finish_native_attachment(std.memory.null<u8>());",
            false,
        ),
    ] {
        let source = format!(
            r#"module app;

using std.memory;
 using internal bray.runtime.bootstrap.host; using internal bray.runtime.bootstrap.thread.destruction; using internal bray.runtime.bootstrap.host.attachment; using internal bray.runtime.bootstrap.entry.bridge; func production_probe()
 {{
     {operation}
 }}"#
        );

        let compilation = production_host_compilation(&source);

        let checked = compilation
            .lowered_unit(source_function_body_key(&compilation, "production_probe"))
            .expect("fixture callable must lower successfully");

        assert_eq!(
            !checked.diagnostics().has_errors(),
            valid,
            "{operation}: {checked:#?}"
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                checked.diagnostics(),
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
#[ignore = "tier:proof"]
fn production_exit_completion_validates_its_producer_and_total_retirement() {
    let compilation = production_host_compilation("module app;");

    let exit_source = source_function_body_key(&compilation, "thread_attachment_leave")
        .source()
        .syntax()
        .source_id();

    for key in compilation
        .declared_unit_keys_for_test()
        .expect("fixture declarations must resolve their units")
        .into_iter()
        .filter(|key| {
            key.kind() == bray_bound_tree::BoundUnitKind::CallableBody
                && key.source().syntax().source_id() == exit_source
        })
    {
        let checked = compilation
            .lowered_unit(key.clone())
            .expect("fixture callable must lower successfully");

        assert!(!checked.diagnostics().has_errors(), "{key:?}: {checked:#?}");
    }

    for name in [
        "request_attachment_exit",
        "take_attachment_exit",
        "select_attachment_slot",
        "finish_attachment",
    ] {
        let key = source_function_body_key(&compilation, name);

        let checked = compilation
            .lowered_unit(key.clone())
            .expect("production exit completion must retain its checked result");

        assert!(!checked.diagnostics().has_errors(), "{name}: {checked:#?}");
    }
}

#[test]
#[ignore = "tier:proof"]
fn production_lease_destructor_checks_and_remains_total() {
    let compilation = production_host_compilation("module app;");

    let access_source = source_function_body_key(&compilation, "host_access")
        .source()
        .syntax()
        .source_id();

    let key = compilation
        .declared_unit_keys_for_test()
        .expect("fixture declarations must resolve their units")
        .into_iter()
        .find(|key| {
            key.kind() == bray_bound_tree::BoundUnitKind::CallableBody
                && key.declared_owner().kind() == bray_symbols::SymbolKind::Destructor
                && key.source().syntax().source_id() == access_source
        })
        .expect("actual acquisition lease destructor must exist");

    let checked = compilation
        .lowered_unit(key.clone())
        .expect("fixture callable must lower successfully");

    assert!(!checked.diagnostics().has_errors(), "{checked:#?}");

    let proof = compilation
        .execution_properties(key)
        .expect("fixture callable must resolve its execution properties");

    assert!(!proof.diagnostics().has_errors(), "{proof:#?}");

    assert!(
        proof
            .value()
            .properties
            .contains(&bray_symbols::ExecutionProperty::Total)
    );
}

#[test]
#[ignore = "tier:proof"]
fn production_selected_exit_destructor_checks_its_mandatory_cleanup() {
    let compilation = production_host_compilation("module app;");

    let exit_source = source_function_body_key(&compilation, "thread_attachment_leave")
        .source()
        .syntax()
        .source_id();

    let key = compilation
        .declared_unit_keys_for_test()
        .expect("fixture declarations must resolve their units")
        .into_iter()
        .find(|key| {
            key.kind() == bray_bound_tree::BoundUnitKind::CallableBody
                && key.declared_owner().kind() == bray_symbols::SymbolKind::Destructor
                && key.source().syntax().source_id() == exit_source
        })
        .expect("actual selected exit destructor must exist");

    let checked = compilation
        .lowered_unit(key)
        .expect("fixture callable must lower successfully");

    assert!(!checked.diagnostics().has_errors(), "{checked:#?}");
}

#[test]
#[ignore = "tier:proof"]
fn production_retirement_owner_preserves_its_frozen_row_and_lock() {
    for (body, expected) in [
        ("let moved = completed;", None),
        (
            "let mut anchor = trusted host_anchor(&completed.lease); let host = trusted host_value(&completed.lease, &mut anchor);",
            Some(DiagnosticKind::CheckingTrustedObligationNotProven),
        ),
        (
            "completed.identity = completed.identity;",
            Some(DiagnosticKind::CheckingMissingMutationAuthority),
        ),
        (
            "completed.lease = completed.lease;",
            Some(DiagnosticKind::CheckingMissingMutationAuthority),
        ),
    ] {
        let caller = format!(
            r#"
            trusted internal module bray.runtime.bootstrap.host;
            func production_probe(pos mut completed: AttachmentRetirement)
                requires(completed.lease.active, trusted host_access_live(owner = &completed.lease.acquisition), trusted attachment_cleanup_completed(owner = &completed))
            {{
                {body}
            }}
        "#
        );

        let compilation = production_host_compilation(&caller);

        let checked = compilation
            .lowered_unit(source_function_body_key(&compilation, "production_probe"))
            .expect("fixture callable must lower successfully");

        assert_eq!(
            !checked.diagnostics().has_errors(),
            expected.is_none(),
            "{body}: {checked:#?}"
        );

        if let Some(expected) = expected {
            bray_testing::assert_goal_state_diagnostic_kind(checked.diagnostics(), expected);
        }
    }
}

#[test]
#[ignore = "tier:proof"]
fn catastrophic_abort_skips_lease_cleanup_without_continuing() {
    for (operation, cleans_up) in [
        ("trusted core.target.abort();", false),
        ("panic(\"lease invariant failed\");", true),
    ] {
        let caller = format!(
            r#"trusted internal module bray.runtime.bootstrap.host;
            func production_probe()
                uses(intrinsic)
            {{
                let lease = trusted host_access();
                if !lease.active {{ {operation} }}
                let anchor = trusted host_anchor(&lease);
            }}"#
        );

        let compilation = production_host_compilation(&caller);

        let checked = compilation
            .lowered_unit(source_function_body_key(&compilation, "production_probe"))
            .expect("target termination must retain its checked result");

        assert!(
            !checked.diagnostics().has_errors(),
            "{operation}: {checked:#?}"
        );

        let terminal = format!(
            r#"trusted internal module bray.runtime.bootstrap.host;
            func production_probe()
                uses(intrinsic)
            {{
                let lease = trusted host_access();
                {operation}
            }}"#
        );

        let compilation = production_host_compilation(&terminal);
        let key = source_function_body_key(&compilation, "production_probe");

        let checked = compilation
            .async_analysis(key)
            .expect("terminal operation must publish cleanup planning");

        assert!(
            !checked.diagnostics().has_errors(),
            "{operation}: {checked:#?}"
        );

        assert_eq!(
            checked
                .value()
                .scope_exits()
                .iter()
                .any(|exit| exit.has_cleanup()),
            cleans_up,
            "{operation}: {checked:#?}"
        );
    }

    let compilation = crate::test_support::compilation(
        r#"
        module app;

        using core.target;
        func production_probe()
            executes(total)
            uses(intrinsic)
        {
            trusted core.target.abort();
        }
    "#,
    );

    let checked = compilation
        .execution_properties(source_function_body_key(&compilation, "production_probe"))
        .expect("reachable abort must retain its execution result");

    assert!(
        !checked
            .value()
            .properties
            .contains(&bray_symbols::ExecutionProperty::Total)
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        checked.diagnostics(),
        DiagnosticKind::CheckingExecutionGuaranteeNotProven,
    );
}

#[test]
#[ignore = "tier:proof"]
fn production_shutdown_owner_uses_ordinary_completion_rules() {
    for (parameter, condition, body, valid) in [
        ("ShutdownOwner", "true", "", false),
        ("ShutdownOwner", "!owner.pending", "", true),
        (
            "ShutdownOwner",
            "true",
            "_ = trusted owner.attempt();",
            false,
        ),
        (
            "ShutdownOwner",
            "true",
            "_ = trusted owner.adopt_fallback();",
            true,
        ),
        ("ShutdownOwner", "true", "panic(\"caller failed\");", true),
        ("ShutdownOwner?", "true", "", false),
        ("Result<ShutdownOwner, u32>", "true", "", false),
    ] {
        let caller = format!(
            r#"
            trusted internal module bray.runtime.bootstrap.host.product;
            trusted func production_probe(pos mut owner: {parameter})
                requires(
                    trusted native_host(),
                    trusted bray.runtime.bootstrap.pool.protocol_access(),
                    {condition}
                )
            {{
                {body}
            }}
            "#,
        );

        let compilation = production_host_compilation(&caller);

        let checked = compilation
            .lowered_unit(source_function_body_key(&compilation, "production_probe"))
            .expect("fixture callable must lower successfully");

        assert_eq!(
            !checked.diagnostics().has_errors(),
            valid,
            "{parameter}, {condition}, {body}: {checked:#?}"
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                checked.diagnostics(),
                DiagnosticKind::CheckingUnresolvedFinalization,
            );
        }
    }
}

#[test]
#[ignore = "tier:proof"]
fn owned_exit_witnesses_move_into_their_consuming_operation() {
    let compilation = crate::test_support::compilation(
        r#"
        trusted module app;
        struct Owner
        {
            value: u64;
        }
        trusted predicate valid(owner: &Owner);
        extern trusted func owner() -> Owner
            ensures(trusted valid(owner = &result));
        extern trusted func consume_owner(pos owner: Owner)
            requires(trusted valid(owner = &owner));
        func caller()
        {
            let value = trusted owner();
            trusted consume_owner(value);
        }
    "#,
    );

    assert!(
        !compilation.check_diagnostics().has_errors(),
        "{:?}",
        compilation.check_diagnostics()
    );
}

#[test]
#[ignore = "tier:proof"]
fn cleanup_registration_requires_the_prepared_owner_and_exact_slot_lease() {
    for (body, valid) in [
        (
            "return trusted store_attachment_cleanup(lease, selected, owner);",
            true,
        ),
        (
            "return trusted store_attachment_cleanup(other, selected, owner);",
            false,
        ),
        (
            "let copied: AttachmentSelection = { slot = { value = selected.slot.value, storage = selected.slot.storage }, pending = selected.pending, status = selected.status }; return trusted store_attachment_cleanup(lease, &copied, owner);",
            false,
        ),
        (
            "owner.cleanup = none; return trusted store_attachment_cleanup(lease, selected, owner);",
            false,
        ),
        (
            "owner.ready = true; return trusted store_attachment_cleanup(lease, selected, owner);",
            false,
        ),
    ] {
        let caller = format!(
            r#"
            trusted internal module bray.runtime.bootstrap.host;

            using core.memory;
            func production_probe(pos lease: &HostLease, pos other: &HostLease,
                pos selected: &AttachmentSelection, pos mut owner: AttachmentExit) -> u32
                requires(
                    trusted host_access_live(owner = &lease.acquisition), trusted host_access_idle(owner = lease),
                    trusted host_access_live(owner = &other.acquisition), trusted host_access_idle(owner = other),
                    !selected.pending || trusted vacant_attachment_exit(slot = &selected.slot, lease = lease),
                    !selected.pending || trusted core.memory.valid_write<(AttachmentExit?)>(pointer = selected.slot.storage, count = 1),
                    !selected.pending || trusted core.memory.aligned_for<(AttachmentExit?)>(pointer = selected.slot.storage),
                    !owner.ready, trusted attachment_exit_inactive(owner = &owner),
                    trusted prepared_attachment_exit(owner = &owner, slot = &selected.slot, lease = lease)
                )
            {{
                {body}
            }}
        "#
        );

        let compilation = production_host_compilation(&caller);

        let checked = compilation
            .lowered_unit(source_function_body_key(&compilation, "production_probe"))
            .expect("fixture callable must lower successfully");

        assert_eq!(
            !checked.diagnostics().has_errors(),
            valid,
            "{body}: {checked:#?}"
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                checked.diagnostics(),
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}
