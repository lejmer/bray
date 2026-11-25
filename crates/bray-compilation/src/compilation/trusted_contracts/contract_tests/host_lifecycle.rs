use bray_diagnostics::{DiagnosticKind};
use crate::test_support::{source_function_body_key};
use super::fixtures::{production_host_compilation};

#[test]
#[ignore = "tier:proof"]
fn production_retained_exit_execution_contracts_are_verified() {
    let compilation = production_host_compilation("module app;");

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
    super::fixtures::assert_production_probes(&[
        ("probe_explicit_scope", Some(DiagnosticKind::CheckingTrustedObligationNotProven)),
        ("probe_admitted_scope", None),
        ("probe_fabricated_scope", Some(DiagnosticKind::CheckingTrustedObligationNotProven)),
        ("probe_copied_scope", Some(DiagnosticKind::CheckingTrustedObligationNotProven)),
        ("probe_native_scope_without_entry", Some(DiagnosticKind::CheckingTrustedObligationNotProven)),
        ("probe_attachment_leave", Some(DiagnosticKind::CheckingTrustedObligationNotProven)),
        ("probe_attachment_enter", Some(DiagnosticKind::CheckingTrustedObligationNotProven)),
        ("probe_acquire_attachment", Some(DiagnosticKind::CheckingTrustedObligationNotProven)),
        ("probe_mark_main", Some(DiagnosticKind::CheckingTrustedObligationNotProven)),
        ("probe_bridge_install", Some(DiagnosticKind::CheckingTrustedObligationNotProven)),
        ("probe_bridge_current", Some(DiagnosticKind::CheckingTrustedObligationNotProven)),
        ("probe_fabricated_owner", Some(DiagnosticKind::CheckingTrustedObligationNotProven)),
        ("probe_thread_storage_exit", Some(DiagnosticKind::CheckingTrustedObligationNotProven)),
        ("probe_finish_native_attachment", Some(DiagnosticKind::CheckingTrustedObligationNotProven)),
    ], &[
        ("probe_explicit_scope", "let unadmitted"),
        ("probe_copied_scope", "finish_attachment(&fake)"),
        ("probe_acquire_attachment", "attachment.acquire()"),
        ("probe_native_scope_without_entry", "native_exit_scope(std.memory.null"),
        ("probe_attachment_leave", "thread_attachment_leave()"),
        ("probe_attachment_enter", "thread_attachment_enter()"),
        ("probe_mark_main", "thread_attachment_mark_main()"),
        ("probe_bridge_install", "entry.bridge.install("),
        ("probe_bridge_current", "entry.bridge.current()"),
        ("probe_thread_storage_exit", "thread_storage_exit(std.memory.null"),
        ("probe_finish_native_attachment", "finish_native_attachment(std.memory.null"),
    ], super::fixtures::check_probe_storage);
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
    super::fixtures::assert_production_probes(&[
        ("probe_retirement_move", None),
        ("probe_retirement_frozen_host", None),
        ("probe_retirement_storage_loss", Some(DiagnosticKind::CheckingTrustedObligationNotProven)),
        ("probe_retirement_identity_write", Some(DiagnosticKind::CheckingMissingMutationAuthority)),
        ("probe_retirement_lease_write", Some(DiagnosticKind::CheckingMissingMutationAuthority)),
    ], &[], super::fixtures::check_probe_storage);
}

#[test]
#[ignore = "tier:proof"]
fn catastrophic_abort_skips_lease_cleanup_without_continuing() {
    let variants = [
        ("abort", "core.target.abort();", false),
        ("panic", "panic(\"lease invariant failed\");", true),
    ];

    let mut caller = String::from(
        r#"
            trusted internal module bray.runtime.bootstrap.host;
        "#,
    );

    for (name, operation, _) in variants {
        let (modifier, capabilities) = if name == "abort" {
            ("trusted ", "uses(intrinsic)")
        } else {
            ("", "")
        };

        caller.push_str(&format!(
            r#"
            {modifier}func guarded_{name}() {capabilities}
            {{
                let lease = trusted host_access();

                if !lease.active
                {{
                    {operation}
                }}

                let anchor = trusted host_anchor(&lease);
            }}

            {modifier}func terminal_{name}() {capabilities}
            {{
                let lease = trusted host_access();

                {operation}
            }}
            "#
        ));
    }

    let compilation = production_host_compilation(&caller);

    for (name, operation, cleans_up) in variants {
        let checked = compilation
            .lowered_unit(source_function_body_key(&compilation, &format!("guarded_{name}")))
            .expect("target termination must retain its checked result");

        assert!(
            !checked.diagnostics().has_errors(),
            "{operation}: {checked:#?}"
        );

        let key = source_function_body_key(&compilation, &format!("terminal_{name}"));

        let checked = compilation
            .storage_flow(key)
            .expect("terminal operation must publish reached cleanup");

        assert!(
            !checked.diagnostics().has_errors(),
            "{operation}: {checked:#?}"
        );

        assert_eq!(
            checked
                .value()
                .exits()
                .iter()
                .any(|exit| exit.has_cleanup()),
            cleans_up,
            "{operation}: {checked:#?}"
        );
    }

    let compilation = crate::test_support::compilation(
        r#"
        trusted module app;

        using core.target;
        trusted func production_probe()
            executes(total)
            uses(intrinsic)
        {
            core.target.abort();
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
    super::fixtures::assert_production_probes(&[
        ("probe_shutdown_pending", Some(DiagnosticKind::CheckingUnresolvedFinalization)),
        ("probe_shutdown_completed", None),
        ("probe_shutdown_attempt", Some(DiagnosticKind::CheckingUnresolvedFinalization)),
        ("probe_shutdown_fallback", None),
        ("probe_shutdown_panic", None),
        ("probe_shutdown_optional", Some(DiagnosticKind::CheckingUnresolvedFinalization)),
        ("probe_shutdown_result", Some(DiagnosticKind::CheckingUnresolvedFinalization)),
    ], &[], |compilation, key| {
        let checked = compilation
            .completed_unit_cleanup(&key, &crate::CancellationToken::new())
            .expect("production shutdown probe reaches cleanup completion");

        checked.result().diagnostics().clone()
    });
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
            let value = owner();
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
            "return store_attachment_cleanup(lease, selected, owner);",
            true,
        ),
        (
            "return store_attachment_cleanup(other, selected, owner);",
            false,
        ),
        (
            r"let copied: AttachmentSelection =
                {
                    slot =
                    {
                        value = selected.slot.value,
                        storage = selected.slot.storage
                    },
                    pending = selected.pending,
                    status = selected.status
                };
                return store_attachment_cleanup(lease, &copied, owner);",
            false,
        ),
        (
            "owner.cleanup = none; return store_attachment_cleanup(lease, selected, owner);",
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
                    trusted host_access_live(owner = &lease.acquisition),
                    trusted host_access_live(owner = &other.acquisition),
                    !selected.pending || trusted vacant_attachment_exit(slot = &selected.slot, lease = lease),
                    !selected.pending || trusted core.memory.valid_write<(AttachmentExit?)>(pointer = selected.slot.storage, count = 1),
                    !selected.pending || trusted core.memory.aligned_for<(AttachmentExit?)>(pointer = selected.slot.storage),
                    trusted attachment_exit_inactive(owner = &owner),
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
