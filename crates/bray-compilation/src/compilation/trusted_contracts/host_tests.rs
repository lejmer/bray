use bray_diagnostics::DiagnosticKind;
use bray_symbols::PackageIdentity;

use crate::test_support::{source_function_body_key, source_input};
use crate::{Compilation, CompilationRequest};

#[test]
fn production_report_consumers_check_their_callback_protocol() {
    let compilation = production_host_compilation("module app;");

    let names = [
        "dispose",
        "retain",
        "begin",
        "number",
        "consume_segment",
        "install",
        "compose",
        "exchange_record",
        "pop_record_value",
        "retain_primary",
        "accept_progress",
        "transfer",
        "retire",
        "acquire",
        "resolve",
        "execute",
        "invoke_retaining",
        "retained_address",
    ];

    let symbols = compilation.symbol_graph().unwrap();

    let sources = symbols
        .functions()
        .iter()
        .filter_map(|function| {
            let symbol = function.id().into();
            let name = symbols.member_name(symbol)?;

            names.contains(&name.as_str()).then(|| {
                symbols
                    .declaration_syntax_anchor(symbol)
                    .expect("production source function has its declaration")
                    .source_id()
            })
        })
        .collect::<Vec<_>>();

    let diagnostics = compilation.check_diagnostics();

    let failures = diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic
                .primary_span()
                .is_some_and(|span| sources.contains(&span.source_id()))
        })
        .collect::<Vec<_>>();

    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn native_report_addresses_retain_authority_only_for_the_unchanged_owner() {
    for (body, valid) in [
        (
            "let callbacks = trusted bray.runtime.bootstrap.primary_callbacks(&primary);",
            true,
        ),
        (
            "other.message_length = 0; let callbacks = trusted bray.runtime.bootstrap.primary_callbacks(&primary);",
            true,
        ),
        (
            "primary.release_message = 0; let callbacks = trusted bray.runtime.bootstrap.primary_callbacks(&primary);",
            false,
        ),
        (
            "let mut forged = trusted bray.runtime.bootstrap.empty_primary(); forged.release_message = primary.release_message; let callbacks = trusted bray.runtime.bootstrap.primary_callbacks(&forged);",
            false,
        ),
    ] {
        let compilation = production_host_compilation(&format!(
            r#"
            trusted internal module bray.runtime.bootstrap;
            trusted func production_probe(
                pos mut primary: bray.runtime.bootstrap.ReportPrimary,
                pos other: &mut bray.runtime.bootstrap.ReportPrimary,
            )
                requires(trusted bray.runtime.bootstrap.native_primary_callbacks(primary = &primary))
            {{
                {body}
            }}
        "#,
        ));

        let lowered = compilation
            .lowered_unit(source_function_body_key(&compilation, "production_probe"))
            .unwrap();

        assert_eq!(
            !lowered.diagnostics().has_errors(),
            valid,
            "{body}: {:?}",
            lowered.diagnostics()
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                lowered.diagnostics(),
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn raw_typed_incident_headers_do_not_grant_disposal_authority() {
    let compilation = production_host_compilation(
        r#"
        trusted internal module bray.runtime.bootstrap;
        trusted func production_probe(pos pointer: RawPointer<bray.runtime.bootstrap.report.incident.Record>) -> u32
            requires(trusted bray.runtime.bootstrap.pool.protocol_access())
        {
            return trusted bray.runtime.bootstrap.report.incident.dispose(pointer, 1);
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
fn retained_incident_values_require_the_exact_ownership_witness() {
    for (body, valid) in [
        (
            "let value = trusted bray.runtime.bootstrap.report.incident.read_record(pointer);",
            true,
        ),
        (
            "let value = bray.runtime.bootstrap.ReportValue.Error(forged);",
            false,
        ),
        (
            r#"
            let original = trusted bray.runtime.bootstrap.report.incident.read_record(pointer);
            let value = match consume original
            {
                case Panic(primary) { yield bray.runtime.bootstrap.ReportValue.Panic(primary); }
                case Error(record) { yield bray.runtime.bootstrap.ReportValue.Error(record); }
            };
            "#,
            false,
        ),
    ] {
        let compilation = production_host_compilation(&format!(
            r#"
            trusted internal module bray.runtime.bootstrap;
            trusted func production_probe(
                pos pointer: RawPointer<bray.runtime.bootstrap.report.incident.Record>,
                pos forged: bray.runtime.bootstrap.report.incident.Record,
            ) -> u32
                requires(
                    trusted bray.runtime.bootstrap.pool.protocol_access(),
                    trusted bray.runtime.bootstrap.report.incident.owned(pointer = pointer),
                )
            {{
                {body}
                return trusted bray.runtime.bootstrap.consume_values(
                    value,
                    bray.runtime.bootstrap.empty_records(),
                    0
                );
            }}
            "#,
        ));

        let checked = compilation
            .lowered_unit(source_function_body_key(&compilation, "production_probe"))
            .expect("retained incident probe must lower");

        assert_eq!(
            !checked.diagnostics().has_errors(),
            valid,
            "{body}: {:?}",
            checked.diagnostics(),
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
fn incident_outcome_destinations_require_distinct_exclusive_borrows() {
    let compilation = production_host_compilation(
        r#"
        trusted internal module bray.runtime.bootstrap;
        trusted func production_probe(pos outcome: &mut ReportOutcome) {
            let pair = trusted bray.runtime.bootstrap.report.incident.destinations(outcome, outcome);
        }
    "#,
    );

    let checked = compilation
        .lowered_unit(source_function_body_key(&compilation, "production_probe"))
        .unwrap();

    bray_testing::assert_goal_state_diagnostic_kind(
        checked.diagnostics(),
        DiagnosticKind::CheckingConflictingBorrow,
    );
}

#[test]
fn production_host_nested_bodies_and_contracts_are_checked() {
    let compilation = production_host_compilation("module app;");

    // Platform calls in this fixture resolve to source bodies. Their imported foreign-call
    // capabilities are checked by the runtime artifact smoke, not this source-only fixture.
    let owners = [
        "initialize_host",
        "shutdown_host",
        "structured_shutdown",
        "thread_attachment_enter",
        "lock",
        "release",
    ]
    .map(|name| {
        source_function_body_key(&compilation, name)
            .source()
            .syntax()
    });

    let acquisition_source = source_function_body_key(&compilation, "acquire")
        .source()
        .syntax()
        .source_id();

    let report_source = source_function_body_key(&compilation, "admit")
        .source()
        .syntax()
        .source_id();

    let bridge_source = source_function_body_key(&compilation, "install")
        .source()
        .syntax()
        .source_id();

    let entry_source = source_function_body_key(&compilation, "dispatch")
        .source()
        .syntax()
        .source_id();

    let diagnostics = compilation.check_diagnostics();

    let failures = diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic.primary_span().is_some_and(|span| {
                span.source_id() == acquisition_source
                    || span.source_id() == report_source
                    || span.source_id() == bridge_source
                    || span.source_id() == entry_source
                    || owners.iter().any(|owner| {
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
            false,
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
            "let _ = trusted bray.runtime.bootstrap.host.thread_attachment_leave();",
            false,
        ),
        (
            "let _ = trusted bray.runtime.bootstrap.host.thread_attachment_enter();",
            false,
        ),
        (
            "let owner = trusted bray.runtime.bootstrap.host.attachment.acquire();",
            false,
        ),
        (
            "let _ = trusted bray.runtime.bootstrap.host.thread_attachment_mark_main();",
            false,
        ),
        (
            "let _ = trusted bray.runtime.bootstrap.entry.bridge.install(std.memory.null<bray.runtime.bootstrap.entry.bridge.NativeCallbacks>(), std.memory.null<bray.runtime.bootstrap.entry.bridge.NativeCallbacks>());",
            false,
        ),
        (
            "let _ = trusted bray.runtime.bootstrap.entry.bridge.current();",
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
            "module app; using internal bray.runtime.bootstrap.host; using internal bray.runtime.bootstrap.thread.destruction; using internal bray.runtime.bootstrap.host.attachment; using internal bray.runtime.bootstrap.entry.bridge; func production_probe() {{ {operation} }}"
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
fn source_attachment_owner_requires_the_native_thread_protocol() {
    for (body, valid) in [
        ("let owner = trusted acquire();", true),
        (
            "let owner = trusted acquire(); let _ = trusted finish(owner);",
            true,
        ),
        (
            "let mut owner = trusted acquire(); owner.identity = 1;",
            false,
        ),
        (
            "let owner = trusted acquire(); let copied: Owner = { status = owner.status, identity = owner.identity };",
            false,
        ),
    ] {
        let source = format!(
            r#"
            trusted internal module bray.runtime.bootstrap.host.attachment;
            func production_probe() requires(trusted native_acquisition()) {{ {body} }}
        "#
        );

        let compilation = production_host_compilation(&source);

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
fn source_report_chain_requires_unchanged_exclusive_ownership() {
    for (body, valid) in [
        ("let owner = trusted admit(2);", true),
        (
            "let owner: Owner = { records = bray.runtime.bootstrap.ReportRecords { head = 1, tail = 1, count = 1 }, status = 0 };",
            false,
        ),
        (
            "let owner = trusted admit(2); let copied: Owner = { records = owner.records, status = owner.status };",
            false,
        ),
        (
            "let mut owner = trusted admit(2); owner.records.head = 1;",
            false,
        ),
        (
            "let mut owner = trusted admit(2); let other = trusted admit(2); owner.records = other.records;",
            false,
        ),
    ] {
        let source = format!(
            r#"
            trusted internal module bray.runtime.bootstrap.report.chain;
            using internal bray.runtime.bootstrap;
            func production_probe() {{ {body} }}
        "#
        );

        let compilation = production_host_compilation(&source);

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
fn production_report_pool_mutation_requires_protocol_authority() {
    for operation in [
        "let mut guard = trusted bray.runtime.bootstrap.pool.lock(); let pool = guard.get(); pool.credits = 0; pool.free = 1;",
        "let mut records: bray.runtime.bootstrap.ReportRecords = bray.runtime.bootstrap.empty_records(); let status = trusted bray.runtime.bootstrap.report_records_admit(1, std.memory.address_of_mut<bray.runtime.bootstrap.ReportRecords>(&mut records));",
        "let records: bray.runtime.bootstrap.ReportRecords = { head = 1, tail = 1, count = 1 }; let result = trusted bray.runtime.bootstrap.pop_record_value(records);",
    ] {
        let source = format!(
            r#"module app;
            using std.memory;
            using internal bray.runtime.bootstrap;
            using internal bray.runtime.bootstrap.pool;
            func production_probe() {{ {operation} }}"#,
        );

        let compilation = production_host_compilation(&source);

        let checked = compilation
            .lowered_unit(source_function_body_key(&compilation, "production_probe"))
            .expect("production report-pool probe must retain its checked result");

        bray_testing::assert_goal_state_diagnostic_kind(
            checked.diagnostics(),
            DiagnosticKind::CheckingTrustedObligationNotProven,
        );
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

    let exit_source = source_function_body_key(&compilation, "thread_attachment_leave")
        .source()
        .syntax()
        .source_id();

    for key in compilation
        .declared_unit_keys_for_test()
        .unwrap()
        .into_iter()
        .filter(|key| {
            key.kind() == bray_bound_tree::BoundUnitKind::CallableBody
                && key.source().syntax().source_id() == exit_source
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
    ] {
        let key = source_function_body_key(&compilation, name);

        let checked = compilation
            .lowered_unit(key.clone())
            .expect("production exit completion must retain its checked result");

        assert!(!checked.diagnostics().has_errors(), "{name}: {checked:#?}");
    }
}

#[test]
fn production_lease_destructor_checks_and_remains_total() {
    let compilation = production_host_compilation("module app;");

    let access_source = source_function_body_key(&compilation, "host_access")
        .source()
        .syntax()
        .source_id();

    let key = compilation
        .declared_unit_keys_for_test()
        .unwrap()
        .into_iter()
        .find(|key| {
            key.kind() == bray_bound_tree::BoundUnitKind::CallableBody
                && key.declared_owner().kind() == bray_symbols::SymbolKind::Destructor
                && key.source().syntax().source_id() == access_source
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

    let exit_source = source_function_body_key(&compilation, "thread_attachment_leave")
        .source()
        .syntax()
        .source_id();

    let key = compilation
        .declared_unit_keys_for_test()
        .unwrap()
        .into_iter()
        .find(|key| {
            key.kind() == bray_bound_tree::BoundUnitKind::CallableBody
                && key.declared_owner().kind() == bray_symbols::SymbolKind::Destructor
                && key.source().syntax().source_id() == exit_source
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
        ("mutate(&mut completed);", false),
        ("let incidents = completed.incidents;", true),
        ("let value = observe();", true),
        ("let value = acquire();", true),
        (
            "let incidents = completed.incidents; let value = acquire();",
            true,
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
        extern func mutate(pos value: &mut Completed);
        func observe() -> u64 executes(pure, total) { return 0; }
        trusted func complete(pos selected: Owner) -> Completed
            requires(trusted live(owner = &selected))
            ensures(trusted done(owner = &result))
        { return { selected = selected, incidents = 0 }; }
        extern trusted func consume_completion(pos completed: Completed) requires(trusted done(owner = &completed));
        func caller() {
            let selected = trusted owner();
            let mut completed = trusted complete(selected);
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
        include_str!("../../../../../standard-library/std/src/numeric/policy.bray"),
        include_str!("../../../../../standard-library/std/src/memory.bray"),
        include_str!("../../../../../standard-library/std/src/platform.bray"),
        include_str!("../../../../../runtime/bootstrap/src/host.bray"),
        include_str!("../../../../../runtime/bootstrap/src/entry.bray"),
        include_str!("../../../../../runtime/bootstrap/src/entry/protocol.bray"),
        include_str!("../../../../../runtime/bootstrap/src/entry/execution.bray"),
        include_str!("../../../../../runtime/bootstrap/src/entry/bridge.bray"),
        include_str!("../../../../../runtime/bootstrap/src/host/owner.bray"),
        include_str!("../../../../../runtime/bootstrap/src/host/access.bray"),
        include_str!("../../../../../runtime/bootstrap/src/host/attachment.bray"),
        include_str!("../../../../../runtime/bootstrap/src/host/attachment/entry.bray"),
        include_str!("../../../../../runtime/bootstrap/src/host/attachment/acquisition.bray"),
        include_str!("../../../../../runtime/bootstrap/src/host/attachment/exit.bray"),
        include_str!("../../../../../runtime/bootstrap/src/host/attachment/cleanup.bray"),
        include_str!("../../../../../runtime/bootstrap/src/bootstrap.bray"),
        include_str!("../../../../../runtime/bootstrap/src/pool.bray"),
        include_str!("../../../../../runtime/bootstrap/src/pool/access.bray"),
        include_str!("../../../../../runtime/bootstrap/src/pool/growth.bray"),
        include_str!("../../../../../runtime/bootstrap/src/record_pool.bray"),
        include_str!("../../../../../runtime/bootstrap/src/records.bray"),
        include_str!("../../../../../runtime/bootstrap/src/panic.bray"),
        include_str!("../../../../../runtime/bootstrap/src/report/transfer.bray"),
        include_str!("../../../../../runtime/bootstrap/src/report.bray"),
        include_str!("../../../../../runtime/bootstrap/src/report/chain.bray"),
        include_str!("../../../../../runtime/bootstrap/src/report/consumer.bray"),
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
        include_str!("../../../../../runtime/bootstrap/src/report/output.bray"),
        include_str!("../../../../../runtime/bootstrap/src/report/incident.bray"),
        include_str!("../../../../../runtime/bootstrap/src/report/sink.bray"),
        include_str!("../../../../../standard-library/std/src/io.bray"),
        include_str!("../../../../../standard-library/std/src/io/platform.bray"),
        include_str!("../../../../../standard-library/std/src/io/windows.bray"),
        include_str!("../../../../../standard-library/std/src/io/unix.bray"),
        caller,
        include_str!("../../../../../runtime/bootstrap/src/activation/data.bray"),
        include_str!("../../../../../runtime/bootstrap/src/activation/completion.bray"),
        include_str!("../../../../../runtime/bootstrap/src/activation/provider.bray"),
        include_str!("../../../../../runtime/bootstrap/src/activation/run/abandonment.bray"),
        include_str!("../../../../../runtime/bootstrap/src/activation/run/access.bray"),
        include_str!("../../../../../runtime/bootstrap/src/activation/run/advance.bray"),
        include_str!("../../../../../runtime/bootstrap/src/activation/run/completion.bray"),
        include_str!("../../../../../runtime/bootstrap/src/activation/run/entry.bray"),
        include_str!("../../../../../runtime/bootstrap/src/activation/run/retirement.bray"),
        include_str!("../../../../../runtime/bootstrap/src/activation/run/step.bray"),
        include_str!("../../../../../runtime/bootstrap/src/activation/storage.bray"),
        include_str!("../../../../../runtime/bootstrap/src/activation/access.bray"),
        include_str!("../../../../../runtime/bootstrap/src/activation/inactive.bray"),
        include_str!("../../../../../runtime/bootstrap/src/activation/transition.bray"),
        include_str!("../../../../../runtime/bootstrap/src/activation/composition.bray"),
        include_str!("../../../../../runtime/bootstrap/src/activation/callback.bray"),
        include_str!("../../../../../runtime/bootstrap/src/activation/execution.bray"),
        include_str!("../../../../../runtime/bootstrap/src/activation/execution/access.bray"),
        include_str!("../../../../../runtime/bootstrap/src/activation/execution/invoke.bray"),
        include_str!("../../../../../runtime/bootstrap/src/activation/execution/outcome.bray"),
        include_str!("../../../../../runtime/bootstrap/src/activation/requirements.bray"),
        include_str!("../../../../../runtime/bootstrap/src/activation/requirements/inactive.bray"),
        include_str!("../../../../../runtime/bootstrap/src/activation/requirements/active.bray"),
        include_str!("../../../../../runtime/bootstrap/src/activation/requirements/state.bray"),
        include_str!("../../../../../runtime/bootstrap/src/storage/data.bray"),
        include_str!("../../../../../runtime/bootstrap/src/storage/admission.bray"),
        include_str!("../../../../../runtime/bootstrap/src/storage/activation.bray"),
        include_str!("../../../../../runtime/bootstrap/src/storage/boundary.bray"),
        include_str!("../../../../../runtime/bootstrap/src/storage/bundle.bray"),
        include_str!("../../../../../runtime/bootstrap/src/outgoing/admission.bray"),
        include_str!("../../../../../runtime/bootstrap/src/outgoing/activation.bray"),
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

#[test]
fn production_storage_lease_requires_admission_evidence() {
    for (admission, body, valid) in [
        (true, "let status = trusted retire(pointer);", true),
        (false, "let status = trusted retire(pointer);", false),
        (
            false,
            "let address = trusted activate(pointer, description);",
            false,
        ),
        (
            false,
            "trusted claim(description, pointer); let status = trusted retire(pointer);",
            false,
        ),
        (
            true,
            "lease = empty_lease(); let status = trusted retire(pointer);",
            false,
        ),
        (
            true,
            "let address = trusted activate(pointer, description); trusted std.memory.write<u8>(address, 1);",
            false,
        ),
    ] {
        let caller = format!(
            r#"
            trusted internal module bray.runtime.bootstrap.storage;
            func probe() requires(trusted bray.runtime.bootstrap.pool.protocol_access()) {{
                let descriptor: Descriptor = {{ identity = [0; 32], bytes = 64, align = 32 }};
                let mut lease: Lease = empty_lease();
                let pointer = std.memory.address_of_mut<Lease>(&mut lease);
                let description = std.memory.address_of<Descriptor>(&descriptor);
                {admission}
                {{ {body} }}
            }}
            "#,
            admission = if admission {
                "if trusted admit(description, pointer) == 0"
            } else {
                ""
            },
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
            "{caller}: {lowered:#?}"
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                lowered.diagnostics(),
                bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn production_storage_boundaries_prove_their_declared_memory_effects() {
    let compilation = production_host_compilation("module probe;");
    let mut failures = Vec::new();
    let mut sources = std::collections::HashSet::new();

    let mut pending = compilation.declared_unit_keys_for_test().unwrap();

    while let Some(key) = pending.pop() {
        if !matches!(
            key.kind(),
            bray_bound_tree::BoundUnitKind::CallableBody
                | bray_bound_tree::BoundUnitKind::AnonymousCallable
                | bray_bound_tree::BoundUnitKind::ContractClause
        ) {
            continue;
        }

        let source = compilation
            .source(key.source().syntax().source_id())
            .unwrap()
            .text();

        if !source.contains("module bray.runtime.bootstrap.storage;")
            && !source.contains("module bray.runtime.bootstrap.outgoing;")
            && !source.contains("module bray.runtime.bootstrap.activation;")
            && !source.contains("module bray.runtime.bootstrap.activation.")
            && !source.contains("trusted internal func retain_report(")
        {
            continue;
        }

        sources.insert(key.source().syntax().source_id());

        let bound = compilation.bound_unit(key.clone()).unwrap();

        if bound.diagnostics().has_errors() {
            failures.push(format!("{key:?}: {:?}", bound.diagnostics()));
            continue;
        }

        if key.kind() == bray_bound_tree::BoundUnitKind::ContractClause {
            continue;
        }

        pending.extend(bound.value().nested_units().iter().cloned());

        let lowered = compilation.lowered_unit(key.clone()).unwrap();

        if lowered.diagnostics().has_errors() {
            failures.push(format!("{key:?}: {:?}", lowered.diagnostics()));
        }
    }

    for diagnostic in compilation.check_diagnostics().iter() {
        if diagnostic.severity() == bray_diagnostics::SeverityKind::Error
            && diagnostic
                .primary_span()
                .is_some_and(|span| sources.contains(&span.source_id()))
        {
            failures.push(format!("{diagnostic:?}"));
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn activation_cursor_fields_do_not_create_or_repair_selection_authority() {
    for (body, valid) in [
        (
            "let _ = trusted bray.runtime.bootstrap.activation.execution.read(original);",
            true,
        ),
        (
            "let mut cursor: bray.runtime.bootstrap.activation.run.Cursor = { root = original.root, pointer = other }; let _ = trusted bray.runtime.bootstrap.activation.execution.read(&mut cursor);",
            false,
        ),
        (
            "let mut cursor: bray.runtime.bootstrap.activation.run.Cursor = { root = original.root, pointer = original.pointer }; let _ = trusted bray.runtime.bootstrap.activation.execution.read(&mut cursor);",
            false,
        ),
        (
            "let mut cursor: bray.runtime.bootstrap.activation.run.Cursor = { root = other, pointer = original.pointer }; let _ = trusted bray.runtime.bootstrap.activation.execution.read(&mut cursor);",
            false,
        ),
    ] {
        let compilation = production_host_compilation(&format!(
            r#"
            trusted internal module bray.runtime.bootstrap;
            using std.memory;
            trusted func production_probe(
                pos original: &mut bray.runtime.bootstrap.activation.run.Cursor,
                pos other: RawPointer<bray.runtime.bootstrap.activation.Header>,
            )
                requires(trusted bray.runtime.bootstrap.activation.run.selected(cursor = &original))
            {{ {body} }}
            "#
        ));

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
                bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn activation_tokens_require_both_producer_and_entry_authority() {
    for (requirements, valid) in [
        ("", false),
        (
            "requires(trusted bray.runtime.bootstrap.activation.producer(token = token))",
            false,
        ),
        (
            "requires(trusted bray.runtime.bootstrap.activation.producer(token = token), trusted bray.runtime.bootstrap.activation.entry_authorized(token = token, origin = origin))",
            true,
        ),
    ] {
        let caller = format!(
            r#"
            trusted internal module bray.runtime.bootstrap;
            trusted func production_probe(
                pos token: &bray.runtime.bootstrap.activation.Inactive,
                pos origin: u64,
            ) -> usize
                requires(trusted bray.runtime.bootstrap.pool.protocol_access())
                {requirements}
            {{
                return trusted bray.runtime.bootstrap.activation.begin(token, origin);
            }}
        "#
        );

        let compilation = production_host_compilation(&caller);

        let lowered = compilation
            .lowered_unit(source_function_body_key(&compilation, "production_probe"))
            .unwrap();

        assert_eq!(
            !lowered.diagnostics().has_errors(),
            valid,
            "{caller}: {lowered:#?}"
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                lowered.diagnostics(),
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn activation_publication_requires_the_exact_producer_witness() {
    for (requirements, expression, valid) in [
        ("", "token", false),
        (
            "requires(trusted bray.runtime.bootstrap.activation.producer(token = token))",
            "token",
            true,
        ),
        (
            "requires(trusted bray.runtime.bootstrap.activation.producer(token = other))",
            "token",
            false,
        ),
    ] {
        let caller = format!(
            r#"
            trusted internal module bray.runtime.bootstrap;
            trusted func production_probe(
                pos token: &bray.runtime.bootstrap.activation.Inactive,
                pos other: &bray.runtime.bootstrap.activation.Inactive,
            ) -> u32
                requires(trusted bray.runtime.bootstrap.pool.protocol_access())
                {requirements}
            {{
                return trusted bray.runtime.bootstrap.activation.publish({expression});
            }}
            "#,
        );

        let compilation = production_host_compilation(&caller);

        let lowered = compilation
            .lowered_unit(source_function_body_key(&compilation, "production_probe"))
            .unwrap();

        assert_eq!(
            !lowered.diagnostics().has_errors(),
            valid,
            "{caller}: {lowered:#?}"
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                lowered.diagnostics(),
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn activation_dispatch_requires_exact_native_ownership() {
    for (requirements, valid) in [
        ("", false),
        (
            "requires(trusted bray.runtime.bootstrap.activation.run.selected(cursor = other))",
            false,
        ),
        (
            "requires(trusted bray.runtime.bootstrap.activation.run.selected(cursor = &cursor))",
            true,
        ),
    ] {
        let caller = format!(
            r#"
            trusted internal module bray.runtime.bootstrap;
            trusted func production_probe(
                pos cursor: &mut bray.runtime.bootstrap.activation.run.Cursor,
                pos other: &bray.runtime.bootstrap.activation.run.Cursor,
                pos callback: bray.runtime.bootstrap.activation.callback.Invoke,
                pos ready: bray.runtime.bootstrap.activation.callback.Ready,
            ) -> bray.runtime.bootstrap.activation.Progress
                requires(trusted bray.runtime.bootstrap.entry.native_entry())
                {requirements}
            {{
                let (progress, _, _, _, _) = trusted bray.runtime.bootstrap.activation.execution.invoke(cursor, callback, ready, 0, 0);
                return progress;
            }}
        "#
        );

        let compilation = production_host_compilation(&caller);

        let lowered = compilation
            .lowered_unit(source_function_body_key(&compilation, "production_probe"))
            .unwrap();

        assert_eq!(
            !lowered.diagnostics().has_errors(),
            valid,
            "{caller}: {:?}",
            lowered.diagnostics()
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                lowered.diagnostics(),
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn native_activation_requires_producer_and_exact_entry_authority() {
    for (requirements, valid) in [
        (
            "requires(trusted bray.runtime.bootstrap.activation.producer_slot(pointer = pointer))",
            false,
        ),
        (
            "requires(trusted bray.runtime.bootstrap.activation.entry_slot_authorized(pointer = pointer, origin = origin))",
            false,
        ),
        (
            "requires(trusted bray.runtime.bootstrap.activation.producer_slot(pointer = pointer), trusted bray.runtime.bootstrap.activation.entry_slot_authorized(pointer = pointer, origin = origin))",
            true,
        ),
        (
            "requires(trusted bray.runtime.bootstrap.activation.producer_slot(pointer = pointer), trusted bray.runtime.bootstrap.activation.entry_slot_authorized(pointer = other, origin = origin))",
            false,
        ),
        (
            "requires(trusted bray.runtime.bootstrap.activation.producer_slot(pointer = pointer), trusted bray.runtime.bootstrap.activation.entry_slot_authorized(pointer = pointer, origin = 0))",
            false,
        ),
    ] {
        let caller = format!(
            r#"
            trusted internal module bray.runtime.bootstrap;
            trusted func production_probe(
                pos pointer: RawPointer<bray.runtime.bootstrap.activation.Inactive>,
                pos other: RawPointer<bray.runtime.bootstrap.activation.Inactive>,
                pos origin: u64,
            ) -> usize
                requires(
                    trusted bray.runtime.bootstrap.pool.protocol_access(),
                    trusted core.memory.valid_read<bray.runtime.bootstrap.activation.Inactive>(pointer = pointer, count = 1),
                    trusted core.memory.initialized_as<bray.runtime.bootstrap.activation.Inactive>(pointer = pointer),
                    trusted core.memory.aligned_for<bray.runtime.bootstrap.activation.Inactive>(pointer = pointer),
                )
                {requirements}
            {{ return trusted bray.runtime.bootstrap.activation.activate(pointer, origin); }}
        "#
        );

        let compilation = production_host_compilation(&caller);

        let lowered = compilation
            .lowered_unit(source_function_body_key(&compilation, "production_probe"))
            .unwrap();

        assert_eq!(
            !lowered.diagnostics().has_errors(),
            valid,
            "{caller}: {:?}",
            lowered.diagnostics()
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                lowered.diagnostics(),
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn native_activation_dispatch_requires_both_exact_callback_addresses() {
    for (requirement, valid) in [
        ("", false),
        (
            "requires(trusted core.memory.callable_address_valid<bray.runtime.bootstrap.activation.callback.Invoke>(pointer = other))",
            false,
        ),
        (
            "requires(trusted core.memory.callable_address_valid<bray.runtime.bootstrap.activation.callback.Invoke>(pointer = callback))",
            false,
        ),
        (
            "requires(trusted core.memory.callable_address_valid<bray.runtime.bootstrap.activation.callback.Invoke>(pointer = callback), trusted core.memory.callable_address_valid<bray.runtime.bootstrap.activation.callback.Ready>(pointer = other_ready))",
            false,
        ),
        (
            "requires(trusted core.memory.callable_address_valid<bray.runtime.bootstrap.activation.callback.Invoke>(pointer = callback), trusted core.memory.callable_address_valid<bray.runtime.bootstrap.activation.callback.Ready>(pointer = ready))",
            true,
        ),
    ] {
        let caller = format!(
            r#"
            trusted internal module bray.runtime.bootstrap;
            trusted func production_probe(
                pos pointer: RawPointer<bray.runtime.bootstrap.activation.Header>,
                pos callback: RawPointer<bray.runtime.bootstrap.activation.callback.Invoke>,
                pos ready: RawPointer<bray.runtime.bootstrap.activation.callback.Ready>,
                pos other_ready: RawPointer<bray.runtime.bootstrap.activation.callback.Ready>,
                pos other: RawPointer<bray.runtime.bootstrap.activation.callback.Invoke>,
                pos output: RawPointer<bray.runtime.bootstrap.activation.Progress>,
                pos records: RawPointer<bray.runtime.bootstrap.ReportRecords>,
                pos incidents: RawPointer<bray.runtime.bootstrap.ReportRecords>,
            ) -> u32
                requires(
                    trusted bray.runtime.bootstrap.activation.execution.owned(pointer = pointer),
                    trusted bray.runtime.bootstrap.entry.native_entry(),
                    trusted bray.runtime.bootstrap.activation.execution.destinations(pointer = pointer, output = output, records = records, incidents = incidents),
                    trusted core.memory.valid_write<bray.runtime.bootstrap.activation.Progress>(pointer = output, count = 1),
                    trusted core.memory.aligned_for<bray.runtime.bootstrap.activation.Progress>(pointer = output),
                    trusted core.memory.valid_write<bray.runtime.bootstrap.ReportRecords>(pointer = records, count = 1),
                    trusted core.memory.aligned_for<bray.runtime.bootstrap.ReportRecords>(pointer = records),
                )
                {requirement}
            {{
                return trusted bray.runtime.bootstrap.activation.execution.execute(pointer, callback, ready, 0, 0, output, records, incidents);
            }}
            "#
        );

        let compilation = production_host_compilation(&caller);

        let lowered = compilation
            .lowered_unit(source_function_body_key(&compilation, "production_probe"))
            .unwrap();

        assert_eq!(
            !lowered.diagnostics().has_errors(),
            valid,
            "{caller}: {:?}",
            lowered.diagnostics()
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                lowered.diagnostics(),
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn activation_retirement_does_not_preserve_dispatch_ownership() {
    for (operation, valid) in [(0, true), (5, false)] {
        let caller = format!(
            r#"
            trusted internal module bray.runtime.bootstrap;
            trusted func production_probe(
                pos cursor: &mut bray.runtime.bootstrap.activation.run.Cursor,
                pos callback: bray.runtime.bootstrap.activation.callback.Invoke,
                pos ready: bray.runtime.bootstrap.activation.callback.Ready,
            ) -> bray.runtime.bootstrap.activation.Progress
                requires(
                    trusted bray.runtime.bootstrap.activation.run.selected(cursor = &cursor),
                    trusted bray.runtime.bootstrap.entry.native_entry(),
                    trusted bray.runtime.bootstrap.activation.execution.retiring(pointer = cursor.pointer),
                )
            {{
                let _ = trusted bray.runtime.bootstrap.activation.execution.invoke(cursor, callback, ready, {operation}, 0);
                let (progress, _, _, _, _) = trusted bray.runtime.bootstrap.activation.execution.invoke(cursor, callback, ready, 0, 0);
                return progress;
            }}
            "#
        );

        let compilation = production_host_compilation(&caller);

        let lowered = compilation
            .lowered_unit(source_function_body_key(&compilation, "production_probe"))
            .unwrap();

        assert_eq!(
            !lowered.diagnostics().has_errors(),
            valid,
            "{caller}: {:?}",
            lowered.diagnostics()
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                lowered.diagnostics(),
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn activation_deferred_retirement_preserves_dispatch_ownership() {
    let compilation = production_host_compilation(
        r#"
        trusted internal module bray.runtime.bootstrap;
        trusted func production_probe(
            pos cursor: &mut bray.runtime.bootstrap.activation.run.Cursor,
            pos callback: bray.runtime.bootstrap.activation.callback.Invoke,
            pos ready: bray.runtime.bootstrap.activation.callback.Ready,
        )
            requires(
                trusted bray.runtime.bootstrap.activation.run.selected(cursor = &cursor),
                trusted bray.runtime.bootstrap.entry.native_entry(),
                    trusted bray.runtime.bootstrap.activation.execution.retiring(pointer = cursor.pointer),
            )
        {
            let (_, _, _, status, _) = trusted bray.runtime.bootstrap.activation.execution.invoke(cursor, callback, ready, 5, 0);

            if status == 0
            {
                return;
            }

            let _ = trusted bray.runtime.bootstrap.activation.execution.read(cursor);
        }
        "#,
    );

    let lowered = compilation
        .lowered_unit(source_function_body_key(&compilation, "production_probe"))
        .unwrap();

    assert!(
        !lowered.diagnostics().has_errors(),
        "{:?}",
        lowered.diagnostics()
    );
}

#[test]
fn activation_callback_requires_one_exact_authorized_invocation() {
    for (execution, descriptor, context, operation, argument, progress, records, repeat, valid) in [
        (
            "other_execution",
            "descriptor",
            "context",
            "0",
            "0",
            "progress",
            "records",
            false,
            false,
        ),
        (
            "execution",
            "descriptor",
            "context",
            "0",
            "0",
            "progress",
            "records",
            false,
            true,
        ),
        (
            "execution",
            "other_descriptor",
            "context",
            "0",
            "0",
            "progress",
            "records",
            false,
            false,
        ),
        (
            "execution",
            "descriptor",
            "other_context",
            "0",
            "0",
            "progress",
            "records",
            false,
            false,
        ),
        (
            "execution",
            "descriptor",
            "context",
            "1",
            "0",
            "progress",
            "records",
            false,
            false,
        ),
        (
            "execution",
            "descriptor",
            "context",
            "0",
            "1",
            "progress",
            "records",
            false,
            false,
        ),
        (
            "execution",
            "descriptor",
            "context",
            "0",
            "0",
            "other_progress",
            "records",
            false,
            false,
        ),
        (
            "execution",
            "descriptor",
            "context",
            "0",
            "0",
            "progress",
            "other_records",
            false,
            false,
        ),
        (
            "execution",
            "descriptor",
            "context",
            "0",
            "0",
            "progress",
            "records",
            true,
            false,
        ),
    ] {
        let invocation =
            "trusted callback(execution, descriptor, context, 0, 0, progress, records);";

        let repeated = if repeat { invocation } else { "" };

        let caller = format!(
            r#"
            trusted internal module bray.runtime.bootstrap;
            trusted func production_probe(
                pos callback: bray.runtime.bootstrap.activation.callback.Invoke,
                pos execution: RawPointer<bray.runtime.bootstrap.activation.requirements.Execution>,
                pos other_execution: RawPointer<bray.runtime.bootstrap.activation.requirements.Execution>,
                pos descriptor: RawPointer<bray.runtime.bootstrap.activation.Descriptor>,
                pos other_descriptor: RawPointer<bray.runtime.bootstrap.activation.Descriptor>,
                pos context: usize,
                pos other_context: usize,
                pos progress: RawPointer<bray.runtime.bootstrap.activation.Progress>,
                pos other_progress: RawPointer<bray.runtime.bootstrap.activation.Progress>,
                pos records: RawPointer<bray.runtime.bootstrap.ReportRecords>,
                pos other_records: RawPointer<bray.runtime.bootstrap.ReportRecords>,
            )
                requires(
                    trusted bray.runtime.bootstrap.entry.native_entry(),
                    trusted bray.runtime.bootstrap.activation.callback.authorized(
                        execution = {execution},
                        descriptor = {descriptor},
                        context = {context},
                        operation = {operation},
                        argument = {argument},
                        progress = {progress},
                        records = {records},
                    ),
                )
            {{
                {invocation}
                {repeated}
            }}
            "#
        );

        let compilation = production_host_compilation(&caller);

        let lowered = compilation
            .lowered_unit(source_function_body_key(&compilation, "production_probe"))
            .unwrap();

        assert_eq!(
            !lowered.diagnostics().has_errors(),
            valid,
            "{caller}: {:?}",
            lowered.diagnostics()
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                lowered.diagnostics(),
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn activation_retirement_requires_retained_native_destinations() {
    for (requirement, valid) in [
        ("", false),
        (
            "requires(trusted bray.runtime.bootstrap.activation.execution.destinations(pointer = other, output = output, records = records, incidents = incidents))",
            false,
        ),
        (
            "requires(trusted bray.runtime.bootstrap.activation.execution.destinations(pointer = pointer, output = other_output, records = records, incidents = incidents))",
            false,
        ),
        (
            "requires(trusted bray.runtime.bootstrap.activation.execution.destinations(pointer = pointer, output = output, records = other_records, incidents = incidents))",
            false,
        ),
        (
            "requires(trusted bray.runtime.bootstrap.activation.execution.destinations(pointer = pointer, output = output, records = records, incidents = incidents))",
            true,
        ),
    ] {
        let caller = format!(
            r#"
            trusted internal module bray.runtime.bootstrap;
            trusted func production_probe(
                pos pointer: RawPointer<bray.runtime.bootstrap.activation.Header>,
                pos other: RawPointer<bray.runtime.bootstrap.activation.Header>,
                pos callback: RawPointer<bray.runtime.bootstrap.activation.callback.Invoke>,
                pos ready: RawPointer<bray.runtime.bootstrap.activation.callback.Ready>,
                pos output: RawPointer<bray.runtime.bootstrap.activation.Progress>,
                pos other_output: RawPointer<bray.runtime.bootstrap.activation.Progress>,
                pos records: RawPointer<bray.runtime.bootstrap.ReportRecords>,
                pos incidents: RawPointer<bray.runtime.bootstrap.ReportRecords>,
                pos other_records: RawPointer<bray.runtime.bootstrap.ReportRecords>,
            ) -> u32
                requires(
                    trusted bray.runtime.bootstrap.activation.execution.owned(pointer = pointer),
                    trusted bray.runtime.bootstrap.entry.native_entry(),
                    trusted bray.runtime.bootstrap.activation.execution.retiring(pointer = pointer),
                    trusted bray.runtime.bootstrap.activation.execution.observed(pointer = pointer),
                    trusted core.memory.callable_address_valid<bray.runtime.bootstrap.activation.callback.Invoke>(pointer = callback),
                    trusted core.memory.callable_address_valid<bray.runtime.bootstrap.activation.callback.Ready>(pointer = ready),
                    trusted core.memory.valid_write<bray.runtime.bootstrap.activation.Progress>(pointer = output, count = 1),
                    trusted core.memory.aligned_for<bray.runtime.bootstrap.activation.Progress>(pointer = output),
                    trusted core.memory.valid_write<bray.runtime.bootstrap.ReportRecords>(pointer = records, count = 1),
                    trusted core.memory.aligned_for<bray.runtime.bootstrap.ReportRecords>(pointer = records),
                )
                {requirement}
            {{
                return trusted bray.runtime.bootstrap.activation.execution.execute(pointer, callback, ready, 5, 0, output, records, incidents);
            }}
            "#
        );

        let compilation = production_host_compilation(&caller);

        let lowered = compilation
            .lowered_unit(source_function_body_key(&compilation, "production_probe"))
            .unwrap();

        assert_eq!(
            !lowered.diagnostics().has_errors(),
            valid,
            "{caller}: {:?}",
            lowered.diagnostics()
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                lowered.diagnostics(),
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn inactive_inspection_requires_retained_exact_owner_and_output() {
    for (producer, owner, output, valid) in [
        (false, "pointer", "output", false),
        (true, "other", "output", false),
        (true, "pointer", "other_output", false),
        (true, "pointer", "output", true),
    ] {
        let producer = if producer {
            "requires(trusted bray.runtime.bootstrap.activation.producer_slot(pointer = pointer))"
        } else {
            ""
        };

        let caller = format!(
            r#"
            trusted internal module bray.runtime.bootstrap;
            trusted func production_probe(
                pos pointer: RawPointer<bray.runtime.bootstrap.activation.Inactive>,
                pos other: RawPointer<bray.runtime.bootstrap.activation.Inactive>,
                pos output: RawPointer<bray.runtime.bootstrap.activation.requirements.Execution>,
                pos other_output: RawPointer<bray.runtime.bootstrap.activation.requirements.Execution>,
            ) -> u32
                {producer}
                requires(
                    trusted bray.runtime.bootstrap.activation.requirements.retained(token = {owner}, output = {output}),
                    trusted bray.runtime.bootstrap.pool.protocol_access(),
                    trusted core.memory.valid_read<bray.runtime.bootstrap.activation.Inactive>(pointer = pointer, count = 1),
                    trusted core.memory.initialized_as<bray.runtime.bootstrap.activation.Inactive>(pointer = pointer),
                    trusted core.memory.aligned_for<bray.runtime.bootstrap.activation.Inactive>(pointer = pointer),
                )
            {{
                return trusted bray.runtime.bootstrap.activation.requirements.inspect(pointer, 0, 0, 0, output);
            }}
            "#
        );

        let compilation = production_host_compilation(&caller);

        let lowered = compilation
            .lowered_unit(source_function_body_key(&compilation, "production_probe"))
            .unwrap();

        assert_eq!(
            !lowered.diagnostics().has_errors(),
            valid,
            "{caller}: {:?}",
            lowered.diagnostics()
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                lowered.diagnostics(),
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn metadata_inspection_cannot_infer_exclusive_ownership_from_a_producer() {
    for (owner, valid) in [("", false), ("token", true), ("other", false)] {
        let ownership = if owner.is_empty() {
            String::new()
        } else {
            format!(
                "requires(trusted bray.runtime.bootstrap.activation.requirements.inactive_owner(token = {owner}))"
            )
        };

        let caller = format!(
            r#"
            trusted internal module bray.runtime.bootstrap;
            trusted func production_probe(
                pos token: &bray.runtime.bootstrap.activation.Inactive,
                pos other: &bray.runtime.bootstrap.activation.Inactive,
            ) -> bray.runtime.bootstrap.activation.requirements.State?
                requires(
                    trusted bray.runtime.bootstrap.activation.producer(token = token),
                    trusted bray.runtime.bootstrap.pool.protocol_access(),
                )
                {ownership}
            {{
                return trusted bray.runtime.bootstrap.activation.requirements.read(token, 0, 0, 0);
            }}
            "#
        );

        let compilation = production_host_compilation(&caller);

        let lowered = compilation
            .lowered_unit(source_function_body_key(&compilation, "production_probe"))
            .unwrap();

        assert_eq!(
            !lowered.diagnostics().has_errors(),
            valid,
            "{caller}: {:?}",
            lowered.diagnostics()
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                lowered.diagnostics(),
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn active_snapshot_requires_exact_dispatch_ownership_and_retained_output() {
    for (owner, retained, output, valid) in [
        ("", "pointer", "output", false),
        ("other", "pointer", "output", false),
        ("pointer", "other", "output", false),
        ("pointer", "pointer", "other_output", false),
        ("pointer", "pointer", "output", true),
    ] {
        let ownership = if owner.is_empty() {
            String::new()
        } else {
            format!(
                "requires(trusted bray.runtime.bootstrap.activation.execution.owned(pointer = {owner}))"
            )
        };

        let caller = format!(
            r#"
            trusted internal module bray.runtime.bootstrap;
            trusted func production_probe(
                pos pointer: RawPointer<bray.runtime.bootstrap.activation.Header>,
                pos other: RawPointer<bray.runtime.bootstrap.activation.Header>,
                pos output: RawPointer<bray.runtime.bootstrap.activation.requirements.Execution>,
                pos other_output: RawPointer<bray.runtime.bootstrap.activation.requirements.Execution>,
            ) -> u32
                {ownership}
                requires(
                    trusted bray.runtime.bootstrap.activation.requirements.active_destination(pointer = {retained}, output = {output}),
                    trusted core.memory.valid_read<bray.runtime.bootstrap.activation.Header>(pointer = pointer, count = 1),
                    trusted core.memory.initialized_as<bray.runtime.bootstrap.activation.Header>(pointer = pointer),
                    trusted core.memory.aligned_for<bray.runtime.bootstrap.activation.Header>(pointer = pointer),
                )
            {{
                return trusted bray.runtime.bootstrap.activation.requirements.snapshot(pointer, 0, 0, 0, output);
            }}
            "#
        );

        let compilation = production_host_compilation(&caller);

        let lowered = compilation
            .lowered_unit(source_function_body_key(&compilation, "production_probe"))
            .unwrap();

        assert_eq!(
            !lowered.diagnostics().has_errors(),
            valid,
            "{caller}: {:?}",
            lowered.diagnostics()
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                lowered.diagnostics(),
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn composition_requires_the_exact_parent_token_continuation_and_origin() {
    for (parent, token, state, origin, destination, abandoning, valid) in [
        ("parent", "token", "state", "origin", "destination", 0, true),
        ("other", "token", "state", "origin", "destination", 0, false),
        (
            "parent",
            "other_token",
            "state",
            "origin",
            "destination",
            0,
            false,
        ),
        (
            "parent",
            "token",
            "other_state",
            "origin",
            "destination",
            0,
            false,
        ),
        (
            "parent",
            "token",
            "state",
            "other_origin",
            "destination",
            0,
            false,
        ),
        (
            "parent",
            "token",
            "state",
            "origin",
            "other_destination",
            0,
            false,
        ),
        ("parent", "token", "state", "origin", "destination", 1, true),
        (
            "parent",
            "other_token",
            "state",
            "origin",
            "destination",
            1,
            false,
        ),
    ] {
        let caller = format!(
            r#"
            trusted internal module bray.runtime.bootstrap;
            trusted func production_probe(
                pos parent: RawPointer<bray.runtime.bootstrap.activation.Header>,
                pos other: RawPointer<bray.runtime.bootstrap.activation.Header>,
                pos token: RawPointer<bray.runtime.bootstrap.activation.Inactive>,
                pos other_token: RawPointer<bray.runtime.bootstrap.activation.Inactive>,
                pos state: u32, pos other_state: u32, pos origin: u64, pos other_origin: u64,
                pos destination: usize, pos other_destination: usize,
                pos ready: RawPointer<bray.runtime.bootstrap.activation.callback.Ready>,
            ) -> usize
                requires(
                    trusted bray.runtime.bootstrap.activation.execution.owned(pointer = parent),
                    trusted bray.runtime.bootstrap.pool.protocol_access(),
                    trusted bray.runtime.bootstrap.entry.native_entry(),
                    trusted core.memory.callable_address_valid<bray.runtime.bootstrap.activation.callback.Ready>(pointer = ready),
                    trusted bray.runtime.bootstrap.activation.composition.authorized(
                        parent = {parent}, token = {token}, state = {state}, origin = {origin}, destination = {destination}, disposition = {abandoning}
                    ),
                )
            {{
                return trusted bray.runtime.bootstrap.activation.composition.compose(parent, token, state, origin, destination, {abandoning}, ready);
            }}
        "#
        );

        let compilation = production_host_compilation(&caller);

        let lowered = compilation
            .lowered_unit(source_function_body_key(&compilation, "production_probe"))
            .unwrap();

        assert_eq!(
            !lowered.diagnostics().has_errors(),
            valid,
            "{caller}: {:?}",
            lowered.diagnostics()
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                lowered.diagnostics(),
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn activation_completion_requires_the_exact_admitted_destination() {
    for (requirement, valid) in [
        ("", false),
        (
            "requires(trusted bray.runtime.bootstrap.activation.execution.completion_destination(pointer = other, address = destination))",
            false,
        ),
        (
            "requires(trusted bray.runtime.bootstrap.activation.execution.completion_destination(pointer = cursor.pointer, address = other_destination))",
            false,
        ),
        (
            "requires(trusted bray.runtime.bootstrap.activation.execution.completion_destination(pointer = cursor.pointer, address = destination))",
            true,
        ),
    ] {
        let caller = format!(
            r#"
            trusted internal module bray.runtime.bootstrap;
            trusted func production_probe(
                pos cursor: &mut bray.runtime.bootstrap.activation.run.Cursor,
                pos other: RawPointer<bray.runtime.bootstrap.activation.Header>,
                pos callback: bray.runtime.bootstrap.activation.callback.Invoke,
                pos ready: bray.runtime.bootstrap.activation.callback.Ready,
                pos destination: usize,
                pos other_destination: usize,
            )
                requires(
                    trusted bray.runtime.bootstrap.activation.run.selected(cursor = &cursor),
                    trusted bray.runtime.bootstrap.entry.native_entry(),
                )
                {requirement}
            {{
                let _ = trusted bray.runtime.bootstrap.activation.execution.invoke(cursor, callback, ready, 4, destination);
            }}
        "#
        );

        let compilation = production_host_compilation(&caller);

        let lowered = compilation
            .lowered_unit(source_function_body_key(&compilation, "production_probe"))
            .unwrap();

        assert_eq!(
            !lowered.diagnostics().has_errors(),
            valid,
            "{caller}: {:?}",
            lowered.diagnostics()
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                lowered.diagnostics(),
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn activation_header_access_retains_ownership_and_excludes_overlapping_borrows() {
    let mut failures = Vec::new();

    for (body, valid) in [
        (
            "let _ = trusted bray.runtime.bootstrap.activation.execution.read(cursor); let _ = trusted bray.runtime.bootstrap.activation.execution.read(cursor);",
            true,
        ),
        (
            "let header = trusted bray.runtime.bootstrap.activation.execution.access(cursor); header.state = 1;",
            true,
        ),
        (
            "let mut forged: bray.runtime.bootstrap.activation.run.Cursor = { root = cursor.root, pointer = other }; let header = trusted bray.runtime.bootstrap.activation.execution.access(&mut forged); header.state = 1;",
            false,
        ),
        (
            "let left = trusted bray.runtime.bootstrap.activation.execution.access(cursor); let right = trusted bray.runtime.bootstrap.activation.execution.access(cursor); left.state = 1; right.state = 2;",
            false,
        ),
    ] {
        let compilation = production_host_compilation(&format!(
            r#"
            trusted internal module bray.runtime.bootstrap;
            trusted func production_probe(
                pos cursor: &mut bray.runtime.bootstrap.activation.run.Cursor,
                pos other: RawPointer<bray.runtime.bootstrap.activation.Header>,
            )
                requires(trusted bray.runtime.bootstrap.activation.run.selected(cursor = &cursor))
            {{ {body} }}
            "#
        ));

        let checked = compilation
            .lowered_unit(source_function_body_key(&compilation, "production_probe"))
            .unwrap();

        if !checked.diagnostics().has_errors() != valid {
            failures.push(format!("{body}: {:?}", checked.diagnostics()));
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn inactive_cursor_requires_the_exact_native_token_and_outcome_pair() {
    for (authority, valid) in [
        ("", false),
        ("trusted entry(token = pointer, outcome = outcome),", true),
        ("trusted entry(token = other, outcome = outcome),", false),
        (
            "trusted entry(token = pointer, outcome = other_outcome),",
            false,
        ),
    ] {
        let source = format!(
            r#"
            trusted internal module bray.runtime.bootstrap.activation.inactive;
            using std.memory;

            func production_probe(
                pos pointer: RawPointer<bray.runtime.bootstrap.activation.Inactive>,
                pos other: RawPointer<bray.runtime.bootstrap.activation.Inactive>,
                pos outcome: RawPointer<bray.runtime.bootstrap.ReportOutcome>,
                pos other_outcome: RawPointer<bray.runtime.bootstrap.ReportOutcome>,
            )
                requires(
                    {authority}
                    trusted bray.runtime.bootstrap.pool.protocol_access(),
                    trusted bray.runtime.bootstrap.activation.producer_slot(pointer = pointer),
                    trusted core.memory.valid_read<bray.runtime.bootstrap.activation.Inactive>(pointer = pointer, count = 1),
                    trusted core.memory.aligned_for<bray.runtime.bootstrap.activation.Inactive>(pointer = pointer),
                    trusted core.memory.initialized_as<bray.runtime.bootstrap.activation.Inactive>(pointer = pointer),
                )
            {{
                let cursor = trusted acquire(pointer, outcome);
            }}
            "#,
        );

        let compilation = production_host_compilation(&source);

        let checked = compilation
            .lowered_unit(source_function_body_key(&compilation, "production_probe"))
            .unwrap();

        assert_eq!(
            !checked.diagnostics().has_errors(),
            valid,
            "{source}: {checked:#?}"
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
fn inactive_cleanup_retains_only_its_independent_native_destination() {
    for (authority, valid) in [
        ("", false),
        ("trusted destination(output = outcome),", true),
        ("trusted destination(output = other),", false),
    ] {
        let source = format!(
            r#"
            trusted internal module bray.runtime.bootstrap.activation.inactive;
            using std.memory;

            func production_probe(
                pos mut cursor: bray.runtime.bootstrap.activation.run.Cursor,
                pos callback: bray.runtime.bootstrap.activation.callback.Invoke,
                pos ready: bray.runtime.bootstrap.activation.callback.Ready,
                pos outcome: RawPointer<bray.runtime.bootstrap.ReportOutcome>,
                pos other: RawPointer<bray.runtime.bootstrap.ReportOutcome>,
            )
                requires(
                    {authority}
                    trusted bray.runtime.bootstrap.activation.run.selected(cursor = &cursor),
                    trusted bray.runtime.bootstrap.entry.native_entry(),
                    trusted bray.runtime.bootstrap.pool.protocol_access(),
                    trusted core.memory.valid_write<bray.runtime.bootstrap.ReportOutcome>(pointer = outcome, count = 1),
                    trusted core.memory.aligned_for<bray.runtime.bootstrap.ReportOutcome>(pointer = outcome),
                )
            {{
                let _ = trusted resolve(cursor, callback, ready, false, outcome);
            }}
            "#,
        );

        let compilation = production_host_compilation(&source);

        let checked = compilation
            .lowered_unit(source_function_body_key(&compilation, "production_probe"))
            .unwrap();

        assert_eq!(
            !checked.diagnostics().has_errors(),
            valid,
            "{source}: {checked:#?}"
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                checked.diagnostics(),
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}
