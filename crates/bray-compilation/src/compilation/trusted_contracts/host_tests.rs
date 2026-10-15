use bray_diagnostics::DiagnosticKind;
use bray_symbols::PackageIdentity;

use crate::test_support::{source_function_body_key, source_input};
use crate::{Compilation, CompilationRequest};

#[test]
fn production_host_nested_bodies_and_contracts_are_checked() {
    let compilation = production_host_compilation("module app;");

    let owners = [
        "initialize_host",
        "shutdown_host",
        "structured_shutdown",
        "lock",
        "release",
    ]
    .map(|name| {
        source_function_body_key(&compilation, name)
            .source()
            .syntax()
    });

    let diagnostics = compilation.check_diagnostics();

    let failures = diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic.primary_span().is_some_and(|span| {
                owners.iter().any(|owner| {
                    span.source_id() == owner.source_id()
                        && span.start() >= owner.full_range().start()
                        && span.end() <= owner.full_range().end()
                })
            })
        })
        .collect::<Vec<_>>();

    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn production_host_lease_cannot_claim_preservation_across_fallible_preparation() {
    let compilation = production_host_compilation(
        r#"
        trusted internal module bray.runtime.bootstrap.host;
        func production_probe() -> u32
        {
            let lease = trusted host_access();
            let mut anchor = trusted host_anchor(&lease);

            if !lease.active
            {
                return 4;
            }

            let status = trusted initialize_host();
            let pointer = trusted host_access_pointer(&lease);

            return status;
        }
        "#,
    );

    let checked = compilation
        .lowered_unit(source_function_body_key(&compilation, "production_probe"))
        .unwrap();

    bray_testing::assert_goal_state_diagnostic_kind(
        checked.diagnostics(),
        DiagnosticKind::CheckingTrustedObligationNotProven,
    );
}

#[test]
fn production_retained_exit_execution_contracts_are_verified() {
    let compilation = production_host_compilation("module app;");

    for name in [
        "host_anchor",
        "access_prepared_host",
        "load_attachment_identity",
        "current_attachment_locked",
        "insert_attachment",
        "enter_attachment_locked",
        "take_attachment_exit",
        "publish_attachment_cleanup",
        "store_attachment_cleanup",
        "release_attachment_exit",
    ] {
        let key = source_function_body_key(&compilation, name);
        let checked = compilation.execution_properties(key).unwrap();

        assert!(!checked.diagnostics().has_errors(), "{name}: {checked:#?}");
    }
}

#[test]
fn production_host_statics_require_mutation_authority() {
    for operation in [
        "core.atomic.store<u64, 2>(&bray.runtime.bootstrap.host.SYNCHRONIZATION, 0);",
        "let storage = &bray.runtime.bootstrap.host.SYNCHRONIZATION; core.atomic.store<u64, 2>(&storage.lock, 0);",
        "let storage = &bray.runtime.bootstrap.host.SYNCHRONIZATION; core.atomic.store<u64, 2>(&storage.key, 0);",
        "let storage = &bray.runtime.bootstrap.host.SYNCHRONIZATION; core.atomic.store<u64, 2>(&storage.main, 0);",
    ] {
        let caller = format!(
            "module app; using internal bray.runtime.bootstrap.host; func production_probe() {{ {operation} }}"
        );

        let compilation = production_host_compilation(&caller);

        let key = source_function_body_key(&compilation, "production_probe");
        let source_id = key.source().syntax().source_id();

        let diagnostics = compilation
            .check_diagnostics()
            .iter()
            .filter(|diagnostic| {
                diagnostic
                    .primary_span()
                    .is_some_and(|span| span.source_id() == source_id)
            })
            .collect::<Vec<_>>();

        assert!(
            !diagnostics.is_empty(),
            "safe internal access must not mutate protected storage: {operation}: {:?}",
            diagnostics
        );
    }
}

