use bray_diagnostics::DiagnosticKind;
use bray_symbols::PackageIdentity;

use crate::test_support::{compilation, source_function_body_key, source_input};
use crate::{Compilation, CompilationRequest};

#[test]
fn sibling_mutation_preserves_a_returned_component_witness() {
    for (mutation, valid) in [
        ("", true),
        ("access.payload = 1;", true),
        ("access.lease.active = false;", false),
    ] {
        let source = format!(
            r#"
            trusted module app;
            trusted predicate live(value: &Lease);
            struct Lease {{ mut active: bool; }}
            struct Access {{ mut payload: u64; mut lease: Lease; }}
            trusted func acquire() -> Access
                ensures(trusted live(&result.lease)) {{
                return {{ payload = 0, lease = {{ active = true }} }};
            }}
            func observe(pos lease: &Lease)
                requires(trusted live(lease))
                ensures(trusted live(lease)) executes(pure, total) {{}}
            func caller() {{
                let mut access = acquire();
                observe(&access.lease);
                {mutation}
                observe(&access.lease);
            }}
            "#
        );

        let compilation = compilation(&source);
        let diagnostics = compilation.check_diagnostics();

        assert_eq!(
            !diagnostics.has_errors(),
            valid,
            "{source}: {diagnostics:#?}"
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn owned_postconditions_follow_a_scoped_mutable_alias() {
    for (body, valid) in [
        (
            "let mut other: Flags = { ready = false }; { let mut borrowed: &mut Flags = &mut value; if condition { borrowed = &mut other; } borrowed.ready = true; }",
            false,
        ),
        (
            "{ let borrowed: &mut Flags = &mut value; borrowed.ready = true; }",
            true,
        ),
        (
            "{ let borrowed: &mut Flags = &mut value; borrowed.ready = false; }",
            false,
        ),
        (
            "{ let borrowed: &mut Flags = &mut value; borrowed.ready = true; } value.ready = false;",
            false,
        ),
        (
            "let mut other: Flags = { ready = false }; { let borrowed: &mut Flags = &mut other; borrowed.ready = true; }",
            false,
        ),
        (
            "let mut other: Flags = { ready = false }; { let mut borrowed: &mut Flags = &mut value; borrowed = &mut other; borrowed.ready = true; }",
            false,
        ),
        (
            "{ let borrowed: &mut Flags = &mut value; let nested: &mut Flags = borrowed; nested.ready = true; }",
            true,
        ),
    ] {
        let source = format!(
            r#"
            module app;
            struct Flags {{ mut ready: bool; }}
            func root() {{
                let action = lambda(pos mut value: Flags, pos condition: bool) -> Flags
                    ensures(result.ready) executes(total) {{
                    {body}
                    return value;
                }};
            }}
            "#
        );

        let compilation = compilation(&source);
        let diagnostics = compilation.check_diagnostics();

        assert_eq!(
            !diagnostics.has_errors(),
            valid,
            "{source}: {diagnostics:#?}"
        );
    }
}

#[test]
fn production_spin_lock_rejects_fabricated_guard_authority() {
    for (guard, valid) in [("{ lock = &lock }", false), ("lock.lock()", true)] {
        let caller = format!(
            r#"
            trusted module app;
            using internal std.sync;
            func production_probe() {{
                let lock = std.sync.SpinLock<u32>(7);
                let mut guard: std.sync.SpinLockGuard<u32> = {guard};
                let value = guard.get();
            }}
            "#
        );

        let compilation = production_sync_compilation(&caller);

        let checked = compilation
            .lowered_unit(source_function_body_key(&compilation, "production_probe"))
            .unwrap();

        assert_eq!(
            !checked.diagnostics().has_errors(),
            valid,
            "{caller}: {:?}",
            checked.diagnostics()
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                checked.diagnostics(),
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

fn production_sync_compilation(caller: &str) -> Compilation {
    let sources = [
        include_str!("../../../../../standard-library/std/src/std.bray"),
        include_str!("../../../../../standard-library/std/src/memory.bray"),
        include_str!("../../../../../standard-library/std/src/sync.bray"),
        include_str!("../../../../../standard-library/std/src/sync/spin_lock.bray"),
        include_str!("../../../../../standard-library/std/src/sync/spin_lock/lock.bray"),
        include_str!("../../../../../standard-library/std/src/sync/spin_lock/guard.bray"),
        include_str!("../../../../../standard-library/std/src/sync/spin_lock/state.bray"),
        include_str!("../../../../../standard-library/std/src/sync/mutex.bray"),
        include_str!("../../../../../standard-library/std/src/sync/mutex/lock.bray"),
        include_str!("../../../../../standard-library/std/src/sync/mutex/guard.bray"),
        include_str!("../../../../../standard-library/std/src/sync/mutex/state.bray"),
        include_str!("../../../../../standard-library/std/src/atomic.bray"),
        include_str!("../../../../../standard-library/std/src/atomic/windows.bray"),
        include_str!("../../../../../standard-library/std/src/atomic/linux.bray"),
        include_str!("../../../../../standard-library/std/src/atomic/darwin.bray"),
        include_str!("../../../../../standard-library/std/src/os.bray"),
        include_str!("../../../../../standard-library/std/src/os/windows.bray"),
        include_str!("../../../../../standard-library/std/src/os/unix.bray"),
        include_str!("../../../../../standard-library/std/src/os/unix/thread.bray"),
        include_str!("../../../../../standard-library/std/src/os/unix/linux.bray"),
        include_str!("../../../../../standard-library/std/src/os/unix/darwin.bray"),
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
        include_str!("../../../../../standard-library/std/src/sync/rw_lock.bray"),
        include_str!("../../../../../standard-library/std/src/sync/rw_lock/lock.bray"),
        include_str!("../../../../../standard-library/std/src/sync/rw_lock/guard.bray"),
        include_str!("../../../../../standard-library/std/src/sync/rw_lock/state.bray"),
        include_str!("../../../../../standard-library/std/src/sync/rw_lock/read.bray"),
        include_str!("../../../../../standard-library/std/src/sync/rw_lock/write.bray"),
        include_str!("../../../../../standard-library/std/src/sync/condition.bray"),
        caller,
    ];

    let request = CompilationRequest::with_options(
        PackageIdentity::try_new("std").unwrap(),
        sources
            .iter()
            .enumerate()
            .map(|(index, source)| source_input(source, u32::try_from(index).unwrap()))
            .collect(),
        crate::CompilationOptions::new(
            crate::WorkerBudget::serial(),
            bray_symbols::ProductKind::Library,
            crate::SelectedTarget::baseline(),
        )
        .with_native_link_inputs(["c", "pthread", "Kernel32"].map(|name| {
            bray_symbols::NativeLinkRequirement::new(
                bray_base::NonEmptySharedStr::try_new(name).unwrap(),
                bray_symbols::NativeLinkKind::System,
            )
        })),
    )
    .with_standard_library_source_authority();

    let compilation = Compilation::load(request).unwrap();

    assert!(
        !compilation.syntax_tree_result().diagnostics().has_errors(),
        "{:?}",
        compilation.syntax_tree_result().diagnostics()
    );

    compilation
}

#[test]
fn nullable_acquisition_contract_follows_the_present_owner() {
    for (body, valid) in [
        (
            "let value = acquire(); match consume value { case ?guard { observe(guard); } case none {} }",
            true,
        ),
        (
            "let value = acquire(); match consume value { case ?guard { let mut changed = guard; changed.active = false; observe(changed); } case none {} }",
            false,
        ),
        (
            "let value = acquire(); match consume value { case ?guard { let fake: Guard = { active = guard.active }; observe(fake); } case none {} }",
            false,
        ),
        ("forward(acquire());", true),
        (
            "let value = acquire(); match consume value { case ?guard { observe(guard); observe(guard); } case none {} }",
            false,
        ),
        (
            "let guard: Guard = { active = true }; observe(guard);",
            false,
        ),
    ] {
        let source = format!(
            r#"
            trusted module app;
            struct Guard {{ mut active: bool; }}
            trusted predicate held(value: Guard?);
            trusted func acquire() -> Guard?
                ensures(trusted held(result)) {{
                let guard: Guard = {{ active = true }};
                return guard;
            }}
            func observe(pos guard: Guard) requires(trusted held(guard)) {{}}
            func forward(pos value: Guard?) requires(trusted held(value)) {{
                match consume value {{ case ?guard {{ observe(guard); }} case none {{}} }}
            }}
            func caller() {{ {body} }}
        "#
        );

        let compilation = compilation(&source);
        let diagnostics = compilation.check_diagnostics();

        assert_eq!(
            !diagnostics.has_errors(),
            valid,
            "{source}: {diagnostics:#?}"
        );
    }
}

#[test]
fn cleanup_keeps_completion_guarantees_for_represented_children() {
    for (guarantee, valid) in [("ensures(trusted live(&self.child))", true), ("", false)] {
        let source = format!(
            r#"
            trusted module app;
            trusted predicate live(value: &Child);
            struct Child {{
                value: u32;
                trusted destruct() requires(trusted live(&self)) {{}}
            }}
            struct Parent {{
                child: Child;
                trusted finalize() {guarantee} executes(total) {{}}
            }}
            func caller() {{ let value: Parent = {{ child = {{ value = 1 }} }}; }}
        "#
        );

        let compilation = compilation(&source);
        let diagnostics = compilation.check_diagnostics();

        assert_eq!(
            !diagnostics.has_errors(),
            valid,
            "{source}: {diagnostics:?}"
        );
    }
}
#[test]
fn nested_nullable_absence_does_not_satisfy_outer_absence() {
    for (value, valid) in [("none", true), ("inner", false)] {
        let source = format!(
            r#"
            module app;
            func absent(pos value: (u32?)?) requires(value matches none) executes(total) {{}}
            func caller() executes(total) {{
                let inner: u32? = none;
                let outer: (u32?)? = {value};
                absent(outer);
            }}
        "#
        );

        let compilation = compilation(&source);
        let diagnostics = compilation.check_diagnostics();

        assert_eq!(
            !diagnostics.has_errors(),
            valid,
            "{source}: {diagnostics:?}"
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                DiagnosticKind::CheckingExecutionGuaranteeNotProven,
            );
        }
    }
}
#[test]
fn nullable_call_coercions_preserve_presence() {
    for (argument, valid) in [("none", true), ("inner", false)] {
        let source = format!(
            r#"
            module app;
            func absent(pos value: (u32?)?) requires(value matches none) executes(total) {{}}
            func caller() executes(total) {{
                let inner: u32? = none;
                absent({argument});
            }}
        "#
        );

        let compilation = compilation(&source);
        let diagnostics = compilation.check_diagnostics();

        assert_eq!(
            !diagnostics.has_errors(),
            valid,
            "{source}: {diagnostics:?}"
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                DiagnosticKind::CheckingExecutionGuaranteeNotProven,
            );
        }
    }
}

#[test]
fn nullable_aggregate_elements_keep_their_outer_discriminant() {
    for (declaration, access) in [
        ("let values: ((u32?)?, u32) = (outer, marker);", "values.0"),
        ("let values: [(u32?)?; 1] = [outer];", "values[0]"),
        ("let values: [(u32?)?; 1] = [outer; 1];", "values[0]"),
    ] {
        for (condition, valid) in [
            ("result matches none", false),
            ("!(result matches none)", true),
        ] {
            let source = format!(
                r#"
                module app;
                func caller() -> (u32?)? ensures({condition}) {{
                    let inner: u32? = none;
                    let outer: (u32?)? = inner;
                    let marker: u32 = 0;
                    {declaration}
                    return {access};
                }}
            "#
            );

            let compilation = compilation(&source);

            let checked = compilation
                .execution_properties(source_function_body_key(&compilation, "caller"))
                .unwrap();

            let diagnostics = checked.diagnostics();

            assert_eq!(
                !diagnostics.has_errors(),
                valid,
                "{source}: {diagnostics:?}"
            );
        }
    }
}

#[test]
fn memory_evidence_uses_the_same_cleanup_state_as_authority() {
    for (cleanup, valid) in [
        ("{ let writer: Writer = { slot = &mut slot }; }", true),
        (
            "if flag { let writer: Writer = { slot = &mut slot }; } else { let writer: Writer = { slot = &mut slot }; }",
            true,
        ),
        (
            "if flag { let writer: Writer = { slot = &mut slot }; }",
            false,
        ),
    ] {
        let source = format!(
            r#"
            trusted module app;
            struct Writer {{
                slot: &mut Uninit<u32>;
                trusted destruct()
                    ensures(trusted core.memory.uninit_initialized<u32>(storage = &self.slot))
                {{
                    core.memory.uninit_write<u32>(self.slot, 42);
                }}
            }}
            trusted func caller(pos flag: bool) -> u32 uses(unchecked_init) {{
                let mut slot = core.memory.uninit<u32>();
                {cleanup}
                return core.memory.move_initialized<u32>(&mut slot);
            }}
        "#
        );

        let application = compilation(&source);
        let diagnostics = application.check_diagnostics();

        assert_eq!(
            !diagnostics.has_errors(),
            valid,
            "{source}: {diagnostics:#?}"
        );

        let flow = application
            .storage_flow(source_function_body_key(&application, "caller"))
            .unwrap();

        let decision = flow
            .value()
            .memory_operations()
            .iter()
            .find(|decision| {
                matches!(
                    flow.value()
                        .checked_memory_operation(decision.expression())
                        .map(|operation| operation.kind()),
                    Some(bray_bound_tree::CheckedMemoryOperationKind::UninitMove { .. })
                )
            })
            .expect("caller retains its protected move operation");

        assert_eq!(
            decision.status(),
            if valid {
                bray_bound_tree::MemoryOperationStatus::Valid
            } else {
                bray_bound_tree::MemoryOperationStatus::MissingTrustedEvidence
            }
        );
    }
}

#[test]
fn cleanup_guarantees_reach_following_blocks_and_replacement() {
    for (body, total, valid) in [
        (
            "{ let value: Producer = { tag = 0 }; } observe();",
            "executes(total)",
            true,
        ),
        (
            "{ let value: Producer = { tag = 0 }; } if branch { observe(); } else { observe(); }",
            "executes(total)",
            true,
        ),
        (
            "let mut value: Producer = { tag = 0 }; value = { tag = 0 }; observe();",
            "executes(total)",
            true,
        ),
        (
            "{ let value: Producer = { tag = 0 }; } observe();",
            "",
            true,
        ),
        (
            "{ let value: Producer = { tag = 0 }; } { let other: Interference = { tag = 0 }; } observe();",
            "executes(total)",
            false,
        ),
    ] {
        let source = format!(
            r#"
            trusted module app;
            trusted predicate ready();
            struct Producer {{ tag: u8; trusted destruct() ensures(trusted ready()) {total} {{}} }}
            struct Interference {{ tag: u8; destruct() executes(total) {{}} }}
            func observe() requires(trusted ready()) ensures(trusted ready()) executes(pure, total) {{}}
            func caller(pos branch: bool) {{ {body} }}
        "#
        );

        let compilation = compilation(&source);
        let diagnostics = compilation.check_diagnostics();

        assert_eq!(
            !diagnostics.has_errors(),
            valid,
            "{source}: {diagnostics:#?}"
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn backing_release_revokes_destructor_guarantees() {
    for (ty, valid) in [("u8", true), ("box[Heap] u8", false)] {
        let source = format!(
            r#"
            trusted module app;
            trusted predicate ready();
            struct Producer {{ payload: {ty}; trusted destruct() ensures(trusted ready()) executes(total) {{}} }}
            func observe() requires(trusted ready()) executes(pure, total) {{}}
            func caller(pos value: Producer) {{ {{ let released = value; }} observe(); }}
        "#
        );

        let compilation = compilation(&source);
        let diagnostics = compilation.check_diagnostics();

        assert_eq!(
            !diagnostics.has_errors(),
            valid,
            "{source}: {diagnostics:#?}"
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn task_cleanup_revokes_preceding_guarantees() {
    for (pending, valid) in [("", true), ("let pending = work().start();", false)] {
        let source = format!(
            r#"
            trusted module app;
            trusted predicate ready();
            async func work() {{}}
            trusted func establish() ensures(trusted ready()) executes(total) {{}}
            func observe() requires(trusted ready()) executes(pure, total) {{}}
            func caller() {{ {{ {pending} establish(); }} observe(); }}
        "#
        );

        let compilation = compilation(&source);
        let diagnostics = compilation.check_diagnostics();

        assert_eq!(
            !diagnostics.has_errors(),
            valid,
            "{source}: {diagnostics:#?}"
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn replacement_guarantees_follow_the_remaining_partition() {
    for (moved, valid) in [("interference", true), ("producer", false)] {
        let source = format!(
            r#"
            trusted module app;
            trusted predicate ready();
            struct Producer {{ tag: u8; trusted destruct() ensures(trusted ready()) executes(total) {{}} }}
            struct Interference {{ tag: u8; destruct() executes(total) {{}} }}
            struct Pair {{ interference: Interference; producer: Producer; }}
            func observe() requires(trusted ready()) executes(pure, total) {{}}
            func caller(pos mut value: Pair, pos replacement: Pair) {{
                let moved = value.{moved};
                value = replacement;
                observe();
            }}
        "#
        );

        let compilation = compilation(&source);
        let diagnostics = compilation.check_diagnostics();

        assert_eq!(
            !diagnostics.has_errors(),
            valid,
            "{source}: {diagnostics:#?}"
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn conditional_cleanup_cannot_publish_unconditional_guarantees() {
    for body in [
        "{ let value: Producer = { tag = 0 }; if flag { retained = value; } } observe();",
        "let mut value: Producer = { tag = 0 }; if flag { retained = value; } value = { tag = 0 }; observe();",
        "{ let value: Producer? = none; } observe();",
    ] {
        let source = format!(
            r#"
            trusted module app;
            trusted predicate ready();
            struct Producer {{ tag: u8; trusted destruct() ensures(trusted ready()) executes(total) {{}} }}
            func observe() requires(trusted ready()) executes(pure, total) {{}}
            func caller(pos flag: bool) {{ let mut retained: Producer? = none; {body} }}
        "#
        );

        let compilation = compilation(&source);

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            DiagnosticKind::CheckingTrustedObligationNotProven,
        );
    }
}

#[test]
fn production_spin_lock_nullable_acquisition_retains_storage() {
    let compilation = production_sync_compilation(
        r#"
        module app;
        using std.sync;
        func production_probe() {
            let lock = std.sync.SpinLock<u32>(7);
            let candidate = lock.try_lock();
            match consume candidate {
                case ?guard {
                    let mut held = guard;
                    let value = held.get();
                }
                case none {}
            }
        }
    "#,
    );

    let checked = compilation
        .lowered_unit(source_function_body_key(&compilation, "production_probe"))
        .unwrap();

    assert!(
        !checked.diagnostics().has_errors(),
        "{:?}",
        checked.diagnostics()
    );
}

#[test]
fn production_mutex_requires_acquisition_authority() {
    for (body, valid) in [
        (
            "let mut guard: std.sync.MutexGuard<u32> = { mutex = &lock }; let value = guard.get();",
            false,
        ),
        (
            "let mut guard = lock.lock(); let value = guard.get();",
            true,
        ),
        (
            "let candidate = lock.try_lock(); match consume candidate { case ?guard { let mut held = guard; let value = held.get(); } case none {} }",
            true,
        ),
    ] {
        let caller = format!(
            r#"
            trusted module app;
            using internal std.sync;
            func production_probe() requires(blocking_execution()) {{
                let lock = std.sync.Mutex<u32>(7);
                {body}
            }}
        "#
        );

        let compilation = production_sync_compilation(&caller);

        let checked = compilation
            .lowered_unit(source_function_body_key(&compilation, "production_probe"))
            .unwrap();

        assert_eq!(
            !checked.diagnostics().has_errors(),
            valid,
            "{caller}: {:?}",
            checked.diagnostics()
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
fn production_rw_lock_requires_distinct_acquisition_authority() {
    for (kind, method) in [("Read", "read"), ("Write", "write")] {
        for (guard, nullable, valid) in [
            ("{ lock = &lock }".to_owned(), false, false),
            (format!("lock.try_{method}()"), true, true),
            (format!("lock.{method}()"), false, true),
        ] {
            let use_guard = if nullable {
                "match consume candidate { case ?guard { let mut held = guard; let value = held.get(); } case none {} }".to_owned()
            } else {
                "let mut held = candidate; let value = held.get();".to_owned()
            };

            let suffix = if nullable { "?" } else { "" };

            let source = format!(
                r#"trusted module app; using internal std.sync;
                func production_probe() requires(blocking_execution()) {{
                    let lock = std.sync.RwLock<u32>(7);
                    let candidate: std.sync.RwLock{kind}Guard<u32>{suffix} = {guard};
                    {use_guard}
                }}"#
            );

            let compilation = production_sync_compilation(&source);

            let checked = compilation
                .lowered_unit(source_function_body_key(&compilation, "production_probe"))
                .unwrap();

            assert_eq!(
                !checked.diagnostics().has_errors(),
                valid,
                "{source}: {:?}",
                checked.diagnostics()
            );

            if !valid {
                bray_testing::assert_goal_state_diagnostic_kind(
                    checked.diagnostics(),
                    DiagnosticKind::CheckingTrustedObligationNotProven,
                );
            }
        }
    }
}

#[test]
fn production_synchronization_declarations_check_their_own_bodies() {
    let compilation = production_sync_compilation("module app;");

    for key in compilation.declared_unit_keys_for_test().unwrap() {
        if key.kind() != bray_bound_tree::BoundUnitKind::CallableBody {
            continue;
        }

        let source = key.source().syntax().source_id().raw();

        if !matches!(source, 4..=6 | 8..=10 | 28..=29 | 33) {
            continue;
        }

        let checked = compilation.lowered_unit(key.clone()).unwrap();

        assert!(
            !checked.diagnostics().has_errors(),
            "{key:?}: {:?}",
            checked.diagnostics()
        );
    }
}

#[test]
fn production_condition_variable_releases_and_reacquires_authority() {
    let compilation = production_sync_compilation(
        r#"
        module app;
        using std.sync;
        func production_probe() requires(blocking_execution()) {
            let mutex = std.sync.Mutex<u32>(7);
            let condition = std.sync.ConditionVariable();
            let mut guard = condition.wait(mutex.lock());
            let value = guard.get();
        }
    "#,
    );

    let checked = compilation
        .lowered_unit(source_function_body_key(&compilation, "production_probe"))
        .unwrap();

    assert!(
        !checked.diagnostics().has_errors(),
        "{:?}",
        checked.diagnostics()
    );
}

#[test]
fn consuming_cleanup_retains_only_proven_external_borrow_identity() {
    for (cleanup, valid) in [("", true), ("self.reference = self.other;", false)] {
        let source = format!(
            r#"
            trusted module app;
            struct Payload {{ value: bool; }}
            trusted predicate live(value: &Payload);
            trusted predicate acquired(value: &Guard);
            struct Guard {{
                mut reference: &Payload;
                other: &Payload;
                trusted destruct()
                    requires(trusted acquired(&self))
                    ensures(trusted live(self.reference)) {{ let reference = self.reference; {cleanup} }}
                consume func release()
                    requires(trusted acquired(&self))
                    ensures(trusted live(self.reference)) {{}}
            }}
        "#
        );

        let application = compilation(&source);
        let diagnostics = application.check_diagnostics();

        assert_eq!(
            !diagnostics.has_errors(),
            valid,
            "{source}: {diagnostics:#?}"
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn returned_address_origin_is_independent_of_its_lifetime_anchor() {
    let application = production_sync_compilation(
        r#"
        trusted module app;
        trusted func exact(pos owner: &bool, pos payload: &bool) -> &bool
            uses(raw_memory, unchecked_alias) {
            let pointer = std.memory.address_of<bool>(payload);
            return trusted std.memory.borrow_from<bool, bool>(owner, pointer);
        }
    "#,
    );

    let checked = application
        .lowered_unit(source_function_body_key(&application, "exact"))
        .unwrap();

    assert!(
        !checked.diagnostics().has_errors(),
        "{:?}",
        checked.diagnostics()
    );

    let callable = crate::test_support::source_function(&application, "exact");

    let context = application
        .binding_context(&application.state.cancellation)
        .unwrap();

    let dependencies = bray_binder::SymbolQueryProvider::resolve_symbol_query(
        &context,
        bray_symbols::SymbolQueryRequest::<bray_symbols::CallableResultDependenciesQuery>::new(
            callable.into(),
        ),
    )
    .unwrap();

    let template = application
        .semantic_value_store()
        .unwrap()
        .dependency_contract_template_data(*dependencies.value());

    assert_eq!(
        template.borrows(),
        &[bray_symbols::DependencyBorrowRelation::new(
            bray_symbols::DependencySubject::root(bray_symbols::DependencySubjectRoot::Parameter(
                bray_symbols::SymbolOrdinal::new(1)
            )),
            bray_symbols::DependencySubject::root(bray_symbols::DependencySubjectRoot::Result),
            bray_symbols::DependencyReferenceKind::Borrow,
        )]
    );
}

#[test]
fn production_guard_borrow_origin_reaches_the_protected_payload() {
    let application = production_sync_compilation("module app;");
    let symbols = application.symbol_graph().unwrap();

    let callable = symbols
        .type_callable_members()
        .iter()
        .find(|member| {
            member
                .syntax_anchor()
                .is_some_and(|anchor| anchor.source_id().raw() == 5)
                && symbols
                    .member_name(member.id().into())
                    .is_some_and(|name| name.as_str() == "get")
        })
        .expect("production guard get member")
        .id();

    let context = application
        .binding_context(&application.state.cancellation)
        .unwrap();

    let dependencies = bray_binder::SymbolQueryProvider::resolve_symbol_query(
        &context,
        bray_symbols::SymbolQueryRequest::<bray_symbols::CallableResultDependenciesQuery>::new(
            callable.into(),
        ),
    )
    .unwrap();

    let template = application
        .semantic_value_store()
        .unwrap()
        .dependency_contract_template_data(*dependencies.value());

    let relation = template
        .borrows()
        .iter()
        .find(|relation| {
            relation.exit().subject_root() == bray_symbols::DependencySubjectRoot::Result
        })
        .unwrap_or_else(|| {
            panic!("production payload borrow must have a checked origin: {template:#?}")
        });

    assert_eq!(
        relation.entry().subject_root(),
        bray_symbols::DependencySubjectRoot::Receiver
    );

    assert_eq!(relation.entry().projections().len(), 3, "{relation:#?}");
}

#[test]
fn opaque_carrier_predicates_are_revoked_by_referent_mutation() {
    for (mutation, valid) in [("", true), ("carrier.payload.value = 1;", false)] {
        let source = format!(
            r#"
            trusted module app;
            struct Payload {{ mut value: u8; }}
            struct Carrier {{ payload: &mut Payload; }}
            trusted predicate live(carrier: &Carrier);
            trusted func make(pos payload: &mut Payload) -> Carrier
                ensures(trusted live(&result)) executes(pure, total) {{
                return {{ payload = payload }};
            }}
            func observe(pos carrier: &Carrier)
                requires(trusted live(carrier)) executes(pure, total) {{}}
            func caller() {{
                let mut payload: Payload = {{ value = 0 }};
                let mut carrier = make(&mut payload);
                {mutation}
                observe(&carrier);
            }}
            "#
        );

        let application = compilation(&source);
        let diagnostics = application.check_diagnostics();

        assert_eq!(
            !diagnostics.has_errors(),
            valid,
            "{source}: {diagnostics:#?}"
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn immutable_guard_fields_keep_acquisition_permits_paired() {
    for (name, body, expected) in [
        (
            "acquired",
            "let guard = acquire(&first); observe(&guard);",
            None,
        ),
        (
            "fabricated",
            "let guard: Guard = { owner = &first, permit = Permit {} }; observe(&guard);",
            Some(DiagnosticKind::CheckingTrustedObligationNotProven),
        ),
        (
            "detached",
            "let original = acquire(&first); let guard: Guard = { owner = &second, permit = original.permit }; observe(&guard);",
            Some(DiagnosticKind::CheckingIncompleteLifecycleStorage),
        ),
        (
            "replaced source",
            "let mut original = acquire(&first); let guard: Guard = { owner = &second, permit = original.permit }; original = acquire(&first); observe(&guard);",
            Some(DiagnosticKind::CheckingIncompleteLifecycleStorage),
        ),
        (
            "whole move",
            "let original = acquire(&first); let guard = original; observe(&guard);",
            None,
        ),
        (
            "fresh detached",
            "let guard: Guard = { owner = &second, permit = acquire(&first).permit }; observe(&guard);",
            Some(DiagnosticKind::CheckingIncompleteLifecycleStorage),
        ),
        (
            "fabricated alongside live",
            "let original = acquire(&first); let guard: Guard = { owner = &second, permit = Permit {} }; observe(&guard); observe(&original);",
            Some(DiagnosticKind::CheckingTrustedObligationNotProven),
        ),
        (
            "restore immutable field",
            "let mut original = acquire(&first); let guard: Guard = { owner = &second, permit = original.permit }; original.permit = Permit {}; observe(&guard);",
            Some(DiagnosticKind::CheckingMissingMutationAuthority),
        ),
        (
            "consume pattern",
            "let original = acquire(&first); match consume original { case { owner, permit } { let guard: Guard = { owner = &second, permit = permit }; observe(&guard); } }",
            Some(DiagnosticKind::CheckingIncompleteLifecycleStorage),
        ),
    ] {
        let source = format!(
            r#"
            trusted module app;
            struct Lock {{ tag: u32; }}
            struct Permit {{}}
            trusted predicate held(permit: &Permit);
            struct Guard {{
                owner: &Lock;
                permit: Permit;
                destruct() requires(trusted held(&self.permit)) executes(pure, total) {{}}
            }}
            trusted func acquire(pos owner: &Lock) -> Guard
                ensures(trusted held(&result.permit)) {{
                return {{ owner = owner, permit = Permit {{}} }};
            }}
            func observe(pos guard: &Guard)
                requires(trusted held(&guard.permit)) executes(pure, total) {{}}
            func caller() {{
                let first: Lock = {{ tag = 1 }};
                let second: Lock = {{ tag = 2 }};
                {body}
            }}
        "#
        );

        let application = compilation(&source);
        let diagnostics = application.check_diagnostics();

        assert_eq!(
            !diagnostics.has_errors(),
            expected.is_none(),
            "{name}: {source}: {diagnostics:#?}"
        );

        if let Some(expected) = expected {
            bray_testing::assert_goal_state_diagnostic_kind(diagnostics, expected);
        }
    }
}

#[test]
fn acquisition_authority_belongs_to_the_complete_produced_carrier() {
    for (name, body, valid) in [
        (
            "whole fresh",
            "let guard = acquire(&first); observe(&guard);",
            true,
        ),
        (
            "whole moved",
            "let original = acquire(&first); let guard = original; observe(&guard);",
            true,
        ),
        (
            "fresh extracted",
            "let marker = acquire(&first).marker;",
            false,
        ),
        (
            "named extracted",
            "let guard = acquire(&first); let marker = guard.marker;",
            false,
        ),
        (
            "fresh reassembled",
            "let guard: Guard = { owner = &second, marker = acquire(&first).marker }; observe(&guard);",
            false,
        ),
        (
            "named reassembled",
            "let original = acquire(&first); let guard: Guard = { owner = &second, marker = original.marker }; observe(&guard);",
            false,
        ),
        (
            "fresh destructured",
            "let { owner, marker } = acquire(&first);",
            false,
        ),
        (
            "extract before divergence",
            "let marker = acquire(&first).marker; loop {}",
            false,
        ),
        (
            "fabricated",
            "let guard: Guard = { owner = &first, marker = Marker {} }; observe(&guard);",
            false,
        ),
    ] {
        let source = format!(
            r#"
            trusted module app;
            struct Lock {{ tag: u32; }}
            struct Marker {{}}
            struct Guard {{ owner: &Lock; marker: Marker; }}
            trusted predicate held(marker: &Marker);
            trusted func acquire(pos owner: &Lock) -> Guard
                ensures(trusted held(&result.marker)) executes(pure, total)
                {{ return {{ owner = owner, marker = Marker {{}} }}; }}
            func observe(pos guard: &Guard)
                requires(trusted held(&guard.marker)) executes(pure, total) {{}}
            func caller() {{
                let first: Lock = {{ tag = 1 }};
                let second: Lock = {{ tag = 2 }};
                {body}
            }}
        "#
        );

        let application = compilation(&source);
        let diagnostics = application.check_diagnostics();

        assert_eq!(
            !diagnostics.has_errors(),
            valid,
            "{name}: {source}: {diagnostics:#?}"
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                if name == "fabricated" {
                    DiagnosticKind::CheckingTrustedObligationNotProven
                } else {
                    DiagnosticKind::CheckingTrustedWitnessTransferNotProven
                },
            );
        }
    }
}

#[test]
fn independent_tuple_carriers_remain_whole_after_destructuring() {
    for (body, valid) in [
        (
            "let (first, second) = pair(); observe(&first); observe(&second);",
            true,
        ),
        ("let first = pair().0; observe(&first);", true),
        (
            "let first = { let owners = pair(); yield owners.0; }; observe(&first);",
            true,
        ),
        ("let marker = pair().0.marker;", false),
    ] {
        let source = format!(
            r#"
            trusted module app;
            struct Marker {{}}
            struct Guard {{ marker: Marker; }}
            trusted predicate held(marker: &Marker);
            trusted func pair() -> (Guard, Guard)
                ensures(trusted held(&result.0.marker), trusted held(&result.1.marker))
                executes(pure, total)
            {{ return ({{ marker = Marker {{}} }}, {{ marker = Marker {{}} }}); }}
            func observe(pos guard: &Guard)
                requires(trusted held(&guard.marker)) executes(pure, total) {{}}
            func caller() {{ {body} }}
        "#
        );

        let application = compilation(&source);
        let diagnostics = application.check_diagnostics();

        assert_eq!(
            !diagnostics.has_errors(),
            valid,
            "{source}: {diagnostics:#?}"
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                DiagnosticKind::CheckingTrustedWitnessTransferNotProven,
            );
        }
    }
}

#[test]
fn input_authority_uses_the_same_complete_carrier_as_the_producer() {
    for (result, body, valid) in [
        ("Guard", "return guard;", true),
        ("Marker", "return guard.marker;", false),
        (
            "Guard",
            "return { owner = other, marker = guard.marker };",
            false,
        ),
    ] {
        let source = format!(
            r#"
            trusted module app;
            struct Lock {{ tag: u32; }}
            struct Marker {{}}
            struct Guard {{ owner: &Lock; marker: Marker; }}
            trusted predicate held(marker: &Marker);
            func transfer(pos guard: Guard, pos other: &Lock) -> {result}
                requires(trusted held(&guard.marker)) {{ {body} }}
        "#
        );

        let application = compilation(&source);
        let diagnostics = application.check_diagnostics();

        assert_eq!(
            !diagnostics.has_errors(),
            valid,
            "{source}: {diagnostics:#?}"
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                DiagnosticKind::CheckingTrustedWitnessTransferNotProven,
            );
        }
    }
}

#[test]
fn field_mutation_preserves_only_independent_carrier_observations() {
    for (ty, result, input, valid) in [
        ("Marker", "&result.marker", "&guard.marker", true),
        ("Guard", "&result", "guard", false),
    ] {
        for mutation in [
            "guard.payload = true;",
            "if flag { guard.payload = true; }",
            "if flag { guard.payload = true; } else { guard.payload = false; }",
        ] {
            let source = format!(
                r#"
            trusted module app;
            struct Marker {{}}
            struct Guard {{ marker: Marker; mut payload: bool; }}
            trusted predicate held(value: &{ty});
            trusted func acquire() -> Guard
                ensures(trusted held({result})) executes(pure, total)
            {{ return {{ marker = Marker {{}}, payload = false }}; }}
            func observe(pos guard: &Guard)
                requires(trusted held({input})) executes(pure, total) {{}}
            func caller(pos flag: bool) {{
                let mut guard = acquire();
                {mutation}
                observe(&guard);
            }}
        "#
            );

            let application = compilation(&source);
            let diagnostics = application.check_diagnostics();

            assert_eq!(
                !diagnostics.has_errors(),
                valid,
                "{source}: {diagnostics:?}"
            );

            if !valid {
                bray_testing::assert_goal_state_diagnostic_kind(
                    diagnostics,
                    DiagnosticKind::CheckingTrustedObligationNotProven,
                );
            }
        }
    }
}

#[test]
fn joined_mutated_values_do_not_inherit_their_entry_contents() {
    for (mutation, valid) in [
        ("if flag { value.ready = false; }", false),
        (
            "if flag { value.ready = false; } else { value.ready = false; }",
            false,
        ),
        ("if flag { value.ready = true; }", true),
    ] {
        let source = format!(
            r#"
            module app;
            @copy struct State {{ mut ready: bool; }}
            func make() -> State ensures(result.ready) {{ return {{ ready = true }}; }}
            func update(pos flag: bool) -> State ensures(result.ready) {{
                let mut value = make();
                {mutation}
                return value;
            }}
        "#
        );

        let application = compilation(&source);

        let checked = application
            .execution_properties(crate::test_support::source_function_body_key(
                &application,
                "update",
            ))
            .unwrap();

        assert_eq!(
            !checked.diagnostics().has_errors(),
            valid,
            "{source}: {checked:?}"
        );
    }
}

#[test]
fn raw_writes_invalidate_only_the_reached_witness_observations() {
    for (field, ty, value, valid) in [
        ("payload", "u64", "1", true),
        ("lease.active", "bool", "false", false),
    ] {
        let source = format!(
            r#"
            trusted module app;
            trusted predicate live(value: &Lease);
            struct Lease {{ mut active: bool; }}
            struct Access {{ mut payload: u64; mut lease: Lease; }}
            trusted func acquire() -> Access
                ensures(trusted live(&result.lease)) {{
                return {{ payload = 0, lease = {{ active = true }} }};
            }}
            func observe(pos lease: &Lease)
                requires(trusted live(lease))
                ensures(trusted live(lease)) executes(pure, total) {{}}
            trusted func caller() uses(raw_memory, unchecked_init) {{
                let mut access = acquire();
                observe(&access.lease);
                let pointer = core.memory.address_of_mut<{ty}>(&mut access.{field});
                let alias = pointer;
                trusted core.memory.write<{ty}>(alias, {value});
                observe(&access.lease);
            }}
        "#
        );

        let compilation = compilation(&source);
        let diagnostics = compilation.check_diagnostics();

        assert_eq!(
            !diagnostics.has_errors(),
            valid,
            "{source}: {diagnostics:#?}"
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}
