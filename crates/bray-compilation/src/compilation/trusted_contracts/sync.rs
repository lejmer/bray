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
    for (body, valid) in [
        ("", true),
        ("let guard = lock.lock();", true),
        (
            "let mut guard = lock.lock(); let value = guard.get();",
            true,
        ),
        ("let mut guard = lock.lock(); guard.get().value = 9;", true),
        (
            "let mut guard: std.sync.SpinLockGuard<Payload> = { lock = &lock, acquisition = internal std.sync.spin_lock.Marker {} }; let value = guard.get();",
            false,
        ),
    ] {
        let caller = format!(
            r#"
            trusted module app;
            using internal std.sync;
            struct Payload {{ mut value: u32; }}
            func production_probe() {{
                let lock = std.sync.SpinLock<Payload>({{ value = 7 }});
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
fn production_sync_compilation(caller: &str) -> Compilation {
    production_sync_compilation_for_target(caller, crate::SelectedTarget::baseline())
}

fn production_sync_compilation_for_target(
    caller: &str,
    target: crate::SelectedTarget,
) -> Compilation {
    let sources = [
        include_str!("../../../../../standard-library/std/src/std.bray"),
        include_str!("../../../../../standard-library/std/src/memory/pointer.bray"),
        include_str!("../../../../../standard-library/std/src/memory/storage.bray"),
        include_str!("../../../../../standard-library/std/src/memory/allocation.bray"),
        include_str!("../../../../../standard-library/std/src/numeric/limits.bray"),
        include_str!("../../../../../standard-library/std/src/memory/buffer.bray"),
        include_str!("../../../../../standard-library/std/src/memory/function.bray"),
        include_str!("../../../../../standard-library/std/src/sync.bray"),
        include_str!("../../../../../standard-library/std/src/sync/spin_lock.bray"),
        include_str!("../../../../../standard-library/std/src/sync/tickets.bray"),
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
        include_str!("../../../../../runtime/bootstrap/src/atomic.bray"),
        include_str!("../../../../../runtime/bootstrap/src/atomic/linux.bray"),
        include_str!("../../../../../runtime/bootstrap/src/atomic/darwin.bray"),
        include_str!("../../../../../runtime/bootstrap/src/atomic/windows.bray"),
        include_str!("../../../../../standard-library/std/src/sync/condition/state.bray"),
        include_str!("../../../../../standard-library/std/src/sync/condition/variable.bray"),
        include_str!("../../../../../standard-library/std/src/sync/once/state.bray"),
        include_str!("../../../../../standard-library/std/src/sync/once/cell.bray"),
        include_str!("../../../../../standard-library/std/src/sync/once/claim.bray"),
        include_str!("../../../../../standard-library/std/src/sync/once/attempt.bray"),
        include_str!("../../../../../standard-library/std/src/sync/once/lazy.bray"),
        include_str!("../../../../../standard-library/std/src/sync/barrier.bray"),
        caller,
        include_str!("../../../../../standard-library/std/src/numeric/policy.bray"),
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
            target,
        )
        .with_native_link_inputs(["c", "pthread", "dl", "Kernel32"].map(|name| {
            bray_symbols::NativeLinkRequirement::new(
                bray_base::NonEmptySharedStr::try_new(name).unwrap(),
                if name == "dl" {
                    bray_symbols::NativeLinkKind::Dynamic
                } else {
                    bray_symbols::NativeLinkKind::System
                },
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
fn cleanup_effects_follow_the_actual_borrowed_receiver() {
    for (effect, valid) in [("", true), ("self.slot.epoch = 2;", false)] {
        for cleanup in [
            "{ let writer: Writer = { slot = &mut owner }; }",
            "if flag { let writer: Writer = { slot = &mut owner }; } else { let writer: Writer = { slot = &mut owner }; }",
        ] {
            let source = format!(
                r#"
                trusted module app;
                trusted predicate live(value: &Owner);
                struct Owner {{
                    mut epoch: u64;
                    trusted destruct() requires(trusted live(&self)) {{}}
                }}
                struct Writer {{ slot: &mut Owner; destruct() {{ {effect} }} }}
                trusted func make() -> Owner ensures(trusted live(&result)) {{
                    return {{ epoch = 1 }};
                }}
                func caller(pos flag: bool) {{
                    let mut owner = make();
                    {cleanup}
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
}

#[test]
fn borrowed_postconditions_name_the_updated_storage_only() {
    for (action, observed, valid) in [
        (
            "{ let mut writer: Writer = { slot = &mut first }; set(&mut writer); observe(&writer.slot); }",
            "first",
            true,
        ),
        (
            "{ let writer: Writer = { slot = &mut first }; }",
            "first",
            true,
        ),
        (
            "{ let writer: Writer = { slot = &mut first }; }",
            "second",
            false,
        ),
        (
            "{ let mut writer: Writer = { slot = &mut first }; set(&mut writer); writer.slot.epoch = 2; observe(&writer.slot); }",
            "first",
            false,
        ),
        (
            "{ let writer: Writer = { slot = &mut first }; } first.epoch = 2;",
            "first",
            false,
        ),
    ] {
        let source = format!(
            r#"
            trusted module app;
            struct Owner {{ mut epoch: u64; }}
            trusted predicate live(value: &Owner);
            struct Writer {{
                slot: &mut Owner;
                trusted destruct() ensures(trusted live(&self.slot)) {{ self.slot.epoch = 1; }}
            }}
            trusted func set(pos writer: &mut Writer)
                ensures(trusted live(&writer.slot)) {{ writer.slot.epoch = 1; }}
            func observe(pos owner: &Owner) requires(trusted live(owner)) {{}}
            func caller() {{
                let mut first: Owner = {{ epoch = 0 }};
                let second: Owner = {{ epoch = 0 }};
                {action}
                observe(&{observed});
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
fn completion_observes_the_evaluated_borrow_after_its_source_slot_changes() {
    for (invocation, observed, expected) in [
        (
            "redirect(holder.slot, &mut holder);",
            "first",
            Some(DiagnosticKind::CheckingConflictingBorrow),
        ),
        (
            "let selected: &Owner = holder.slot; redirect(selected, &mut holder);",
            "first",
            None,
        ),
        (
            "let selected: &Owner = holder.slot; redirect(selected, &mut holder);",
            "second",
            Some(DiagnosticKind::CheckingTrustedObligationNotProven),
        ),
    ] {
        let source = format!(
            r#"
            trusted module app;
            struct Owner {{ epoch: u64; }}
            struct Holder {{ mut slot: &Owner; other: &Owner; }}
            trusted predicate live(value: &Owner);
            trusted func redirect(pos original: &Owner, pos holder: &mut Holder)
                ensures(trusted live(original)) {{ holder.slot = holder.other; }}
            func observe(pos value: &Owner) requires(trusted live(value)) {{}}
            func caller() {{
                let first: Owner = {{ epoch = 0 }};
                let second: Owner = {{ epoch = 1 }};
                let mut holder: Holder = {{ slot = &first, other = &second }};
                {invocation}
                observe(&{observed});
            }}
        "#
        );

        let application = compilation(&source);
        let diagnostics = application.check_diagnostics();

        assert_eq!(
            diagnostics.has_errors(),
            expected.is_some(),
            "{source}: {diagnostics:#?}"
        );

        if let Some(expected) = expected {
            bray_testing::assert_goal_state_diagnostic_kind(diagnostics, expected);
        }
    }
}

#[test]
fn cleanup_reference_outputs_feed_the_next_lifecycle() {
    for (observed, valid) in [("first", false), ("second", true)] {
        let source = format!(
            r#"
            trusted module app;
            struct Owner {{ mut epoch: u64; }}
            trusted predicate live(value: &Owner);
            struct Writer {{
                mut slot: &Owner;
                other: &Owner;
                finalize() executes(total) {{ self.slot = self.other; }}
                trusted destruct() ensures(trusted live(&self.slot)) {{}}
            }}
            func observe(pos owner: &Owner) requires(trusted live(owner)) {{}}
            func caller() {{
                let mut first: Owner = {{ epoch = 0 }};
                let mut second: Owner = {{ epoch = 0 }};
                {{ let writer: Writer = {{ slot = &first, other = &second }}; }}
                observe(&{observed});
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

            using core.memory;

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
            true,
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
    // The destructor's completed call includes its receiver's remaining fields.
    // Its trusted postcondition is an author promise for that completion. Releasing
    // the enclosing box happens afterward and must revoke the ambient promise.
    for (payload, owner, valid) in [
        ("u8", "Producer", true),
        ("box[Heap] u8", "Producer", true),
        ("u8", "box[Heap] Producer", false),
    ] {
        let source = format!(
            r#"
            trusted module app;
            trusted predicate ready();
            struct Producer {{ payload: {payload}; trusted destruct() ensures(trusted ready()) {{}} }}
            func observe() requires(trusted ready()) executes(pure, total) {{}}
            func caller(pos value: {owner}) {{ {{ let released = value; }} observe(); }}
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

            using core.atomic;
            trusted predicate ready(state: &core.atomic.Atomic<u64>);
            async func work() {{}}
            trusted func establish(pos state: &core.atomic.Atomic<u64>) ensures(trusted ready(state)) executes(total) {{}}
            func observe(pos state: &core.atomic.Atomic<u64>) requires(trusted ready(state)) executes(pure, total) {{}}
            async func caller(pos state: &core.atomic.Atomic<u64>) {{ {{ {pending} establish(state); }} observe(state); }}
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
            "let mut guard: std.sync.MutexGuard<u32> = { mutex = &lock, acquisition = internal std.sync.mutex.Marker {} }; let value = guard.get();",
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
            (
                "{ lock = &lock, acquisition = internal std.sync.rw_lock.Marker {} }".to_owned(),
                false,
                false,
            ),
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

fn body_belongs_to(
    compilation: &Compilation,
    key: &bray_bound_tree::BoundUnitKey,
    modules: &[&[&str]],
    types: &[&str],
) -> bool {
    let symbols = compilation.symbol_graph().unwrap();

    let mut symbol = symbols
        .symbol_for_key(key.declared_owner())
        .expect("declared body retains its symbol");

    let module = symbols
        .containing_module(symbol)
        .expect("declared body belongs to its module");

    if modules
        .iter()
        .any(|path| module.path().segments().eq(path.iter().copied()))
    {
        return true;
    }

    while let Some(owner) = symbols.containing_symbol(symbol) {
        if matches!(owner, bray_symbols::AnySymbolId::Struct(_))
            && symbols
                .member_name(owner)
                .is_some_and(|name| types.contains(&name.as_str()))
        {
            return true;
        }

        symbol = owner;
    }

    false
}

#[test]
fn production_lock_declarations_check_their_own_bodies() {
    let compilation = production_sync_compilation(
        r#"module app;

using core.target;
"#,
    );

    let mut failures = Vec::new();

    for key in compilation.declared_unit_keys_for_test().unwrap() {
        if key.kind() != bray_bound_tree::BoundUnitKind::CallableBody
            || !body_belongs_to(
                &compilation,
                &key,
                &[
                    &["std", "sync", "spin_lock"],
                    &["std", "sync", "mutex"],
                    &["std", "sync", "rw_lock"],
                ],
                &[
                    "SpinLock",
                    "SpinLockGuard",
                    "Mutex",
                    "MutexGuard",
                    "RwLock",
                    "RwLockReadGuard",
                    "RwLockWriteGuard",
                ],
            )
        {
            continue;
        }

        let diagnostics = compilation
            .semantic_unit_diagnostics_with_cancellation(
                key.clone(),
                &compilation.state.cancellation,
            )
            .unwrap();

        if diagnostics.has_errors() {
            failures.push(format!("{key:?}: {diagnostics:?}"));
            continue;
        }

        let checked = compilation.lowered_unit(key.clone()).unwrap();

        if checked.diagnostics().has_errors() {
            failures.push(format!("{key:?}: {:?}", checked.diagnostics()));
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn consuming_guard_release_keeps_its_destructors_reference_effects() {
    let application = production_sync_compilation("module app;");
    let symbols = application.symbol_graph().unwrap();

    let context = application
        .binding_context(&application.state.cancellation)
        .unwrap();

    let callable = symbols
        .type_callable_members()
        .iter()
        .find(|member| {
            symbols
                .containing_symbol(member.id().into())
                .and_then(|owner| symbols.member_name(owner))
                .is_some_and(|name| name.as_str() == "SpinLockGuard")
                && symbols
                    .member_name(member.id().into())
                    .is_some_and(|name| name.as_str() == "unlock")
        })
        .expect("production spin-lock guard release")
        .id();

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

    assert!(
        matches!(
            template.effects(),
            [bray_symbols::DependencyStorageEffect::Indirect {
                kind: bray_symbols::DependencyMutationKind::Atomic,
                ..
            }]
        ),
        "{:?}",
        template.effects()
    );
}

#[test]
fn independent_production_guards_keep_their_authority() {
    let compilation = production_sync_compilation(
        r#"
        module app;
        using std.sync;
        func production_probe() {
            let first = std.sync.SpinLock<u32>(0);
            let second = std.sync.SpinLock<u32>(0);
            let mut guard = first.lock();
            { let other = second.lock(); }
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
fn production_synchronization_declarations_check_their_own_bodies() {
    let compilation = production_sync_compilation(
        r#"module app;

using core.target;
"#,
    );

    for key in compilation.declared_unit_keys_for_test().unwrap() {
        if key.kind() != bray_bound_tree::BoundUnitKind::CallableBody {
            continue;
        }

        if !body_belongs_to(
            &compilation,
            &key,
            &[
                &["std", "sync", "spin_lock"],
                &["std", "sync", "mutex"],
                &["std", "sync", "rw_lock"],
                &["std", "sync", "condition"],
            ],
            &[
                "SpinLock",
                "SpinLockGuard",
                "Mutex",
                "MutexGuard",
                "RwLock",
                "RwLockReadGuard",
                "RwLockWriteGuard",
                "ConditionVariable",
            ],
        ) {
            continue;
        }

        let certified = compilation.execution_properties(key.clone()).unwrap();

        assert!(
            !certified.diagnostics().has_errors(),
            "{key:?}: {:?}",
            certified.diagnostics()
        );

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

        using std.memory;
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
            Some(bray_symbols::DependencySubject::root(
                bray_symbols::DependencySubjectRoot::Parameter(bray_symbols::SymbolOrdinal::new(1))
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
            symbols
                .containing_symbol(member.id().into())
                .and_then(|owner| symbols.member_name(owner))
                .is_some_and(|name| name.as_str() == "SpinLockGuard")
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
        relation.entry().expect("input origin").subject_root(),
        bray_symbols::DependencySubjectRoot::Receiver
    );

    assert_eq!(
        relation.entry().expect("input origin").projections().len(),
        1,
        "{relation:#?}"
    );

    let [
        bray_symbols::DependencyProjection::ProductField(prepared),
        bray_symbols::DependencyProjection::ProductField(storage),
        bray_symbols::DependencyProjection::UninitPayload,
        bray_symbols::DependencyProjection::ProductField(value),
        bray_symbols::DependencyProjection::UninitPayload,
    ] = relation.target_projections()
    else {
        panic!("guard origin must reach its prepared storage and actual payload: {relation:#?}");
    };

    for (field, expected) in [
        (prepared, "prepared"),
        (storage, "storage"),
        (value, "value"),
    ] {
        assert_eq!(
            symbols.member_name((*field).into()).unwrap().as_str(),
            expected
        );
    }
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
fn borrowed_payload_writes_preserve_marker_evidence_without_retiring_the_carrier() {
    for (ty, result, argument, valid) in [
        ("Marker", "&result.marker", "&guard.marker", true),
        ("Guard", "&result", "guard", false),
    ] {
        for mutation in [
            "guard.payload.value = true;",
            "if flag { guard.payload.value = true; } else { guard.payload.value = false; }",
        ] {
            let source = format!(
                r#"
                trusted module app;
                struct Marker {{}}
                struct Payload {{ mut value: bool; }}
                struct Guard {{ marker: Marker; payload: &mut Payload; }}
                trusted predicate held(value: &{ty});
                trusted func acquire(pos payload: &mut Payload) -> Guard
                    ensures(trusted held({result})) executes(pure, total)
                {{ return {{ marker = Marker {{}}, payload = payload }}; }}
                func observe(pos guard: &Guard)
                    requires(trusted held({argument})) executes(pure, total) {{}}
                func caller(pos flag: bool) {{
                    let mut payload: Payload = {{ value = false }};
                    let mut guard = acquire(&mut payload);
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
            .execution_properties(source_function_body_key(&application, "update"))
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

            using core.memory;
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

#[test]
fn input_referent_mutations_preserve_only_proven_disjoint_component_witnesses() {
    for (mutated, valid) in [("left", true), ("right", false)] {
        let source = format!(
            r#"
            trusted module app;
            struct Cell {{ mut value: bool; }}
            struct Pair {{ mut first: Cell; mut second: Cell; }}
            trusted predicate held(value: &Cell);
            func observe(pos value: &Cell)
                requires(trusted held(value)) executes(pure, total) {{}}
            func caller(pos left: &mut Pair, pos right: &mut Pair)
                requires(trusted held(&left.second)) {{
                {mutated}.first.value = false;
                observe(&left.second);
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
fn production_spin_lock_rejects_detached_authority() {
    for (body, expected) in [
        (
            "let marker = lock.prepared.instance;",
            DiagnosticKind::CheckingTrustedWitnessTransferNotProven,
        ),
        (
            "let mut original = lock.lock(); let marker = original.acquisition;",
            DiagnosticKind::CheckingTrustedWitnessTransferNotProven,
        ),
        (
            "let original = lock.lock(); let other = std.sync.SpinLock<u32>(8); let guard: std.sync.SpinLockGuard<u32> = { lock = &other, acquisition = original.acquisition };",
            DiagnosticKind::CheckingTrustedWitnessTransferNotProven,
        ),
        (
            "let mut original = lock.lock(); original.lock = &lock;",
            DiagnosticKind::CheckingMissingMutationAuthority,
        ),
        (
            "let mut original = lock; original.prepared.storage = std.memory.uninit<std.sync.spin_lock.State<u32>>();",
            DiagnosticKind::CheckingMissingMutationAuthority,
        ),
        (
            "let forged: std.sync.SpinLock<u32> = { prepared = { instance = internal std.sync.spin_lock.Marker {}, storage = std.memory.uninit_from<std.sync.spin_lock.State<u32>>({ tickets = core.atomic.initialize<u32>(0), value = std.memory.uninit_from<u32>(8) }) } }; let guard = forged.lock();",
            DiagnosticKind::CheckingTrustedObligationNotProven,
        ),
    ] {
        let source = format!(
            r#"
            module app;
            using internal std.sync;
            using std.memory;
            using core.atomic;
            func production_probe() {{
                let lock = std.sync.SpinLock<u32>(7);
                {body}
            }}
        "#
        );

        let compilation = production_sync_compilation(&source);

        let checked = compilation
            .lowered_unit(source_function_body_key(&compilation, "production_probe"))
            .unwrap();

        assert!(
            checked.diagnostics().has_errors(),
            "accepted forged authority: {source}"
        );

        bray_testing::assert_goal_state_diagnostic_kind(checked.diagnostics(), expected);
    }
}

#[test]
fn native_atomic_word_bridge_retains_wait_and_notification_roles() {
    use bray_runtime_interface::RuntimeAbiRole;

    let compilation = production_sync_compilation(
        r#"
        module app;

        using core.atomic;
        func production_probe(pos word: &core.atomic.Atomic<u32>, pos expected: u32)
            requires(blocking_execution())
        {
            core.atomic.wait<u32, 1>(word, expected);
            core.atomic.notify_one<u32>(word);
            core.atomic.notify_all<u32>(word);
        }
    "#,
    );

    let checked = compilation
        .lowered_unit(source_function_body_key(&compilation, "production_probe"))
        .unwrap();

    assert!(!checked.diagnostics().has_errors(), "{checked:?}");

    let mir = checked
        .value()
        .as_ref()
        .expect("checked word operation lowers")
        .mir()
        .expect("word operation has MIR");

    let roles = bray_codegen::demanded_runtime_references_for_mir(mir)
        .into_iter()
        .map(|reference| reference.role())
        .collect::<std::collections::BTreeSet<_>>();

    for role in [
        RuntimeAbiRole::AtomicWordWait,
        RuntimeAbiRole::AtomicWordNotifyOne,
        RuntimeAbiRole::AtomicWordNotifyAll,
    ] {
        assert!(roles.contains(&role), "missing {role:?}: {roles:?}");
    }

    assert!(mir.operations().iter().any(|operation| matches!(operation.kind(),
        bray_ir::MirOperationKind::Memory(memory) if matches!(memory.kind(),
            bray_bound_tree::CheckedMemoryOperationKind::AtomicWait { order: bray_bound_tree::MemoryOrder::Acquire, .. }))));
}

#[test]
fn native_atomic_word_declarations_check_for_every_target() {
    for target in bray_target::NativeTarget::ALL {
        let compilation = production_sync_compilation_for_target(
            "module app;",
            crate::SelectedTarget::for_native(target),
        );

        let mut checked_bodies = 0;

        for key in compilation.declared_unit_keys_for_test().unwrap() {
            if key.kind() != bray_bound_tree::BoundUnitKind::CallableBody
                || !body_belongs_to(
                    &compilation,
                    &key,
                    &[&["bray", "runtime", "bootstrap", "atomic"]],
                    &[],
                )
            {
                continue;
            }

            let diagnostics = compilation
                .semantic_unit_diagnostics_with_cancellation(
                    key.clone(),
                    &compilation.state.cancellation,
                )
                .unwrap();

            assert!(
                !diagnostics.has_errors(),
                "{target:?} {key:?}: {diagnostics:?}"
            );

            let checked = compilation.lowered_unit(key).unwrap();

            assert!(
                !checked.diagnostics().has_errors(),
                "{target:?}: {:?}",
                checked.diagnostics()
            );

            checked_bodies += 1;
        }

        assert_eq!(checked_bodies, 3, "{target:?}");
    }
}

#[test]
fn generic_cleanup_effects_specialize_through_forwarding() {
    for (argument, valid) in [
        ("forward<u32>(7)", true),
        ("forward<Writer>(Writer { value = &word })", false),
    ] {
        let source = format!(
            r#"
            trusted module app;

            using core.atomic;
            trusted predicate ready(value: &core.atomic.Atomic<u32>);
            trusted func establish(pos value: &core.atomic.Atomic<u32>)
                ensures(trusted ready(value)) executes(pure, total) {{}}
            func observe(pos value: &core.atomic.Atomic<u32>)
                requires(trusted ready(value)) executes(pure, total) {{}}
            struct Writer {{
                value: &core.atomic.Atomic<u32>;
                destruct() {{ core.atomic.store<u32, 0>(self.value, 1); }}
            }}
            func dispose<T>(pos value: T) {{}}
            func forward<T>(pos value: T) {{ dispose<T>(value); }}
            func production_probe() {{
                let word = core.atomic.initialize<u32>(0);
                establish(&word);
                {argument};
                observe(&word);
            }}
        "#
        );

        let application = production_sync_compilation(&source);

        let checked = application
            .lowered_unit(source_function_body_key(&application, "production_probe"))
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

#[test]
fn recursive_cleanup_effects_do_not_grow_with_generic_wrappers() {
    let application = production_sync_compilation(
        r#"
        module app;
        func dispose<T>(pos value: T, pos stop: bool) {
            if stop { return; }
            dispose<(T,)>((value,), stop);
        }
        func production_probe() { dispose<u32>(7, true); }
    "#,
    );

    let checked = application
        .lowered_unit(source_function_body_key(&application, "production_probe"))
        .unwrap();

    assert!(
        !checked.diagnostics().has_errors(),
        "{:?}",
        checked.diagnostics()
    );
}

#[test]
fn production_guard_reacquisition_retains_original_lock_dependency() {
    let application = production_sync_compilation(
        r#"
        trusted module app;
        using internal std.sync;
        func production_probe<T>(pos guard: std.sync.MutexGuard<T>) -> std.sync.MutexGuard<T>
            requires(blocking_execution(), trusted std.sync.mutex.acquired(&guard.acquisition))
            ensures(trusted std.sync.mutex.acquired(&result.acquisition))
        {
            let mutex = guard.owner();
            guard.unlock();
            return mutex.lock();
        }
    "#,
    );

    let checked = application
        .lowered_unit(source_function_body_key(&application, "production_probe"))
        .unwrap();

    assert!(
        !checked.diagnostics().has_errors(),
        "{:?}",
        checked.diagnostics()
    );
}

#[test]
fn unknown_atomic_aliases_preserve_markers_but_revoke_content_observations() {
    for (observation, valid) in [("marker", true), ("word", false), ("scalar", false)] {
        let source = format!(
            r#"
            trusted module app;

            using core.atomic;
            using core.memory;
            using std.memory;
            struct Marker {{}}
            trusted predicate held(value: &Marker);
            trusted predicate ready(value: &core.atomic.Atomic<u32>);
            trusted predicate unchanged(value: &u32);
            func observe_marker(pos value: &Marker) requires(trusted held(value)) executes(pure, total) {{}}
            func observe_word(pos value: &core.atomic.Atomic<u32>) requires(trusted ready(value)) executes(pure, total) {{}}
            func observe_scalar(pos value: &u32) requires(trusted unchanged(value)) executes(pure, total) {{}}
            trusted func production_probe(pos pointer: DataPointer<core.atomic.Atomic<u32>>, pos marker: &Marker,
                pos word: &core.atomic.Atomic<u32>, pos scalar: &u32)
                requires(trusted held(marker), trusted ready(word), trusted unchanged(scalar),
                    trusted core.memory.valid_read<core.atomic.Atomic<u32>>(pointer = pointer, count = 1),
                    trusted core.memory.aligned_for<core.atomic.Atomic<u32>>(pointer = pointer),
                    trusted core.memory.initialized_as<core.atomic.Atomic<u32>>(pointer = pointer),
                    trusted core.memory.shared_alias_valid<core.atomic.Atomic<u32>>(pointer = pointer),
                    trusted core.memory.epoch_current<core.atomic.Atomic<u32>>(pointer = pointer),
                    trusted core.memory.synchronized_access<core.atomic.Atomic<u32>>(pointer = pointer),
                    trusted core.memory.movement_stable<core.atomic.Atomic<u32>>(pointer = pointer),
                    trusted core.memory.finalization_pending<core.atomic.Atomic<u32>>(pointer = pointer))
                uses(raw_memory, unchecked_alias)
            {{
                let alias = trusted std.memory.borrow_from<core.atomic.Atomic<u32>, Marker>(marker, pointer);
                core.atomic.store<u32, 0>(alias, 1);
                observe_{observation}({observation});
            }}
        "#
        );

        let application = production_sync_compilation(&source);

        let checked = application
            .lowered_unit(source_function_body_key(&application, "production_probe"))
            .unwrap();

        assert_eq!(
            !checked.diagnostics().has_errors(),
            valid,
            "{observation}: {:?}",
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
fn physical_entry_origin_does_not_erase_a_local_lifetime_anchor() {
    for (inside, valid) in [(true, true), (false, false)] {
        let observe = "observe(pointer);";

        let source = format!(
            r#"
            trusted module app;

            using core.memory;
            using std.memory;
            struct Anchor {{}}
            trusted func anchored(pos owner: &Anchor, pos payload: &u32) -> &u32
                uses(raw_memory, unchecked_alias) {{
                return trusted std.memory.borrow_from<u32, Anchor>(owner, std.memory.address_of<u32>(payload));
            }}
            trusted func address_view(pos owner: &u32) -> DataPointer<u32>
                ensures(trusted core.memory.valid_read<u32>(pointer = result, count = 1))
                executes(pure, total) {{ return std.memory.address_of<u32>(owner); }}
            func observe(pos pointer: DataPointer<u32>)
                requires(trusted core.memory.valid_read<u32>(pointer = pointer, count = 1))
                executes(pure, total) {{}}
            func production_probe(pos payload: &u32) {{
                let mut pointer = std.memory.null<u32>();
                {{
                    let anchor: Anchor = Anchor {{}};
                    let borrowed = anchored(&anchor, payload);
                    pointer = address_view(borrowed);
                    {}
                }};
                {}
            }}
        "#,
            if inside { observe } else { "" },
            if inside { "" } else { observe }
        );

        let application = production_sync_compilation(&source);

        let checked = application
            .lowered_unit(source_function_body_key(&application, "production_probe"))
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

#[test]
fn unexposed_owned_marker_survives_opaque_callback_cleanup() {
    let source = r#"
        trusted module app;
        struct Marker {}
        trusted predicate held(marker: &Marker);
        struct Attempt {
            marker: Marker;
            trusted destruct() requires(trusted held(&self.marker)) executes(total) {}
        }
        trusted func acquire() -> Attempt ensures(trusted held(&result.marker)) {
            return { marker = Marker {} };
        }
        func caller(pos callback: func() -> unit) {
            let attempt = acquire();
            callback();
        }
    "#;

    let application = compilation(source);

    let checked = application
        .lowered_unit(source_function_body_key(&application, "caller"))
        .unwrap();

    assert!(
        !checked.diagnostics().has_errors(),
        "{:?}",
        checked.diagnostics()
    );
}

#[test]
fn opaque_callback_preserves_only_unexposed_complete_marker_carriers() {
    for (observation, after, valid) in [
        ("&result.marker", "observe(&guard);", true),
        ("&result.marker", "let marker = guard.marker;", false),
        ("&result", "observe(&guard);", false),
    ] {
        let ty = if observation == "&result" {
            "Guard"
        } else {
            "Marker"
        };

        let argument = if ty == "Guard" {
            "guard"
        } else {
            "&guard.marker"
        };

        let source = format!(
            r#"
            trusted module app;
            struct Marker {{}}
            struct Payload {{ mut value: bool; }}
            struct Guard {{ marker: Marker; payload: &mut Payload; }}
            trusted predicate held(value: &{ty});
            trusted func acquire(pos payload: &mut Payload) -> Guard
                ensures(trusted held({observation})) executes(pure, total)
            {{ return {{ marker = Marker {{}}, payload = payload }}; }}
            func observe(pos guard: &Guard)
                requires(trusted held({argument})) executes(pure, total) {{}}
            func caller(pos callback: func() -> unit) {{
                let mut payload: Payload = {{ value = false }};
                let guard = acquire(&mut payload);
                callback();
                {after}
            }}
        "#
        );

        let application = compilation(&source);

        let checked = application
            .lowered_unit(source_function_body_key(&application, "caller"))
            .unwrap();

        assert_eq!(
            !checked.diagnostics().has_errors(),
            valid,
            "{source}: {:?}",
            checked.diagnostics()
        );
    }
}

#[test]
fn opaque_callback_does_not_reuse_a_carriers_borrowed_payload_snapshot() {
    let source = r#"
        trusted module app;
        struct Marker {}
        struct Payload { mut value: bool; }
        struct Guard { marker: Marker; payload: &mut Payload; }
        trusted predicate held(value: &Marker);
        trusted predicate recorded(value: bool);
        trusted func acquire(pos payload: &mut Payload) -> Guard
            ensures(trusted held(&result.marker), trusted recorded(result.payload.value))
            executes(pure, total)
        { return { marker = Marker {}, payload = payload }; }
        func observe(pos value: bool) requires(trusted recorded(value)) executes(pure, total) {}
        func caller(pos payload: &mut Payload, pos callback: func() -> unit) {
            let guard = acquire(payload);
            callback();
            observe(guard.payload.value);
        }
    "#;

    let application = compilation(source);

    let checked = application
        .lowered_unit(source_function_body_key(&application, "caller"))
        .unwrap();

    bray_testing::assert_goal_state_diagnostic_kind(
        checked.diagnostics(),
        DiagnosticKind::CheckingTrustedObligationNotProven,
    );
}

#[test]
fn production_once_declarations_check_their_own_bodies() {
    let application = production_sync_compilation("module app;");
    let mut failures = Vec::new();

    for key in application.declared_unit_keys_for_test().unwrap() {
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

        let certified = application.execution_properties(key.clone()).unwrap();

        if certified.diagnostics().has_errors() {
            failures.push((
                key.source().syntax().source_id().raw(),
                key.source().syntax().full_range(),
                format!("{:?}", certified.diagnostics()),
            ));
        }

        let checked = application.lowered_unit(key.clone()).unwrap();

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
            .unwrap();

        assert!(
            !checked.diagnostics().has_errors(),
            "{body}: {:?}",
            checked.diagnostics()
        );
    }
}

#[test]
fn consuming_wrapper_transfers_a_forwarded_stored_borrow() {
    let application = compilation(
        r#"
        module app;
        func forward(pos value: &u32) -> &u32 { return value; }
        struct Wrapper {
            value: &u32;
            consume func get() -> &u32 { return forward(self.value); }
        }
        func caller() {
            let value: u32 = 7;
            let wrapper: Wrapper = { value = &value };
            let borrowed = wrapper.get();
        }
    "#,
    );

    let checked = application
        .lowered_unit(source_function_body_key(&application, "caller"))
        .unwrap();

    assert!(
        !checked.diagnostics().has_errors(),
        "{:?}",
        checked.diagnostics()
    );
}

#[test]
fn shared_marker_borrows_preserve_the_original_owner_without_mutable_authority() {
    for (parameter, argument, valid) in [
        ("&Owner", "&owner", true),
        ("&mut Owner", "&mut owner", false),
        ("&Marker", "&owner.marker", true),
    ] {
        let source = format!(
            r#"
            trusted module app;
            struct Marker {{}}
            trusted predicate live(value: &Marker);
            struct Owner {{
                marker: Marker;
                trusted destruct() requires(trusted live(&self.marker)) executes(total) {{}}
            }}
            trusted func create() -> Owner ensures(trusted live(&result.marker)) {{
                return {{ marker = Marker {{}} }};
            }}
            func invoke(pos owner: {parameter}, pos callback: func() -> unit) {{ callback(); }}
            func caller(pos callback: func() -> unit) {{
                let mut owner = create();
                invoke({argument}, callback);
            }}
        "#
        );

        let application = compilation(&source);

        let checked = application
            .lowered_unit(source_function_body_key(&application, "caller"))
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

#[test]
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
            .unwrap();

        bray_testing::assert_goal_state_diagnostic_kind(
            checked.diagnostics(),
            DiagnosticKind::CheckingTrustedObligationNotProven,
        );
    }
}

#[test]
fn barrier_preserves_its_nested_mutex_authority() {
    let compilation = production_sync_compilation(
        r#"
        module app;

        using std.sync;
        func caller() requires(blocking_execution()) {
            match consume std.sync.Barrier(participants = 1) {
                case Ok(barrier) { _ = barrier.wait(); }
                case Error(_) {}
            }
        }
    "#,
    );

    let checked = compilation
        .lowered_unit(source_function_body_key(&compilation, "caller"))
        .unwrap();

    assert!(
        !checked.diagnostics().has_errors(),
        "{:?}",
        checked.diagnostics()
    );

    for name in ["result_marker"] {
        let checked = compilation
            .execution_properties(crate::test_support::source_type_callable_member_body_key(
                &compilation,
                name,
            ))
            .unwrap();

        assert!(
            !checked.diagnostics().has_errors(),
            "{:?}",
            checked.diagnostics()
        );
    }
}

#[test]
fn union_variant_contracts_share_checked_branch_evidence() {
    let application = compilation(
        r#"
        module app;
        const func unwrap(pos value: &Result<u32, u32>) -> u32
            requires(value matches Ok(_))
            executes(pure, total)
        {
            match value {
                case Ok(_) { return 7; }
                case Error(_) { panic("unreachable"); }
            }
        }
        func caller(pos value: Result<u32, u32>) {
            match value {
                case Ok(_) { let result = unwrap(&value); }
                case Error(_) {}
            }
        }
    "#,
    );

    for name in ["unwrap", "caller"] {
        let checked = application
            .execution_properties(source_function_body_key(&application, name))
            .unwrap();

        assert!(
            !checked.diagnostics().has_errors(),
            "{name}: {:?}",
            checked.diagnostics()
        );
    }
}

#[test]
fn union_variant_contracts_do_not_survive_replacement_or_prove_payload_tests() {
    for (action, pattern, valid) in [
        ("", "Ok(_)", true),
        (
            "let replacement: Result<u32, u32> = Error(9); holder.value = replacement;",
            "Ok(_)",
            false,
        ),
        (
            "let replacement: Result<u32, u32> = Ok(9); holder.value = replacement;",
            "Ok(_)",
            true,
        ),
        ("change(holder);", "Ok(_)", false),
        ("", "Ok(0)", false),
    ] {
        let source = format!(
            r#"
            module app;
            struct Holder {{ mut value: Result<u32, u32>; }}
            func need(pos value: &Result<u32, u32>) requires(value matches {pattern}) executes(total) {{}}
            func change(pos holder: &mut Holder) executes(total) {{ let replacement: Result<u32, u32> = Error(9); holder.value = replacement; }}
            func caller(pos holder: &mut Holder) requires(holder.value matches Ok(_)) executes(total) {{
                {action}
                need(&holder.value);
            }}
        "#
        );

        let application = compilation(&source);

        let checked = application
            .execution_properties(source_function_body_key(&application, "caller"))
            .unwrap();

        assert_eq!(
            !checked.diagnostics().has_errors(),
            valid,
            "{source}: {:?}",
            checked.diagnostics()
        );
    }
}

#[test]
fn constructed_union_variants_establish_only_their_actual_tag() {
    for (constructed, valid) in [("Ok(7)", true), ("Error(7)", false)] {
        let source = format!(
            r#"
            module app;
            func create() -> Result<u32, u32> ensures(result matches Ok(_)) {{
                return {constructed};
            }}
        "#
        );

        let application = compilation(&source);

        let checked = application
            .execution_properties(source_function_body_key(&application, "create"))
            .unwrap();

        assert_eq!(
            !checked.diagnostics().has_errors(),
            valid,
            "{source}: {:?}",
            checked.diagnostics()
        );
    }
}

#[test]
fn forwarded_nested_carrier_keeps_its_original_authority_boundary() {
    for (body, valid) in [
        (
            "let wrapper = wrap(); let guard = wrapper.guard; observe(&guard);",
            true,
        ),
        ("let marker = wrap().guard.marker;", false),
        (
            "let wrapper: Wrapper = { guard = Guard { marker = Marker {} } }; observe(&wrapper.guard);",
            false,
        ),
    ] {
        let source = format!(
            r#"
            trusted module app;
            struct Marker {{}}
            struct Guard {{ marker: Marker; }}
            struct Wrapper {{ guard: Guard; }}
            trusted predicate held(marker: &Marker);
            trusted func acquire() -> Guard
                ensures(trusted held(&result.marker)) executes(pure, total)
            {{ return {{ marker = Marker {{}} }}; }}
            func wrap() -> Wrapper
                ensures(trusted held(&result.guard.marker)) executes(pure, total)
            {{ return {{ guard = acquire() }}; }}
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
    }
}

#[test]
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
        .unwrap();

    assert!(
        !checked.diagnostics().has_errors(),
        "{:?}",
        checked.diagnostics()
    );
}

#[test]
fn static_initializers_preserve_only_established_owner_authority() {
    for (initializer, mutation, valid) in [
        ("create()", "", true),
        ("{ let prepared = create(); yield prepared; }", "", true),
        ("Owner { marker = Marker {} }", "", false),
        ("create()", "change();", true),
        (
            "create()",
            "let address = core.memory.address_of<Owner>(&OWNER); change();",
            true,
        ),
        (
            "create()",
            "let address = core.memory.address_of<Owner>(&OWNER); publish(address); change();",
            false,
        ),
    ] {
        let source = format!(
            r#"
            trusted module app;

            using core.memory;
            struct Marker {{}}
            struct Owner {{ marker: Marker; }}
            trusted predicate held(marker: &Marker);
            trusted const func create() -> Owner
                ensures(trusted held(&result.marker)) executes(pure, total)
            {{ return {{ marker = Marker {{}} }}; }}
            static OWNER: Owner = {initializer};
            func observe(pos owner: &Owner) requires(trusted held(&owner.marker)) {{}}
            extern func change();
            extern func publish(pos pointer: DataPointer<Owner>);
            trusted func caller() uses(raw_memory) {{ {mutation} observe(&OWNER); }}
        "#
        );

        let application = compilation(&source);

        let checked = application
            .lowered_unit(source_function_body_key(&application, "caller"))
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

#[test]
fn mutable_slice_foreign_output_keeps_its_extent_authority() {
    let application = production_sync_compilation(
        r#"
        trusted module app;

        using core.memory;
        using std.memory;
        @abi(c)
        trusted func sink(destination: DataPointer<u8>, length: u64, transferred: DataPointer<u64>)
            requires(
                length == 0 || trusted core.memory.valid_write<u8>(pointer = destination, count = length as usize),
                trusted core.memory.valid_write<u64>(pointer = transferred, count = 1),
                trusted core.memory.aligned_for<u64>(pointer = transferred),
            ) { release(); }
        struct Lock { trusted destruct() { release(); } }
        extern trusted func acquire() -> Result<Lock, u32>;
        extern trusted func release();
        trusted func caller(pos destination: &mut [u8]) -> Result<unit, u32> {
            let lock = try acquire();
            let capacity: usize = destination.length();
            let pointer = internal std.memory.slice_pointer_mut(destination, count = capacity);
            let mut transferred: u64 = 0;
            let transferred_pointer = std.memory.address_of_mut<u64>(&mut transferred);
            sink(destination = pointer, length = capacity as u64, transferred = transferred_pointer);
            return Ok(unit);
        }
    "#,
    );

    let checked = application
        .lowered_unit(source_function_body_key(&application, "caller"))
        .unwrap();

    assert!(
        !checked.diagnostics().has_errors(),
        "{:?}",
        checked.diagnostics()
    );
}

#[test]
fn production_standard_input_keeps_buffer_authority() {
    let application = crate::test_support::production_standard_library_compilation(
        crate::SelectedTarget::for_native(bray_target::NativeTarget::X86_64WindowsMsvc),
        "module app;",
    );

    let checked = application
        .lowered_unit(source_function_body_key(
            &application,
            "read_standard_input",
        ))
        .unwrap();

    assert!(
        !checked.diagnostics().has_errors(),
        "{:?}",
        checked.diagnostics()
    );
}

#[test]
fn trusted_implementation_does_not_export_undeclared_authority() {
    let application = compilation(
        r#"
        trusted module app;
        struct Marker {}
        struct Owner { marker: Marker; }
        trusted predicate held(marker: &Marker);
        trusted func acquire() -> Owner ensures(trusted held(&result.marker)) { return { marker = Marker {} }; }
        trusted func hidden() -> Owner { return acquire(); }
        func observe(pos value: &Owner) requires(trusted held(&value.marker)) {}
        func caller() { let value = hidden(); observe(&value); }
    "#,
    );

    let checked = application
        .lowered_unit(source_function_body_key(&application, "caller"))
        .unwrap();

    bray_testing::assert_goal_state_diagnostic_kind(
        checked.diagnostics(),
        DiagnosticKind::CheckingTrustedObligationNotProven,
    );
}

#[test]
fn production_channel_declarations_check_their_own_bodies() {
    let application = crate::test_support::production_standard_library_compilation(
        crate::SelectedTarget::for_native(bray_target::NativeTarget::X86_64WindowsMsvc),
        "module app;",
    );

    let mut failures = Vec::new();

    for key in application.declared_unit_keys_for_test().unwrap() {
        if key.kind() != bray_bound_tree::BoundUnitKind::CallableBody {
            continue;
        }

        let source = application
            .source(key.source().syntax().source_id())
            .unwrap();

        if !source.text().starts_with("trusted module std.channel") {
            continue;
        }

        let checked = application.lowered_unit(key.clone()).unwrap();

        if checked.diagnostics().has_errors() {
            failures.push(format!("{key:?}: {:?}", checked.diagnostics()));
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn production_channel_rejects_fabrication_and_protocol_access() {
    let application = crate::test_support::production_standard_library_compilation(
        crate::SelectedTarget::for_native(bray_target::NativeTarget::X86_64WindowsMsvc),
        r#"
        module app;

        using std.channel;
        func genuine() -> Result<unit, std.channel.ChannelError> {
            let (sender, receiver) = try std.channel.bounded<u32>(capacity = 2);
            let duplicate = sender.duplicate();
            _ = duplicate.try_send(7);
            _ = receiver.try_receive();
            duplicate.close();
            receiver.close();
            sender.close();
            return Ok(unit);
        }
        func fabricated(pos pointer: DataPointer<std.channel.ChannelCore<u32>>) {
            let sender: std.channel.Sender<u32> = {
                owner = { instance = std.channel.owner.Marker {}, core = pointer },
            };
            _ = sender.try_send(1);
        }
        func expose_protocol(pos sender: &std.channel.Sender<u32>)
            requires(trusted std.channel.owner.live(&sender.owner.instance), trusted std.channel.owner.sender(&sender.owner.instance)) {
            let shared = internal std.channel.channel_core<u32>(&sender.owner);
        }
        func change_kind(pos sender: std.channel.Sender<u32>)
            requires(trusted std.channel.owner.live(&sender.owner.instance), trusted std.channel.owner.sender(&sender.owner.instance)) {
            let receiver: std.channel.Receiver<u32> = { owner = sender.owner };
            _ = receiver.try_receive();
        }
        "#,
    );

    for (name, valid) in [
        ("genuine", true),
        ("fabricated", false),
        ("expose_protocol", false),
        ("change_kind", false),
    ] {
        let checked = application
            .lowered_unit(source_function_body_key(&application, name))
            .unwrap();

        if valid {
            assert!(
                !checked.diagnostics().has_errors(),
                "{name}: {:?}",
                checked.diagnostics()
            );
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                checked.diagnostics(),
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn nested_owner_cleanup_preserves_its_required_authority() {
    let application = compilation(
        r#"
        trusted module app;
        extern trusted func opaque();
        struct Marker {}
        trusted predicate live(marker: &Marker);
        struct Owner {
            instance: Marker;
            trusted destruct() requires(trusted live(&self.instance)) { opaque(); }
        }
        struct Outer {
            owner: Owner;
            trusted destruct() requires(trusted live(&self.owner.instance))
                ensures(trusted live(&self.owner.instance)) {}
            consume func close() requires(trusted live(&self.owner.instance)) {}
        }
        trusted func make() -> Outer ensures(trusted live(&result.owner.instance)) {
            return { owner = { instance = Marker {} } };
        }
        func caller() { let value = make(); value.close(); }
    "#,
    );

    let diagnostics = application.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:?}");
}

#[test]
fn opaque_execution_preserves_only_unexposed_shared_carrier_references() {
    for (body, valid) in [
        (
            "let owner = make(); let borrowed = borrow(&owner); opaque(); observe(borrowed);",
            true,
        ),
        (
            "let owner = make(); observe(&owner); opaque(); observe(&owner);",
            true,
        ),
        (
            "let mut owner = make(); replace_owner(&mut owner); observe(&owner);",
            false,
        ),
        (
            "let mut owner = make(); let holder: Holder = { owner = &mut owner }; replace_holder(holder); observe(&owner);",
            false,
        ),
        (
            "let mut owner = make(); let address = core.memory.address_of_mut<Owner>(&mut owner); opaque(); observe(&owner);",
            true,
        ),
        (
            "let mut owner = make(); let address = core.memory.address_of_mut<Owner>(&mut owner); publish(address); opaque(); observe(&owner);",
            false,
        ),
        (
            "let owner = make(); let mut borrowed = borrow(&owner); replace(&mut borrowed); observe(borrowed);",
            false,
        ),
    ] {
        let application = compilation(&format!(
            r#"
            trusted module app;

            using core.memory;
            struct Marker {{}}
            struct Owner {{ instance: Marker; }}
            trusted predicate live(marker: &Marker);
            trusted func make() -> Owner ensures(trusted live(&result.instance)) {{ return {{ instance = Marker {{}} }}; }}
            func borrow(pos owner: &Owner) -> &Owner {{ return owner; }}
            func observe(pos owner: &Owner) requires(trusted live(&owner.instance)) {{}}
            extern trusted func opaque();
            extern trusted func replace(pos owner: &mut &Owner);
            extern trusted func replace_owner(pos owner: &mut Owner);
            extern trusted func publish(pos pointer: DataPointer<Owner>);
            struct Holder {{ owner: &mut Owner; }}
            extern trusted func replace_holder(pos holder: Holder);
            trusted func caller() uses(raw_memory) {{ {body} }}
        "#
        ));

        let checked = application
            .lowered_unit(source_function_body_key(&application, "caller"))
            .unwrap();

        assert_eq!(
            !checked.diagnostics().has_errors(),
            valid,
            "{body}: {:?}",
            checked.diagnostics()
        );
    }
}

#[test]
fn borrowed_accessor_does_not_prepare_its_referents_cleanup() {
    let application = compilation(
        r#"
        trusted module app;
        struct Marker {}
        trusted predicate live(marker: &Marker);
        struct Owner {
            instance: Marker;
            const func marker() -> &Marker executes(pure, total) { return &self.instance; }
            trusted destruct() requires(trusted live(self.marker())) {}
        }
        trusted func make() -> Owner ensures(trusted live(result.marker())) {
            return { instance = Marker {} };
        }
        func caller() { let owner = make(); }
    "#,
    );

    let diagnostics = application.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:?}");
}

#[test]
fn shared_component_borrow_preserves_only_its_marker_observations() {
    let application = compilation(
        r#"
        trusted module app;
        struct Marker {}
        struct Owner { instance: Marker; }
        struct Pair { first: Owner; second: Owner; }
        trusted predicate live(marker: &Marker);
        trusted func make() -> Pair ensures(trusted live(&result.first.instance)) {
            return { first = { instance = Marker {} }, second = { instance = Marker {} } };
        }
        extern trusted func opaque();
        func observe(pos owner: &Owner) requires(trusted live(&owner.instance)) {}
        func caller() {
            let pair = make();
            let borrowed = &pair.first;
            opaque();
            observe(borrowed);
        }
    "#,
    );

    let checked = application
        .lowered_unit(source_function_body_key(&application, "caller"))
        .unwrap();

    assert!(
        !checked.diagnostics().has_errors(),
        "{:?}",
        checked.diagnostics()
    );
}

#[test]
fn fallible_pair_factory_keeps_each_complete_owner() {
    let source = r#"
        trusted module app;
        struct Marker {}
        struct Owner { instance: Marker; }
        trusted predicate live(marker: &Marker);
        trusted func make() -> Owner ensures(trusted live(&result.instance)) { return { instance = Marker {} }; }
        const func first(pos value: &Result<(Owner, Owner), u32>) -> &Marker
            requires(value matches Ok(_)) executes(pure, total) {
            match value {
                case Ok((owner, _)) { return &owner.instance; }
                case Error(_) { panic("absent owner"); }
            }
        }
        const func second(pos value: &Result<(Owner, Owner), u32>) -> &Marker
            requires(value matches Ok(_)) executes(pure, total) {
            match value {
                case Ok((_, owner)) { return &owner.instance; }
                case Error(_) { panic("absent owner"); }
            }
        }
        func pair(pos fail: bool) -> Result<(Owner, Owner), u32>
            ensures(!(result matches Ok(_)) || trusted live(first(&result)), !(result matches Ok(_)) || trusted live(second(&result))) {
            if fail { return Error(1); }
            return Ok((make(), make()));
        }
        func observe(pos owner: &Owner) requires(trusted live(&owner.instance)) {}
        func caller(pos fail: bool) -> Result<unit, u32> {
            let (left, right) = try pair(fail);
            observe(&left);
            observe(&right);
            return Ok(unit);
        }
    "#;

    for valid in [true, false] {
        let source = if valid {
            source.to_owned()
        } else {
            source
                .replace(
                    "return Ok((make(), make()));",
                    "return Ok((make(), Owner { instance = Marker {} }));",
                )
                .replace(
                    ", !(result matches Ok(_)) || trusted live(second(&result))",
                    "",
                )
        };

        let application = compilation(&source);
        let diagnostics = application.check_diagnostics();

        assert_eq!(!diagnostics.has_errors(), valid, "{diagnostics:?}");

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                &diagnostics,
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn nullable_propagation_keeps_only_the_successful_carriers_authority() {
    let application = compilation(
        r#"
        trusted module app;
        struct Marker {}
        struct Owner { instance: Marker; }
        trusted predicate live(marker: &Marker);
        trusted func make() -> Owner ensures(trusted live(&result.instance)) { return { instance = Marker {} }; }
        const func marker(pos value: &(Owner?)) -> &Marker requires(!(value matches none)) executes(pure, total) {
            match value { case ?owner { return &owner.instance; } case none { panic("absent owner"); } }
        }
        func maybe(pos fail: bool) -> Owner? ensures(result matches none || trusted live(marker(&result))) {
            if fail { return none; }
            return make();
        }
        func observe(pos owner: &Owner) requires(trusted live(&owner.instance)) {}
        func caller(pos fail: bool) -> unit? {
            let owner: Owner = maybe(fail)?;
            observe(&owner);
            return unit;
        }
    "#,
    );

    let diagnostics = application.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:?}");
}

#[test]
fn loop_cleanup_keeps_independent_complete_carriers() {
    for operation in [
        "opaque();",
        "_ = take<T>(&mut pending);",
        "await suspend();",
    ] {
        let application = compilation(&format!(
            r#"
            trusted module app;
            struct Marker {{}}
            trusted predicate live(marker: &Marker);
            struct Owner {{ instance: Marker; }}
            struct Anchor {{}}
            extern trusted func borrow(pos anchor: &Anchor) -> &Owner ensures(trusted live(&result.instance)) executes(pure, total);
            struct Pending<T> {{ mut value: T?; }}
            struct Guard {{
                instance: Marker;
                owner: &Owner;
                trusted destruct() requires(trusted live(&self.instance)) {{}}
            }}
            trusted func make() -> Owner ensures(trusted live(&result.instance)) {{ return {{ instance = Marker {{}} }}; }}
            trusted func lock(pos owner: &Owner) -> Guard
                requires(trusted live(&owner.instance))
                ensures(trusted live(&owner.instance), trusted live(&result.instance)) {{
                return {{ instance = Marker {{}}, owner = owner }};
            }}
            struct Registration {{
                instance: Marker;
                owner: &Owner;
                mut active: bool;
                trusted destruct() requires(!self.active || (trusted live(&self.instance) && trusted live(&self.owner.instance)))
                    when(!self.active) {{ executes(pure, total) }} {{ if self.active {{ opaque(); }} }}
                consume mut func complete() {{ self.active = false; }}
            }}
            trusted func register(pos owner: &Owner) -> Registration
                requires(trusted live(&owner.instance))
                ensures(trusted live(&result.instance), trusted live(&result.owner.instance)) {{
                return {{ instance = Marker {{}}, owner = owner, active = true }};
            }}
            extern trusted func stop() -> bool;
            extern trusted func opaque();
            extern trusted func value<T>() -> T;
            extern trusted func take<T>(pos pending: &mut Pending<T>) -> T;
            extern trusted func get<T>(pos guard: &mut Guard) -> &mut T
                requires(trusted live(&guard.instance)) ensures(trusted live(&guard.instance)) executes(pure, total);
            extern trusted async func suspend();
            async func caller<T>(pos anchor: &Anchor, pos mut pending: Pending<T>) {{
                let owner = borrow(anchor);
                let mut registration = register(owner);
                while true {{
                    let generation: u32 = {{
                        let mut guard = lock(owner);
                        let payload: &mut T = get<T>(&mut guard);
                        if stop() {{ registration.complete(); return unit; }}
                        {operation}
                        yield 0;
                    }};
                    await suspend();
                }}
            }}
        "#
        ));

        let checked = application
            .lowered_unit(source_function_body_key(&application, "caller"))
            .unwrap();

        assert!(
            !checked.diagnostics().has_errors(),
            "{operation}: {:?}",
            checked.diagnostics()
        );
    }
}

#[test]
fn production_guard_cleanup_survives_loop_payload_transitions() {
    for operation in [
        "payload.inner.change();",
        "touch<Payload<T>>(payload);",
        "opaque();",
        "_ = take<T>(&mut pending);",
        "await suspend();",
    ] {
        let application = production_sync_compilation(&format!(
            r#"
            trusted module app;

            using std.sync;
            struct Pending<T> {{ mut value: T?; }}
            struct Payload<T> {{ mut inner: Inner; mut value: T?; }}
            struct Inner {{
                mut value: u32;
                mut func change() {{ opaque(); self.value += 1; }}
            }}
            extern trusted func opaque();
            extern trusted func touch<T>(pos value: &mut T);
            extern trusted func stop() -> bool;
            extern trusted func take<T>(pos pending: &mut Pending<T>) -> T;
            extern trusted async func suspend();
            async func production_probe<T>(pos lock: &std.sync.SpinLock<Payload<T>>, pos mut pending: Pending<T>)
                requires(trusted std.sync.spin_lock.valid(&lock.prepared.instance)) {{
                while true {{
                    let generation: u32 = {{
                        let mut guard = lock.lock();
                        let payload: &mut Payload<T> = guard.get();
                        if stop() && stop() {{ return unit; }}
                        {operation}
                        yield 0;
                    }};
                    await suspend();
                }}
            }}
        "#
        ));

        let checked = application
            .lowered_unit(source_function_body_key(&application, "production_probe"))
            .unwrap();

        assert!(
            !checked.diagnostics().has_errors(),
            "{operation}: {:?}",
            checked.diagnostics()
        );
    }
}

#[test]
fn opaque_calls_preserve_unexposed_scalar_values_only() {
    for (effect, valid) in [
        ("opaque();", true),
        ("signal();", true),
        ("replace(&mut value);", false),
        (
            "let pointer = core.memory.address_of_mut<u32>(&mut value); opaque();",
            true,
        ),
        (
            "let pointer = core.memory.address_of_mut<u32>(&mut value); publish(pointer); opaque();",
            false,
        ),
    ] {
        let source = format!(
            r#"
            trusted module app;

            using core.memory;

            using core.atomic;
            extern trusted func opaque();
            static WORD: core.atomic.Atomic<u32> = core.atomic.initialize<u32>(0);
            func signal() {{ core.atomic.store<u32, 2>(&WORD, 1); }}
            extern trusted func replace(pos value: &mut u32);
            extern trusted func publish(pos pointer: DataPointer<u32>);
            trusted func caller(pos initial: u32) -> u32 ensures(result == initial) uses(raw_memory) {{
                let mut value = initial;
                {effect}
                return value;
            }}
        "#
        );

        let application = compilation(&source);

        let checked = application
            .execution_properties(source_function_body_key(&application, "caller"))
            .unwrap();

        assert_eq!(
            !checked.diagnostics().has_errors(),
            valid,
            "{effect}: {:?}",
            checked.diagnostics()
        );
    }
}

#[test]
fn opaque_returned_borrows_revoke_possible_inputs_but_not_unpassed_owners() {
    for (observed, valid) in [("first", false), ("second", false), ("third", true)] {
        let application = compilation(&format!(
            r#"
            trusted module app;
            struct Marker {{}}
            struct Owner {{ marker: Marker; }}
            trusted predicate live(marker: &Marker);
            trusted func make() -> Owner ensures(trusted live(&result.marker)) {{ return {{ marker = Marker {{}} }}; }}
            extern trusted func choose(pos first: &mut Owner, pos second: &mut Owner) -> &mut Owner;
            extern trusted func replace(pos owner: &mut Owner);
            func observe(pos owner: &Owner) requires(trusted live(&owner.marker)) {{}}
            func caller() {{
                let mut first = make();
                let mut second = make();
                let third = make();
                {{ let selected = choose(&mut first, &mut second); replace(selected); }}
                observe(&{observed});
            }}
        "#
        ));

        let checked = application
            .lowered_unit(source_function_body_key(&application, "caller"))
            .unwrap();

        assert_eq!(
            !checked.diagnostics().has_errors(),
            valid,
            "{observed}: {:?}",
            checked.diagnostics()
        );
    }
}

#[test]
fn returned_tuple_owner_keeps_its_predicate_after_scalar_field_reads() {
    let application = compilation(
        r#"
        trusted module app;

        using std.memory;
        struct Owner
        {
            mut first: u32;
            mut second: u32;
            mut third: usize;
            mut fourth: usize;
            mut pointer: DataPointer<u8>;
        }
        trusted predicate live(owner: &Owner);
        trusted func make() -> (u32, Owner) ensures(trusted live(&result.1)) {
            return (1, Owner { first = 2, second = 3, third = 0, fourth = 0, pointer = std.memory.null<u8>() });
        }
        func consume_owner(pos owner: Owner) requires(trusted live(&owner)) {}
        func caller() {
            loop {
                let (state, owner) = make();
                if state == 1 {
                    let fields = (owner.first, owner.second, owner.third, owner.fourth);
                    consume_owner(owner);
                } else { return; }
            }
        }
    "#,
    );

    let checked = application
        .lowered_unit(source_function_body_key(&application, "caller"))
        .unwrap();

    assert!(
        !checked.diagnostics().has_errors(),
        "{:?}",
        checked.diagnostics()
    );
}

#[test]
fn opaque_drivers_retain_the_borrows_held_by_deferred_values() {
    for (parameter, initialize, argument) in [
        ("Future<unit>", "work(&mut owner)", "pending"),
        ("&mut Future<unit>", "work(&mut owner)", "&mut pending"),
        (
            "&mut (Future<unit>, i32)",
            "(work(&mut owner), 0)",
            "&mut pending",
        ),
        ("Task<unit>", "work(&mut owner).start()", "pending"),
        (
            "&mut Task<unit>",
            "work(&mut owner).start()",
            "&mut pending",
        ),
    ] {
        let application = compilation(&format!(
            r#"
            trusted module app;
            struct Marker {{}}
            struct Owner {{ mut marker: Marker; }}
            trusted predicate live(marker: &Marker);
            trusted func make() -> Owner ensures(trusted live(&result.marker)) {{ return {{ marker = Marker {{}} }}; }}
            extern func replace(pos owner: &mut Owner);
            async func work(pos owner: &mut Owner) {{ replace(owner); }}
            extern func drive(pos pending: {parameter});
            func observe(pos owner: &Owner) requires(trusted live(&owner.marker)) {{}}
            async func caller() {{
                let mut owner = make();
                {{ let mut pending = {initialize}; drive({argument}); }}
                observe(&owner);
            }}
        "#
        ));

        let diagnostics = application.check_diagnostics();

        assert!(
            diagnostics
                .by_kind(bray_diagnostics::DiagnosticKind::CheckingIncompatibleExpressionType)
                .next()
                .is_none(),
            "{parameter}: {diagnostics:?}"
        );

        assert!(
            diagnostics
                .by_kind(bray_diagnostics::DiagnosticKind::CheckingCannotInferExpressionType)
                .next()
                .is_none(),
            "{parameter}: {diagnostics:?}"
        );

        assert!(
            diagnostics
                .by_kind(bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven)
                .next()
                .is_some(),
            "{parameter}: {diagnostics:?}"
        );
    }
}

#[test]
fn production_owner_transport_and_async_observations_preserve_authority() {
    let application = crate::test_support::production_standard_library_compilation(
        crate::SelectedTarget::for_native(bray_target::NativeTarget::X86_64WindowsMsvc),
        r#"
        module app;

        using std.channel;
        using std.fs;
        using std.io;
        using std.path;
        using std.sync;
        using std.testing;
        static ONCE: std.sync.Once<u32> = { prepared = std.sync.Once<u32>.prepare() };
        func initialized_value() -> u32 { return 37; }
        func static_once_case() requires(blocking_execution()) {
            _ = catch { _ = ONCE.get_or_init(initialized_value); };
            _ = ONCE.get_or_init(initialized_value);
        }
        func pair() {
            let (sender, receiver) = std.testing.assert_ok<
                (std.channel.Sender<u32>, std.channel.Receiver<u32>), std.channel.ChannelError
            >(std.channel.bounded<u32>(capacity = 1));
            sender.close();
            receiver.close();
        }
        func barrier_case() requires(blocking_execution()) {
            let barrier = std.testing.assert_ok<std.sync.Barrier, std.sync.BarrierError>(
                std.sync.Barrier(participants = 1));
            _ = barrier.wait();
        }
        func once_case(pos callback: func() -> u32) requires(blocking_execution()) {
            let once = std.sync.Once<u32>.empty();
            _ = once.get_or_init(callback);
        }
        async func channel_wait_case() {
            let (sender, receiver) = std.testing.assert_ok<
                (std.channel.Sender<u32>, std.channel.Receiver<u32>), std.channel.ChannelError
            >(std.channel.bounded<u32>(capacity = 1));
            _ = catch { _ = await sender.send(1); };
        }
        func completed_file_case(pos path: &std.path.Path) -> Result<unit, std.io.IoError>
            requires(blocking_execution()) {
            let options = std.fs.OpenOptions(access = std.fs.FileAccess.ReadWrite,
                creation = std.fs.FileCreation.CreateNew);
            let mut file = std.testing.assert_ok<std.fs.File, std.io.IoError>(
                std.fs.File.open(path, options = options));
            let mut buffer: bytes<1> = [0];
            _ = file.read(&mut buffer[..]);
            _ = file.write(&buffer[..]);
            std.testing.assert_ok<unit, std.io.IoError>(file.close());
            assert(file.is_complete());
            std.testing.assert_ok<unit, std.io.IoError>(std.fs.remove_file(path));
            return Ok(unit);
        }
        func completed_directory_case(pos path: &std.path.Path) -> Result<unit, std.io.IoError>
            requires(blocking_execution()) {
            let mut directory = std.testing.assert_ok<std.fs.Directory, std.io.IoError>(
                std.fs.Directory.open(path));
            let first = std.testing.assert_ok<std.fs.DirectoryEntry?, std.io.IoError>(
                directory.next());
            std.testing.assert_ok<unit, std.io.IoError>(directory.close());
            assert(directory.is_complete());
            return Ok(unit);
        }
        async func worker(pos lock: &std.sync.Mutex<u32>)
            requires(blocking_execution(), trusted std.sync.mutex.valid(&lock.instance))
            ensures(trusted std.sync.mutex.valid(&lock.instance)) {
            let guard = lock.lock();
        }
        async func task_case() requires(blocking_execution()) {
            let lock = std.sync.Mutex<u32>(0);
            let task = worker(&lock).start();
            try await task.join();
            let guard = lock.lock();
        }
        "#,
    );

    for name in ["assert_ok", "assert_error", "assert_present"] {
        let summary = crate::compilation::result_dependencies::source_function_dependencies(
            &application,
            name,
        );

        assert!(summary.returned_input().is_some(), "{name}: {summary:?}");
    }

    let failures = [
        "pair",
        "barrier_case",
        "once_case",
        "task_case",
        "static_once_case",
        "channel_wait_case",
        "completed_file_case",
        "completed_directory_case",
    ]
    .into_iter()
    .filter_map(|name| {
        let checked = application
            .lowered_unit(source_function_body_key(&application, name))
            .unwrap();

        checked.diagnostics().has_errors().then(|| {
            format!(
                "{name}: {:?}. Source: {:?}",
                checked.diagnostics(),
                crate::test_support::diagnostic_source_fragments(
                    &application,
                    checked.diagnostics()
                )
            )
        })
    })
    .collect::<Vec<_>>();

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn task_observation_preserves_the_shared_owner_marker() {
    let application = compilation(
        r#"
        trusted module app;

        struct Marker
        {
        }

        trusted predicate live(marker: &Marker);

        struct Owner
        {
            marker: Marker;

            trusted destruct()
                requires(trusted live(&self.marker))
            {
            }
        }

        trusted func make() -> Owner
            ensures(trusted live(&result.marker))
        {
            return
            {
                marker = Marker {},
            };
        }

        extern async func suspend();

        async func work(pos owner: &Owner)
            requires(trusted live(&owner.marker))
            ensures(trusted live(&owner.marker))
        {
            await suspend();
        }

        func observe(pos owner: &Owner)
            requires(trusted live(&owner.marker))
        {
        }

        async func caller()
        {
            let owner = make();
            let task = work(&owner).start();

            try await task.join();
            observe(&owner);
        }
    "#,
    );

    let diagnostics = application.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:?}");
}

#[test]
fn static_marker_preservation_keeps_selected_instances_distinct() {
    for (expected, valid) in [(1, true), (2, false)] {
        let application = compilation(&format!(
            r#"
            trusted module app;
            struct Marker {{}}
            struct Owner {{ marker: Marker; }}
            trusted predicate held(marker: &Marker, identity: u32);
            trusted const func create(pos identity: u32) -> Owner
                ensures(trusted held(&result.marker, identity)) executes(pure, total)
            {{ return {{ marker = Marker {{}} }}; }}
            static OWNER<const N: u32>: Owner = create(N);
            extern func opaque();
            func observe(pos owner: &Owner, pos identity: u32)
                requires(trusted held(&owner.marker, identity)) {{}}
            func caller() {{
                observe(&OWNER<1>, 1);
                observe(&OWNER<2>, 2);
                opaque();
                observe(&OWNER<1>, {expected});
                observe(&OWNER<2>, 2);
            }}
        "#
        ));

        let checked = application
            .lowered_unit(source_function_body_key(&application, "caller"))
            .unwrap();

        if valid {
            assert!(
                !checked.diagnostics().has_errors(),
                "{:?}",
                checked.diagnostics()
            );
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                checked.diagnostics(),
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn opaque_cleanup_preserves_independent_owner_authority() {
    for cleanup in [
        "let task = work().start();",
        r#"let report = catch { panic("failure"); };"#,
    ] {
        let application = compilation(&format!(
            r#"
            trusted module app;
            struct Marker {{}}
            trusted predicate live(marker: &Marker);
            struct Owner {{
                marker: Marker;
                trusted destruct() requires(trusted live(&self.marker)) {{}}
            }}
            trusted func make() -> Owner ensures(trusted live(&result.marker)) {{
                return {{ marker = Marker {{}} }};
            }}
            extern trusted async func work();
            async func caller() {{
                let owner = make();
                {{ {cleanup} }}
            }}
        "#
        ));

        let diagnostics = application.check_diagnostics();

        assert!(!diagnostics.has_errors(), "{cleanup}: {diagnostics:?}");
    }
}

#[test]
fn production_synchronization_api_cleanup_checks() {
    let names = [
        "synchronization_owners_guard_and_publish_values",
        "concurrency_stress_mutex_contention_and_panic_cleanup_preserve_progress",
        "concurrency_model_reader_writer_lock_admits_a_pending_writer_before_later_readers",
        "concurrency_stress_barrier_reuses_its_generation_across_contended_rounds",
    ];

    let mut source = include_str!("../../../../../standard-library/std/tests/api/sync.bray")
        .replace("@test", "");

    for name in names {
        source = source.replace(
            &format!("func {name}()"),
            &format!("func {name}() requires(blocking_execution())"),
        );
    }

    let application = crate::test_support::production_standard_library_compilation(
        crate::SelectedTarget::for_native(bray_target::NativeTarget::X86_64WindowsMsvc),
        &source,
    );

    let failures = names
        .into_iter()
        .filter_map(|name| {
            let checked = application
                .lowered_unit(source_function_body_key(&application, name))
                .unwrap();

            checked
                .diagnostics()
                .has_errors()
                .then(|| format!("{name}: {:?}", checked.diagnostics()))
        })
        .collect::<Vec<_>>();

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn conditional_cleanup_requires_authority_for_the_selected_payload() {
    for (construction, valid) in [
        ("make();", true),
        ("let owner = make();", true),
        ("let _: Owner? = make();", true),
        ("let _: Owner? = Owner { marker = Marker {} };", false),
        ("let owner: Owner? = Owner { marker = Marker {} };", false),
    ] {
        let application = compilation(&format!(
            r#"
            trusted module app;
            struct Marker {{}}
            trusted predicate live(marker: &Marker);
            struct Owner {{
                marker: Marker;
                trusted destruct() requires(trusted live(&self.marker)) executes(total) {{}}
            }}
            const func marker(pos value: &(Owner?)) -> &Marker
                requires(!(value matches none)) executes(pure, total)
            {{
                match value {{
                    case ?owner {{ return &owner.marker; }}
                    case none {{ panic("absent owner"); }}
                }}
            }}
            extern trusted func make() -> Owner?
                ensures(result matches none || trusted live(marker(&result)));
            func caller() {{ {construction} }}
        "#
        ));

        let diagnostics = application.check_diagnostics();

        if valid {
            assert!(!diagnostics.has_errors(), "{construction}: {diagnostics:?}");
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn task_cleanup_retains_loans_until_the_task_is_consumed() {
    for (body, valid) in [
        (
            "let task = work(&value).start(); opaque(); value = 1;",
            false,
        ),
        ("_ = work(&value).start(); opaque(); value = 1;", false),
        (
            "let task = work(&value).start(); let moved = task; opaque(); value = 1;",
            false,
        ),
        (
            "let task = work(&value).start(); try await task.join(); value = 1;",
            true,
        ),
        (
            "{ let task = work(&value).start(); opaque(); } value = 1;",
            true,
        ),
        (
            "let task = work(&value).start(); try await task.cancel(); value = 1;",
            true,
        ),
        (
            "let task = work(&value).start(); if flag { try await task.join(); } value = 1;",
            false,
        ),
        (
            "let task = work(&value).start(); if flag { try await task.join(); } else { try await task.cancel(); } value = 1;",
            true,
        ),
        (
            "let owner = { let task = work(&value).start(); yield Holder { task = task }; }; opaque(); value = 1;",
            false,
        ),
        (
            "{ let owner = Holder { task = work(&value).start() }; opaque(); } value = 1;",
            true,
        ),
        (
            "let mut task = work(&value).start(); task = work(&value).start(); value = 1;",
            false,
        ),
    ] {
        let application = compilation(&format!(
            r#"
            module app;
            extern async func work(pos value: &u32);
            extern func opaque();
            struct Holder {{ task: Task<unit>; }}
            async func caller(pos flag: bool) {{
                let mut value: u32 = 0;
                {body}
            }}
        "#
        ));

        let diagnostics = application.check_diagnostics();

        if valid {
            assert!(!diagnostics.has_errors(), "{body}: {diagnostics:?}");
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                DiagnosticKind::CheckingConflictingBorrow,
            );
        }
    }
}

#[test]
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
                .unwrap();

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