#[test]
fn production_raw_synchronization_pointer_does_not_grant_safe_atomic_access() {
    for memory in ["core.memory", "std.memory"] {
        let source = r#"
        module app;
        using internal bray.runtime.bootstrap.host;

        func production_probe()
        {
            let pointer = trusted bray.runtime.bootstrap.host.synchronization_pointer();
            let borrowed = trusted MEMORY.borrow_from<bray.runtime.bootstrap.host.Synchronization, Uninit<bray.runtime.bootstrap.host.Synchronization>>(
                &bray.runtime.bootstrap.host.SYNCHRONIZATION, pointer
            );

            core.atomic.store<u64, 2>(&borrowed.lock, 0);
        }
    "#.replace("MEMORY", memory);

        let compilation = production_host_compilation(&source);

        let source_id = source_function_body_key(&compilation, "production_probe")
            .source()
            .syntax()
            .source_id();

        let diagnostics = compilation
            .check_diagnostics()
            .iter()
            .filter(|diagnostic| {
                diagnostic
                    .primary_span()
                    .is_some_and(|span| span.source_id() == source_id)
            })
            .collect::<Vec<_>>();

        assert!(
            diagnostics.iter().any(|diagnostic| diagnostic.kind()
                == DiagnosticKind::CheckingTrustedCapabilityRequiresTrustedCallable),
            "{diagnostics:?}"
        );
    }
}

#[test]
fn native_exit_authority_is_required_by_actual_production_operations() {
    for (operation, valid) in [
        (
            "let scope = trusted bray.runtime.bootstrap.thread.destruction.explicit_exit_scope(); let _ = trusted bray.runtime.bootstrap.host.finish_attachment(&scope);",
            true,
        ),
        (
            "let scope: bray.runtime.bootstrap.thread.destruction.ExitScope = { identity = 1, native = true }; let _ = trusted bray.runtime.bootstrap.host.finish_attachment(&scope);",
            false,
        ),
        (
            "let scope = trusted bray.runtime.bootstrap.thread.destruction.explicit_exit_scope(); let fake: bray.runtime.bootstrap.thread.destruction.ExitScope = { identity = scope.identity, native = true }; let _ = trusted bray.runtime.bootstrap.host.finish_attachment(&fake);",
            false,
        ),
        (
            "let _ = trusted bray.runtime.bootstrap.thread.destruction.native_exit_scope(std.memory.null<u8>());",
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
            "module app; using internal bray.runtime.bootstrap.host; using internal bray.runtime.bootstrap.thread.destruction; func production_probe() {{ {operation} }}"
        );

        let compilation = production_host_compilation(&source);

        let checked = compilation
            .lowered_unit(source_function_body_key(&compilation, "production_probe"))
            .unwrap();

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
fn production_host_borrow_remains_total() {
    let compilation = production_host_compilation("module app;");

    for name in ["host_value"] {
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
fn production_attachment_growth_only_transfers_prepared_backing_under_the_lease() {
    let compilation = production_host_compilation("module app;");
    let key = source_function_body_key(&compilation, "insert_attachment");

    let checked = compilation
        .lowered_unit(key.clone())
        .expect("production attachment growth must lower");

    assert!(!checked.diagnostics().has_errors(), "{checked:#?}");

    let checked = compilation
        .execution_properties(key)
        .expect("production attachment growth must certify its execution guarantee");

    assert!(!checked.diagnostics().has_errors(), "{checked:#?}");

    assert!(
        checked
            .value()
            .properties
            .contains(&bray_symbols::ExecutionProperty::Total)
    );
}

#[test]
fn bootstrap_module_dependencies_do_not_form_export_cycles() {
    let compilation = crate::test_support::compilation(
        "internal module root; using internal root.host;\n\
         internal module root.host { using internal root; using internal root.thread.destruction; }\n\
         internal module root.thread.destruction { using internal root.host; }",
    );

    let diagnostics = compilation.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:#?}");
}

#[test]
fn opaque_attachment_ordinals_complete_without_becoming_pure_observations() {
    for (parameter, result, body) in [
        (
            "value: usize",
            "RawPointer<u8>",
            "trusted core.target.from_exposed_address<u8>(address = value)",
        ),
        (
            "value: RawPointer<u8>",
            "usize",
            "trusted core.target.expose_address<u8>(value)",
        ),
    ] {
        for (property, expected) in [("total", true), ("pure", false)] {
            let source = format!(
                "module app; func production_probe(pos {parameter}) -> {result} executes({property}) uses(intrinsic) {{ return {body}; }}"
            );

            let compilation = crate::test_support::compilation(&source);

            let checked = compilation
                .execution_properties(source_function_body_key(&compilation, "production_probe"))
                .expect("selected ordinal conversion must publish its execution result");

            assert_eq!(
                !checked.diagnostics().has_errors(),
                expected,
                "{source}: {checked:#?}"
            );
        }
    }
}

#[test]
fn callback_reconstruction_requires_the_actual_callable_address_evidence() {
    for (requirement, valid) in [
        (
            "requires(trusted core.memory.callable_address_valid<Callback>(pointer = pointer))",
            true,
        ),
        ("", false),
    ] {
        let source = format!(
            "trusted internal module bray.runtime.bootstrap.host; callable Callback = @abi(c) func() -> u32; func production_probe(pos pointer: RawPointer<Callback>) -> Callback {requirement} executes(pure, total) uses(layout_reinterpret) {{ return trusted std.memory.callable_from_pointer<Callback>(pointer); }}"
        );

        let compilation = production_host_compilation(&source);
        let key = source_function_body_key(&compilation, "production_probe");

        let checked = compilation
            .lowered_unit(key.clone())
            .expect("callback reconstruction must retain its checked result");

        assert_eq!(
            !checked.diagnostics().has_errors(),
            valid,
            "{requirement}: {checked:#?}"
        );

        if valid {
            let properties = compilation
                .execution_properties(key)
                .expect("valid callback projection must certify its guarantees");

            assert!(!properties.diagnostics().has_errors(), "{properties:#?}");
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                checked.diagnostics(),
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn production_host_authority_cannot_be_constructed_from_fields() {
    for (body, valid) in [
        ("let pointer = trusted host_access_pointer(&lease);", true),
        (
            "let moved = lease; let pointer = trusted host_access_pointer(&moved);",
            true,
        ),
        (
            "let fake: HostLease = { acquisition = lease.acquisition, synchronization = lease.synchronization, owner = lease.owner, active = true, retirement = lease.retirement }; let pointer = trusted host_access_pointer(&fake);",
            false,
        ),
        (
            "let fake: HostLease = { acquisition = lease.acquisition, synchronization = lease.synchronization, owner = lease.owner, active = lease.active, retirement = lease.retirement };",
            false,
        ),
        (
            "let mut fake: HostAnchor = { owner = anchor.owner }; let value = trusted host_value(&lease, &mut fake);",
            false,
        ),
        (
            "let other_lease = trusted host_access(); let mut other = trusted host_anchor(&other_lease); let value = trusted host_value(&lease, &mut other);",
            false,
        ),
        (
            "let host = trusted host_value(&lease, &mut anchor); host.next_identity = 0;",
            false,
        ),
        (
            "let host = trusted host_value(&lease, &mut anchor); host.transitioning = true;",
            false,
        ),
        (
            "let host = trusted host_value(&lease, &mut anchor); host.storage = none;",
            false,
        ),
        (
            "let host = trusted host_value(&lease, &mut anchor); if let ?storage = host.storage { storage.released = true; }",
            false,
        ),
    ] {
        let caller = format!(
            r#"trusted internal module bray.runtime.bootstrap.host;
            func production_probe() {{
                let lease = trusted host_access();
                let mut anchor = trusted host_anchor(&lease);
                if !lease.active {{ return; }}
                {body}
            }}"#,
        );

        let compilation = production_host_compilation(&caller);

        let checked = compilation
            .lowered_unit(source_function_body_key(&compilation, "production_probe"))
            .expect("production host probe must retain its checked result");

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

#[test]
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
            "trusted internal module bray.runtime.bootstrap.host; func production_probe() {{ {operation} }}"
        );

        let compilation = production_host_compilation(&caller);

        let checked = compilation
            .lowered_unit(source_function_body_key(&compilation, "production_probe"))
            .unwrap();

        bray_testing::assert_goal_state_diagnostic_kind(
            checked.diagnostics(),
            DiagnosticKind::BindingUnresolvedName,
        );
    }
}

#[test]
fn production_attachment_scan_keeps_its_returned_mutable_borrow_live() {
    let compilation = production_host_compilation("module app;");
    let key = source_function_body_key(&compilation, "find_attachment");

    let checked = compilation
        .lowered_unit(key.clone())
        .expect("production record scan must lower");

    assert!(!checked.diagnostics().has_errors(), "{checked:#?}");

    let properties = compilation
        .execution_properties(key)
        .expect("production record scan must certify its total guarantee");

    assert!(
        properties
            .value()
            .properties
            .contains(&bray_symbols::ExecutionProperty::Total),
        "{properties:#?}"
    );

    assert!(!properties.diagnostics().has_errors(), "{properties:#?}");
}

#[test]
fn production_exit_completion_validates_its_producer_and_total_retirement() {
    let compilation = production_host_compilation("module app;");

    for key in compilation
        .declared_unit_keys_for_test()
        .unwrap()
        .into_iter()
        .filter(|key| {
            key.kind() == bray_bound_tree::BoundUnitKind::CallableBody
                && key.source().syntax().source_id() == bray_source::SourceId::new(8)
        })
    {
        let checked = compilation.lowered_unit(key.clone()).unwrap();

        assert!(!checked.diagnostics().has_errors(), "{key:?}: {checked:#?}");
    }

    for name in [
        "request_attachment_exit",
        "take_attachment_exit",
        "select_attachment_slot",
        "finish_attachment",
        "run_attachment_cleanup",
        "complete_attachment_exit_locked",
    ] {
        let key = source_function_body_key(&compilation, name);

        let checked = compilation
            .lowered_unit(key.clone())
            .expect("production exit completion must retain its checked result");

        assert!(!checked.diagnostics().has_errors(), "{name}: {checked:#?}");
    }

    let checked = compilation
        .execution_properties(source_function_body_key(
            &compilation,
            "complete_attachment_exit_locked",
        ))
        .expect("production retirement must retain its execution proof");

    assert!(!checked.diagnostics().has_errors(), "{checked:#?}");

    assert!(
        checked
            .value()
            .properties
            .contains(&bray_symbols::ExecutionProperty::Total)
    );
}

#[test]
fn production_lease_destructor_checks_and_remains_total() {
    let compilation = production_host_compilation("module app;");

    let key = compilation
        .declared_unit_keys_for_test()
        .unwrap()
        .into_iter()
        .find(|key| {
            key.kind() == bray_bound_tree::BoundUnitKind::CallableBody
                && key.declared_owner().kind() == bray_symbols::SymbolKind::Destructor
                && key.source().syntax().source_id() == bray_source::SourceId::new(5)
        })
        .expect("actual acquisition lease destructor must exist");

    let checked = compilation.lowered_unit(key.clone()).unwrap();

    assert!(!checked.diagnostics().has_errors(), "{checked:#?}");

    let proof = compilation.execution_properties(key).unwrap();

    assert!(!proof.diagnostics().has_errors(), "{proof:#?}");

    assert!(
        proof
            .value()
            .properties
            .contains(&bray_symbols::ExecutionProperty::Total)
    );
}

#[test]
fn production_selected_exit_destructor_checks_its_mandatory_cleanup() {
    let compilation = production_host_compilation("module app;");

    let key = compilation
        .declared_unit_keys_for_test()
        .unwrap()
        .into_iter()
        .find(|key| {
            key.kind() == bray_bound_tree::BoundUnitKind::CallableBody
                && key.declared_owner().kind() == bray_symbols::SymbolKind::Destructor
                && key.source().syntax().source_id() == bray_source::SourceId::new(8)
        })
        .expect("actual selected exit destructor must exist");

    let checked = compilation.lowered_unit(key).unwrap();

    assert!(!checked.diagnostics().has_errors(), "{checked:#?}");
}

#[test]
fn production_retirement_lease_cannot_mutate_or_relocate_its_owned_row() {
    for (body, valid) in [
        ("let moved = completed;", true),
        (
            "let mut anchor = trusted host_anchor(&completed); let host = trusted host_value(&completed, &mut anchor);",
            false,
        ),
        ("completed.retirement = completed.retirement;", false),
        ("completed.retirement.pending = false;", false),
    ] {
        let caller = format!(
            r#"
            trusted internal module bray.runtime.bootstrap.host;
            func production_probe(pos mut completed: HostLease)
                requires(completed.active, completed.retirement.pending, trusted host_access_live(owner = &completed.acquisition), trusted attachment_cleanup_completed(owner = &completed))
            {{
                {body}
            }}
        "#
        );

        let compilation = production_host_compilation(&caller);

        let checked = compilation
            .lowered_unit(source_function_body_key(&compilation, "production_probe"))
            .unwrap();

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

#[test]
fn catastrophic_abort_skips_lease_cleanup_without_continuing() {
    for (operation, cleans_up) in [
        ("trusted core.target.abort();", false),
        ("panic(\"lease invariant failed\");", true),
    ] {
        let caller = format!(
            r#"trusted internal module bray.runtime.bootstrap.host;
            func production_probe() uses(intrinsic) {{
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
            func production_probe() uses(intrinsic) {{ let lease = trusted host_access(); {operation} }}"#
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
        func production_probe() executes(total) uses(intrinsic) { trusted core.target.abort(); }
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
fn owned_exit_witnesses_move_into_their_consuming_operation() {
    let compilation = crate::test_support::compilation(
        r#"
        trusted module app;
        struct Owner { value: u64; }
        trusted predicate valid(owner: &Owner);
        extern trusted func owner() -> Owner ensures(trusted valid(owner = &result));
        extern trusted func consume_owner(pos owner: Owner) requires(trusted valid(owner = &owner));
        func caller() { let value = trusted owner(); trusted consume_owner(value); }
    "#,
    );

    assert!(
        !compilation.check_diagnostics().has_errors(),
        "{:?}",
        compilation.check_diagnostics()
    );
}

#[test]
fn execution_domains_use_live_witnesses_without_accepting_fabricated_or_stale_ones() {
    for (body, valid) in [
        (
            "let owner = trusted produce(1); return trusted observe(&owner);",
            true,
        ),
        (
            "let owner = trusted produce(2); return trusted observe(&owner);",
            false,
        ),
        (
            "let owner: Owner = { value = 1 }; return trusted observe(&owner);",
            false,
        ),
        (
            "let mut owner = trusted produce(1); change(&mut owner); return trusted observe(&owner);",
            false,
        ),
    ] {
        let source = format!(
            r#"
            trusted module app;
            struct Owner {{ mut value: u64; }}
            trusted predicate live(owner: &Owner);
            trusted func produce(pos value: u64) -> Owner
                ensures(result.value == value, trusted live(owner = &result)) executes(total)
            {{ return {{ value = value }}; }}
            trusted func observe(pos owner: &Owner) -> u64
                requires(trusted live(owner = owner), owner.value == 1) executes(total)
            {{ return owner.value; }}
            func change(pos owner: &mut Owner) executes(total) {{ owner.value = 2; }}
            func production_probe() -> u64 executes(total) {{ {body} }}
        "#
        );

        let compilation = crate::test_support::compilation(&source);

        let checked = compilation
            .execution_properties(source_function_body_key(&compilation, "production_probe"))
            .expect("live execution domain evidence must be checked");

        assert_eq!(
            !checked.diagnostics().has_errors(),
            valid,
            "{body}: {checked:#?}"
        );
    }
}

#[test]
fn completion_witnesses_require_proven_preservation_across_calls() {
    for (body, valid) in [
        ("", true),
        ("let incidents = completed.incidents;", true),
        ("let value = observe();", true),
        ("let value = acquire();", false),
        (
            "let incidents = completed.incidents; let value = acquire();",
            false,
        ),
    ] {
        let source = r#"
        trusted module app;
        struct Owner { value: u64; }
        struct Completed { selected: Owner; incidents: u32; }
        trusted predicate live(owner: &Owner);
        trusted predicate done(owner: &Completed);
        extern trusted func owner() -> Owner ensures(trusted live(owner = &result));
        extern func acquire() -> u64;
        func observe() -> u64 executes(pure, total) { return 0; }
        trusted func complete(pos selected: Owner) -> Completed
            requires(trusted live(owner = &selected))
            ensures(trusted done(owner = &result))
        { return { selected = selected, incidents = 0 }; }
        extern trusted func consume_completion(pos completed: Completed) requires(trusted done(owner = &completed));
        func caller() {
            let selected = trusted owner();
            let completed = trusted complete(selected);
            BODY
            trusted consume_completion(completed);
        }
    "#.replace("BODY", body);

        let compilation = crate::test_support::compilation(&source);

        assert_eq!(
            !compilation.check_diagnostics().has_errors(),
            valid,
            "{body}: {:?}",
            compilation.check_diagnostics()
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                compilation.check_diagnostics(),
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn closing_epoch_authority_cannot_be_copied_from_internal_fields() {
    for (body, valid) in [
        ("", true),
        ("let moved = selected;", true),
        ("selected.ready = false;", false),
        ("selected.cleanup = none;", false),
        (
            "let fake: AttachmentExit = { identity = selected.identity, native_thread = selected.native_thread, cleanup = none, ready = selected.ready, incidents = none };",
            false,
        ),
        (
            "let own_lease = trusted host_access(); let fake: HostLease = { acquisition = own_lease.acquisition, synchronization = own_lease.synchronization, owner = own_lease.owner, active = true, retirement = AttachmentRetirement { pending = true, identity = selected.identity, native_thread = selected.native_thread, record = std.memory.null<Attachment>(), active = std.memory.null<usize>(), remaining_active = 0 } }; let pointer = trusted completed_attachment_pointer(&fake);",
            false,
        ),
    ] {
        let caller = format!(
            r#"trusted internal module bray.runtime.bootstrap.host;
            func production_probe(pos mut selected: AttachmentExit, pos lease: &HostLease, pos anchor: &mut HostAnchor, pos main: &core.atomic.Atomic<u64>)
                requires(
                    selected.ready,
                    trusted attachment_exit_selected(owner = &selected),
                    lease.active,
                    trusted host_access_live(owner = &lease.acquisition),
                    trusted host_anchor_matches(anchor = &anchor, lease = lease)
                )
            {{ {body} }}"#
        );

        let compilation = production_host_compilation(&caller);

        let checked = compilation
            .lowered_unit(source_function_body_key(&compilation, "production_probe"))
            .expect("closing epoch probe must retain its checked result");

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

#[test]
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
            func production_probe(pos lease: &HostLease, pos other: &HostLease,
                pos selected: &AttachmentSelection, pos mut owner: AttachmentExit) -> u32
                requires(
                    trusted host_access_live(owner = &lease.acquisition), trusted host_access_idle(owner = lease), trusted attachment_cleanup_completed(owner = lease),
                    trusted host_access_live(owner = &other.acquisition), trusted host_access_idle(owner = other), trusted attachment_cleanup_completed(owner = other),
                    !selected.pending || trusted vacant_attachment_exit(slot = &selected.slot, lease = lease),
                    !selected.pending || trusted core.memory.valid_write<(AttachmentExit?)>(pointer = selected.slot.storage, count = 1),
                    !selected.pending || trusted core.memory.aligned_for<(AttachmentExit?)>(pointer = selected.slot.storage),
                    !owner.ready, trusted attachment_exit_inactive(owner = &owner),
                    trusted prepared_attachment_exit(owner = &owner, slot = &selected.slot, lease = lease)
                )
            {{ {body} }}
        "#
        );

        let compilation = production_host_compilation(&caller);

        let checked = compilation
            .lowered_unit(source_function_body_key(&compilation, "production_probe"))
            .unwrap();

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

#[test]
fn inactive_exit_authority_requires_the_unchanged_whole_owner() {
    for (body, valid) in [
        ("", true),
        ("let moved = selected;", true),
        ("selected.ready = true;", false),
        ("selected.cleanup = none;", false),
        (
            "let fake: AttachmentExit = { identity = selected.identity, native_thread = selected.native_thread, cleanup = none, ready = false, incidents = none };",
            false,
        ),
    ] {
        let caller = format!(
            r#"
            trusted internal module bray.runtime.bootstrap.host;
            func production_probe(pos mut selected: AttachmentExit)
                requires(!selected.ready, trusted attachment_exit_inactive(owner = &selected))
            {{ {body} }}
        "#
        );

        let compilation = production_host_compilation(&caller);

        let checked = compilation
            .lowered_unit(source_function_body_key(&compilation, "production_probe"))
            .unwrap();

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

#[test]
fn a_live_payload_borrow_preserves_only_its_enclosing_projection() {
    for (body, valid) in [
        ("poke(payload); payload.second = 3;", true),
        ("payload.inner = none; payload.second = 3;", true),
        ("owner.state = none; payload.second = 3;", false),
        (
            "if let ?inner = payload.inner { let view = &mut inner; poke(payload); view.value = 3; }",
            false,
        ),
    ] {
        let caller = format!(
            r#"module app;
            struct Inner {{ mut value: u64; }}
            struct State {{ mut first: u64; mut second: u64; mut inner: Inner?; }}
            struct Owner {{ mut state: State?; }}
            func poke(pos state: &mut State) {{ state.first = 2; state.inner = none; }}
            func production_probe(pos owner: &mut Owner) {{
                if let ?state = owner.state {{ let payload: &mut State = &mut state; {body} }}
            }}"#
        );

        let compilation = production_host_compilation(&caller);

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

#[test]
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
        let caller = format!(
            r#"module app;
            func production_probe(pos mut values: &[u64], pos other: &[u64], pos mut index: usize) -> u64 executes(total) {{ {body} }}"#
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
            false,
        ),
        ("index < count", "count += 1;", "index += 1;", false),
        (
            "index < count",
            "while index < count { index += 1; }",
            "index += 1;",
            false,
        ),
    ] {
        let caller = format!(
            r#"module app;
            func production_probe(pos limit: usize) -> usize executes(total) {{
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

pub(in crate::compilation) fn production_host_compilation(caller: &str) -> Compilation {
    production_host_compilation_for_target(caller, crate::SelectedTarget::baseline())
}

pub(in crate::compilation) fn production_host_compilation_for_target(
    caller: &str,
    target: crate::SelectedTarget,
) -> Compilation {
    let sources = [
        include_str!("../../../../../standard-library/std/src/std.bray"),
        include_str!("../../../../../standard-library/std/src/memory.bray"),
        include_str!("../../../../../standard-library/std/src/platform.bray"),
        include_str!("../../../../../runtime/bootstrap/src/host.bray"),
        include_str!("../../../../../runtime/bootstrap/src/host/owner.bray"),
        include_str!("../../../../../runtime/bootstrap/src/host/access.bray"),
        include_str!("../../../../../runtime/bootstrap/src/host/attachment.bray"),
        include_str!("../../../../../runtime/bootstrap/src/host/attachment/entry.bray"),
        include_str!("../../../../../runtime/bootstrap/src/host/attachment/exit.bray"),
        include_str!("../../../../../runtime/bootstrap/src/host/attachment/cleanup.bray"),
        include_str!("../../../../../runtime/bootstrap/src/bootstrap.bray"),
        include_str!("../../../../../runtime/bootstrap/src/pool.bray"),
        include_str!("../../../../../runtime/bootstrap/src/record_pool.bray"),
        include_str!("../../../../../runtime/bootstrap/src/records.bray"),
        include_str!("../../../../../runtime/bootstrap/src/panic.bray"),
        include_str!("../../../../../runtime/bootstrap/src/outgoing.bray"),
        include_str!("../../../../../runtime/bootstrap/src/report_consumer.bray"),
        include_str!("../../../../../standard-library/std/src/sync.bray"),
        include_str!("../../../../../standard-library/std/src/sync/spin_lock.bray"),
        include_str!("../../../../../standard-library/std/src/sync/spin_lock/state.bray"),
        include_str!("../../../../../standard-library/std/src/sync/spin_lock/lock.bray"),
        include_str!("../../../../../standard-library/std/src/sync/spin_lock/guard.bray"),
        include_str!("../../../../../runtime/bootstrap/src/thread.bray"),
        include_str!("../../../../../runtime/bootstrap/src/thread/destruction.bray"),
        include_str!("../../../../../runtime/bootstrap/src/thread/destruction/common.bray"),
        include_str!("../../../../../runtime/bootstrap/src/thread/destruction/unix.bray"),
        include_str!("../../../../../runtime/bootstrap/src/thread/destruction/windows.bray"),
        include_str!("../../../../../standard-library/std/src/thread.bray"),
        include_str!("../../../../../standard-library/std/src/thread/boundary.bray"),
        include_str!("../../../../../standard-library/std/src/thread/windows.bray"),
        include_str!("../../../../../standard-library/std/src/thread/unix.bray"),
        include_str!("../../../../../standard-library/std/src/thread/linux.bray"),
        include_str!("../../../../../standard-library/std/src/thread/darwin.bray"),
        include_str!("../../../../../standard-library/std/src/platform/status.bray"),
        include_str!("../../../../../standard-library/std/src/platform/representation.bray"),
        include_str!("../../../../../standard-library/std/src/platform/linux.bray"),
        include_str!("../../../../../standard-library/std/src/platform/darwin.bray"),
        include_str!("../../../../../standard-library/std/src/platform/windows.bray"),
        include_str!("../../../../../runtime/bootstrap/src/memory/heap.bray"),
        include_str!("../../../../../standard-library/std/src/runtime/memory.bray"),
        include_str!("../../../../../standard-library/std/src/platform/heap.bray"),
        include_str!("../../../../../standard-library/std/src/platform/heap/windows.bray"),
        include_str!("../../../../../standard-library/std/src/platform/heap/unix.bray"),
        include_str!("../../../../../standard-library/std/src/os/windows.bray"),
        include_str!("../../../../../standard-library/std/src/os.bray"),
        include_str!("../../../../../standard-library/std/src/os/unix.bray"),
        include_str!("../../../../../standard-library/std/src/os/unix/thread.bray"),
        include_str!("../../../../../standard-library/std/src/os/unix/linux.bray"),
        include_str!("../../../../../standard-library/std/src/os/unix/darwin.bray"),
        include_str!("../../../../../runtime/bootstrap/src/host/synchronization.bray"),
        include_str!(
            "../../../../../standard-library/std/src/os/generated/x86_64_pc_windows_msvc.bray"
        ),
        include_str!(
            "../../../../../standard-library/std/src/os/generated/aarch64_pc_windows_msvc.bray"
        ),
        include_str!(
            "../../../../../standard-library/std/src/os/generated/x86_64_unknown_linux_gnu.bray"
        ),
        include_str!(
            "../../../../../standard-library/std/src/os/generated/aarch64_unknown_linux_gnu.bray"
        ),
        include_str!(
            "../../../../../standard-library/std/src/os/generated/x86_64_apple_darwin.bray"
        ),
        include_str!(
            "../../../../../standard-library/std/src/os/generated/aarch64_apple_darwin.bray"
        ),
        caller,
    ];

    let request = CompilationRequest::with_options(
        PackageIdentity::try_new("std").expect("standard library identity must be valid"),
        sources
            .iter()
            .enumerate()
            .map(|(index, source)| {
                source_input(
                    source,
                    u32::try_from(index).expect("host source index must fit"),
                )
            })
            .collect(),
        crate::CompilationOptions::new(
            crate::WorkerBudget::serial(),
            bray_symbols::ProductKind::Library,
            target,
        )
        .with_native_link_inputs(["c", "pthread", "Kernel32", "bray_runtime_host"].map(
            |name| {
                bray_symbols::NativeLinkRequirement::new(
                    bray_base::NonEmptySharedStr::try_new(name).expect("native test library name"),
                    bray_symbols::NativeLinkKind::System,
                )
            },
        )),
    )
    .with_standard_library_source_authority();

    let compilation =
        Compilation::load(request).expect("actual production host declarations must load");

    assert!(
        !compilation.syntax_tree_result().diagnostics().has_errors(),
        "actual bootstrap module declarations must parse: {:?}",
        compilation.syntax_tree_result().diagnostics()
    );

    compilation
}

#[test]
fn cleanup_backing_requires_the_live_allocation_and_its_exact_layout() {
    for (body, valid) in [
        ("trusted deallocate_bytes(pointer, 64, align = 32);", true),
        ("trusted deallocate_bytes(pointer, 65, align = 32);", false),
        ("trusted deallocate_bytes(pointer, 64, align = 16);", false),
        (
            "let copied = pointer; trusted deallocate_bytes(pointer, 64, align = 32); trusted deallocate_bytes(copied, 64, align = 32);",
            false,
        ),
        (
            "trusted deallocate_bytes(pointer, 64, align = 32); let next = trusted allocate(64, align = 32); if !std.memory.is_null<u8>(next) { trusted deallocate_bytes(pointer, 64, align = 32); trusted deallocate_bytes(next, 64, align = 32); }",
            false,
        ),
    ] {
        let caller = format!(
            r#"
            trusted internal module bray.runtime.bootstrap.heap;
            func probe() {{
                let pointer = trusted allocate(64, align = 32);
                if !std.memory.is_null<u8>(pointer) {{ {body} }}
            }}
            "#,
        );

        let compilation = production_host_compilation(&caller);

        let lowered = compilation
            .lowered_unit(crate::test_support::source_function_body_key(
                &compilation,
                "probe",
            ))
            .unwrap();

        assert_eq!(
            !lowered.diagnostics().has_errors(),
            valid,
            "{body}: {lowered:#?}"
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                lowered.diagnostics(),
                bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}
