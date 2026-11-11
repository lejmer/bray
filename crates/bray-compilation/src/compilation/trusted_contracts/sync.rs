use std::sync::OnceLock;

use bray_diagnostics::{DiagnosticArgValue, DiagnosticContractObligationKind, DiagnosticKind};
use bray_symbols::PackageIdentity;

use crate::compilation::result_dependencies::callable_dependencies;
use crate::test_support::{
    ProductionFixtureCache, compilation, production_fixture, source_function_body_key, source_input,
};
use crate::{Compilation, CompilationRequest};

#[test]
fn ownership_composition_accepts_complete_transport() {
    let source = include_str!("../../../../../xtask/fixtures/native-execution/ownership-transitions.bray");
    let compilation = compilation(source);
    let diagnostics = compilation.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:#?}");
}

#[test]
fn ownership_composition_preserves_trusted_result_transport() {
    let source = include_str!("../../../../../xtask/fixtures/native-execution/ownership-transitions.bray")
        .replace("owner = acquire()", "owner = trusted acquire()");
    let compilation = compilation(&source);
    let diagnostics = compilation.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:#?}");
}

#[test]
fn ownership_composition_rejects_fabricated_authority() {
    let source = include_str!("../../../../../xtask/fixtures/native-execution/ownership-transitions.bray")
        .replace("owner = acquire()", r#"owner = Owner
        {
            marker = Marker
            {
            },
            value = 7
        }"#);

    let compilation = compilation(&source);
    let diagnostics = compilation.check_diagnostics();

    bray_testing::assert_goal_state_diagnostic_kind(
        diagnostics,
        DiagnosticKind::CheckingTrustedObligationNotProven,
    );
}

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
#[ignore = "tier:proof"]
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
            .expect("fixture callable must lower successfully");

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
    static FIXTURES: ProductionFixtureCache = OnceLock::new();

    production_fixture(&FIXTURES, caller, target, build_production_sync)
}

fn build_production_sync(caller: &str, target: crate::SelectedTarget) -> Compilation {
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
        PackageIdentity::try_new("std").expect("fixture package identity must be valid"),
        sources
            .iter()
            .enumerate()
            .map(|(index, source)| {
                source_input(
                    source,
                    u32::try_from(index)
                        .expect("fixture value must fit the requested representation"),
                )
            })
            .collect(),
        crate::CompilationOptions::new(
            crate::WorkerBudget::serial(),
            bray_symbols::ProductKind::Library,
            target,
        )
        .with_native_link_inputs(["c", "pthread", "dl", "Kernel32"].map(|name| {
            bray_symbols::NativeLinkRequirement::new(
                bray_base::NonEmptySharedStr::try_new(name)
                    .expect("fixture non empty shared str must be valid"),
                if name == "dl" {
                    bray_symbols::NativeLinkKind::Dynamic
                } else {
                    bray_symbols::NativeLinkKind::System
                },
            )
        })),
    )
    .with_standard_library_source_authority();

    let compilation = Compilation::load(request).expect("fixture input must load");

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
fn nullable_borrowed_witnesses_preserve_their_portable_guards() {
    for (body, valid) in [
        ("return owner;", true),
        ("let stored: (&Owner)? = owner; return stored;", true),
        ("return lend(owner);", true),
        ("return other;", false),
    ] {
        let source = format!(
            r#"
            trusted module app;
            struct Marker {{}}
            struct Owner {{ acquisition: Marker; }}
            trusted predicate held(owner: &Marker);
            extern func lend(pos owner: &Owner) -> &Owner
                requires(trusted held(owner = &owner.acquisition))
                ensures(trusted held(owner = &result.acquisition));
            const func marker(pos owner: &((&Owner)?)) -> &Marker
                requires(owner matches ?_)
                executes(pure, total)
            {{
                return match owner {{
                    case ?value {{ yield &value.acquisition; }}
                    case none {{ panic("missing owner"); }}
                }};
            }}
            func forward(pos owner: &Owner, pos other: &Owner) -> (&Owner)?
                requires(trusted held(owner = &owner.acquisition))
                ensures(result matches none || trusted held(owner = marker(&result)))
            {{ {body} }}
        "#
        );

        let application = compilation(&source);

        let checked = application
            .lowered_unit(source_function_body_key(&application, "forward"))
            .expect("nullable borrowed witness retains its checked body");

        assert_eq!(
            !checked.diagnostics().has_errors(),
            valid,
            "{source}: {checked:#?}"
        );
    }
}

#[test]
fn transparent_predicate_getters_preserve_the_contained_owner() {
    for (body, valid) in [
        (
            "match consume owner { case ?value { release_owner(value); } case none {} }",
            true,
        ),
        ("release_owner(Owner { acquisition = Marker {} });", false),
    ] {
        let source = format!(
            r#"
            trusted module app;
            struct Marker {{}}
            struct Owner {{ acquisition: Marker; }}
            trusted predicate acquired(owner: &Marker);
            const func present_owner(pos owner: &(Owner?)) -> &Owner
                requires(owner matches ?_) executes(pure, total) {{
                return match owner {{
                    case ?value {{ yield &value; }}
                    case none {{ panic("missing owner"); }}
                }};
            }}
            predicate optional_owner(owner: &(Owner?)) = owner matches none || acquired(owner = &present_owner(owner).acquisition);
            func release_owner(pos owner: Owner) requires(trusted acquired(owner = &owner.acquisition)) {{}}
            func forward(pos owner: Owner?) requires(trusted optional_owner(owner = &owner)) {{ {body} }}
            "#
        );

        let application = compilation(&source);

        assert!(
            !application.syntax_tree_result().diagnostics().has_errors(),
            "{source}: {:?}",
            application.syntax_tree_result().diagnostics()
        );

        let checked = application
            .lowered_unit(source_function_body_key(&application, "forward"))
            .expect("predicate getter retains its checked reference context");

        assert_eq!(
            !checked.diagnostics().has_errors(),
            valid,
            "{source}: {checked:#?}"
        );
    }
}

#[test]
fn copied_addresses_keep_value_dependencies_without_the_container_loan() {
    for mutation in ["", "source = 1;"] {
        let source = format!(
            r#"
            trusted module app;
            using core.memory;
            struct Marker {{}}
            struct Reference {{ acquisition: Marker; pointer: DataPointer<u32>; }}
            trusted predicate acquired(owner: &Marker);
            trusted func fresh(pos pointer: DataPointer<u32>) -> Reference
                ensures(trusted acquired(owner = &result.acquisition)) {{
                return {{ acquisition = Marker {{}}, pointer = pointer }};
            }}
            const func borrow_reference(pos owner: &Reference) -> &Reference executes(pure, total) {{ return owner; }}
            trusted func retain(pos owner: &Reference) -> Reference
                requires(trusted acquired(owner = &owner.acquisition))
                ensures(trusted acquired(owner = &result.acquisition)) {{
                return {{ acquisition = Marker {{}}, pointer = owner.pointer }};
            }}
            func release_reference(pos owner: Reference) requires(trusted acquired(owner = &owner.acquisition)) {{}}
            func caller() {{
                let mut source: u32 = 0;
                let first = fresh(core.memory.address_of<u32>(&source));
                let second = retain(borrow_reference(&first));
                release_reference(first);
                {mutation}
                release_reference(second);
            }}
            "#
        );

        let application = compilation(&source);

        assert!(
            !application.syntax_tree_result().diagnostics().has_errors(),
            "{source}: {:?}",
            application.syntax_tree_result().diagnostics()
        );

        let checked = application
            .lowered_unit(source_function_body_key(&application, "caller"))
            .expect("address retention keeps its checked producer contracts");

        assert!(
            !checked.diagnostics().has_errors(),
            "{source}: {checked:#?}"
        );
    }
}

#[test]
fn borrowed_heap_fields_preserve_the_anchor_without_projecting_onto_it() {
    let mut failures = Vec::new();

    for (parameter, argument, valid) in [("&Owner", "owner", true), ("Owner", "&owner", false)] {
        let source = format!(
            r#"
            trusted module app;
            using core.memory;
            struct Owner {{ pointer: DataPointer<ForeignPayload>; }}
            struct Marker {{}}
            struct ForeignPayload {{ marker: Marker; }}
            trusted predicate retained(owner: &Owner);
            trusted func referent(pos owner: &Owner) -> &ForeignPayload
                requires(trusted retained(owner))
                executes(pure, total)
                uses(raw_memory, unchecked_alias)
            {{
                let permit = trusted lambda(pos owner: &Owner)
                    requires(trusted retained(owner))
                    ensures(
                        trusted core.memory.valid_read<ForeignPayload>(pointer = owner.pointer, count = 1),
                        trusted core.memory.aligned_for<ForeignPayload>(pointer = owner.pointer),
                        trusted core.memory.initialized_as<ForeignPayload>(pointer = owner.pointer),
                        trusted core.memory.shared_alias_valid<ForeignPayload>(pointer = owner.pointer),
                        trusted core.memory.epoch_current<ForeignPayload>(pointer = owner.pointer),
                        trusted core.memory.synchronized_access<ForeignPayload>(pointer = owner.pointer),
                        trusted core.memory.movement_stable<ForeignPayload>(pointer = owner.pointer),
                        trusted core.memory.finalization_pending<ForeignPayload>(pointer = owner.pointer),
                    )
                    executes(pure, total)
                {{}};

                trusted permit(owner);

                return trusted core.memory.borrow_from<ForeignPayload, Owner>(owner, owner.pointer);
            }}
            func selected_marker(pos owner: &Owner) -> &Marker
                requires(trusted retained(owner))
            {{
                let backing = referent(owner);
                return &backing.marker;
            }}
            func forwarded(pos owner: {parameter}) -> &Marker
                requires(trusted retained({argument}))
            {{ return selected_marker({argument}); }}
            "#
        );

        let application = compilation(&source);

        assert!(
            !application.syntax_tree_result().diagnostics().has_errors(),
            "{source}: {:?}",
            application.syntax_tree_result().diagnostics()
        );

        let diagnostics = application.check_diagnostics();
        let expected = DiagnosticKind::CheckingEscapingStorageDependency;

        let matches_expected = if valid {
            !diagnostics.has_errors()
        } else {
            diagnostics.iter().any(|diagnostic| diagnostic.kind() == expected)
                && diagnostics.iter().all(|diagnostic| diagnostic.kind() == expected)
        };

        if !matches_expected {
            failures.push((valid, source, diagnostics.clone()));
        }
    }

    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn complete_protocol_owners_survive_unrelated_calls_until_their_storage_is_exposed() {
    for (fields, operation) in [
        ("tag: usize; pointer: DataPointer<u8>;", "opaque();"),
        ("storage: Uninit<[u64; 16]>; finish: Native;", "opaque();"),
        ("first: &Uninit<u32>; second: &Uninit<u32>;", ""),
    ] {
        let exposures: &[(&str, bool)] = if operation.is_empty() {
            &[("", true)]
        } else {
            &[("", true), ("_ = trusted publish(&owner);", false)]
        };

        for &(publish, valid) in exposures {
            let source = format!(
                r#"
                trusted module app;
                using core.memory;
                using core.target;
                callable Native = @abi(c) trusted func();
                struct Owner {{ {fields} }}
                trusted predicate live(owner: &Owner);
                trusted predicate checked(owner: &Owner);
                extern trusted func inspect(pos owner: &Owner)
                    requires(trusted live(owner)) ensures(trusted checked(owner)) executes(pure, total);
                func observe(pos owner: &Owner) requires(trusted checked(owner)) executes(pure, total) {{}}
                extern func opaque() executes(total);
                extern func release_owner(pos owner: Owner) requires(trusted live(owner = &owner)) executes(total);
                trusted func publish(pos owner: &Owner) -> usize uses(intrinsic) {{
                    return trusted core.target.expose_address<Owner>(core.memory.address_of<Owner>(owner));
                }}
                func caller(pos owner: Owner) requires(trusted live(owner = &owner)) {{
                    {publish}
                    {operation}
                    inspect(&owner);
                    observe(&owner);
                    release_owner(owner);
                }}
            "#
            );

            let application = compilation(&source);

            assert!(
                !application.syntax_tree_result().diagnostics().has_errors(),
                "{source}"
            );

            let checked = application
                .lowered_unit(source_function_body_key(&application, "caller"))
                .expect("complete protocol owners retain their checked storage context");

            assert_eq!(
                !checked.diagnostics().has_errors(),
                valid,
                "{source}: {checked:#?}"
            );
        }
    }
}

#[test]
fn shared_protocol_markers_survive_native_exposure_and_loop_merges() {
    for (parameter, subject, binding) in [
        ("pos owner: &Owner", "owner", ""),
        ("pos original: Owner", "original", "let owner = &original;"),
    ] {
        for (use_owner, valid) in [
            ("observe(&owner.acquisition);", true),
            ("let fabricated: Marker = {}; observe(&fabricated);", false),
            ("observe_payload(owner.pointer);", false),
        ] {
            let source = format!(
                r#"
                trusted module app;
                using core.memory;
                using core.target;
                struct Marker {{}}
                struct Owner {{ acquisition: Marker; identity: u64; pointer: DataPointer<u64>; }}
                trusted predicate held(owner: &Marker);
                trusted predicate scalar_held(owner: &u64);
                trusted predicate payload_held(pointer: DataPointer<u64>);
                extern func opaque(pos pointer: DataPointer<u64>) executes(total);
                func observe(pos marker: &Marker) requires(trusted held(owner = marker)) executes(pure, total) {{}}
                func observe_scalar(pos value: &u64) requires(trusted scalar_held(owner = value)) executes(pure, total) {{}}
                func observe_payload(pos pointer: DataPointer<u64>) requires(trusted payload_held(pointer = pointer)) executes(pure, total) {{}}
                func advance(pos value: &mut u32) {{ value += 1; }}
                trusted func publish(pos owner: &Owner) -> usize uses(intrinsic) {{
                    return trusted core.target.expose_address<Owner>(core.memory.address_of<Owner>(owner));
                }}
                func caller({parameter}, pos count: u32)
                    requires(
                        trusted held(owner = &{subject}.acquisition),
                        trusted scalar_held(owner = &{subject}.identity),
                        trusted payload_held(pointer = {subject}.pointer)
                    )
                {{
                    {binding}
                    _ = trusted publish(owner);
                    let mut index: u32 = 0;
                    while index < count {{
                        if index & 1 == 0 {{ opaque(owner.pointer); }}
                        {use_owner}
                        observe_scalar(&owner.identity);
                        advance(&mut index);
                    }}
                    observe(&owner.acquisition);
                    observe_scalar(&owner.identity);
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
        }
    }
}

#[test]
fn mutable_protocol_markers_survive_only_marker_free_writes() {
    for (operation, valid) in [
        ("owner.payload += 1;", true),
        ("advance(&mut owner.payload);", true),
        ("trusted core.memory.write<u32>(pointer, 1);", true),
        ("replace(owner);", false),
        ("owner.acquisition = Marker {};", false),
    ] {
        let raw = operation.contains("core.memory.write");

        let memory_requirements = if raw {
            "trusted core.memory.valid_write<u32>(pointer = pointer, count = 1), trusted core.memory.aligned_for<u32>(pointer = pointer),"
        } else {
            ""
        };

        for publication in ["", "_ = trusted core.target.expose_address<Owner>(core.memory.address_of<Owner>(&owner));"] {
        let capabilities = match (raw, publication.is_empty()) {
            (true, false) => "uses(intrinsic, raw_memory, unchecked_init)",
            (true, true) => "uses(raw_memory, unchecked_init)",
            (false, false) => "uses(intrinsic)",
            (false, true) => "",
        };

        let source = format!(
            r#"
            trusted module app;
            using core.memory;
            using core.target;
            struct Marker {{}}
            struct Owner {{ mut acquisition: Marker; mut payload: u32; }}
            trusted predicate held(owner: &Marker);
            extern func replace(pos owner: &mut Owner);
            func advance(pos value: &mut u32) {{ value += 1; }}
            func observe(pos marker: &Marker) requires(trusted held(owner = marker)) {{}}
            trusted func caller(pos owner: &mut Owner, pos pointer: DataPointer<u32>)
                requires(
                    trusted held(owner = &owner.acquisition),
                    {memory_requirements}
                ) {capabilities}
            {{
                {publication}
                {operation}
                observe(&owner.acquisition);
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
                &diagnostics,
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
        }
    }
}

#[test]
fn borrowed_completion_keeps_the_selected_static_instance() {
    for (observed, valid) in [("u32", true), ("u64", false)] {
        let source = format!(
            r#"
            trusted module app;
            using core.memory;
            struct Marker {{}}
            struct Owner {{ marker: Marker; storage: Uninit<u32>; }}
            static OWNED<T>: Owner = {{ marker = Marker {{}}, storage = core.memory.uninit<u32>() }};
            trusted predicate held(owner: &Marker);
            trusted func grant(pos owner: &Owner)
                ensures(trusted held(owner = &owner.marker)) executes(pure, total) {{}}
            func observe(pos owner: &Owner) requires(trusted held(owner = &owner.marker)) {{}}
            func caller() {{
                grant(&OWNED<u32>);
                observe(&OWNED<{observed}>);
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
    }
}

#[test]
fn nested_buffer_views_preserve_checked_storage_effects() {
    let application = crate::test_support::production_standard_library_compilation(
        crate::SelectedTarget::baseline(),
        r#"
        trusted module app;
        using std.memory;
        struct Row { mut slots: std.memory.RawBuffer<u32>; }
        trusted func inspect_buffer(pos buffer: &mut std.memory.RawBuffer<Row>, pos index: usize) -> &mut u32
            uses(raw_memory)
        {
            let rows = trusted std.memory.initialized_slice_mut<Row>(buffer);
            let slots = trusted std.memory.initialized_slice_mut<u32>(&mut rows[index].slots);
            return &mut slots[0];
        }
        "#,
    );

    assert!(!application.syntax_tree_result().diagnostics().has_errors());

    let key = source_function_body_key(&application, "inspect_buffer");
    let checked = application.lowered_unit(key.clone()).unwrap();

    assert!(!checked.diagnostics().has_errors(), "{checked:#?}");

    let flow = application.storage_flow(key).unwrap();

    assert!(!flow.value().is_recovered());
    assert!(flow.value().dependencies().preserves_storage_on_normal_return());
}

#[test]
fn entry_slot_authority_preserves_its_pointer_without_granting_unowned_memory_access() {
    for (body, valid) in [
        ("opaque(); observe_slot(&pointer);", true),
        ("overwrite(other); observe_slot(&pointer);", true),
        (
            "_ = trusted publish(&pointer); opaque(); observe_slot(&pointer);",
            false,
        ),
    ] {
        let source = format!(
            r#"
            trusted module app;
            using core.memory;
            using core.target;
            trusted predicate slot(owner: &DataPointer<u32>);
            extern func opaque() executes(total);
            func observe_slot(pos owner: &DataPointer<u32>) requires(trusted slot(owner = owner)) executes(pure, total) {{}}
            trusted func publish(pos owner: &DataPointer<u32>) -> usize uses(intrinsic) {{
                return trusted core.target.expose_address<DataPointer<u32>>(core.memory.address_of<DataPointer<u32>>(owner));
            }}
            trusted func overwrite(pos pointer: DataPointer<u64>)
                requires(
                    trusted core.memory.valid_write<u64>(pointer = pointer, count = 1),
                    trusted core.memory.aligned_for<u64>(pointer = pointer)
                ) uses(raw_memory, unchecked_init) {{ trusted core.memory.write<u64>(pointer, 0); }}
            func caller(pos pointer: DataPointer<u32>, pos other: DataPointer<u64>)
                requires(
                    trusted slot(owner = &pointer),
                    trusted core.memory.valid_write<u64>(pointer = other, count = 1),
                    trusted core.memory.aligned_for<u64>(pointer = other)
                ) {{ {body} }}
        "#
        );

        let application = compilation(&source);

        assert!(
            !application.syntax_tree_result().diagnostics().has_errors(),
            "{source}"
        );

        let checked = application
            .lowered_unit(source_function_body_key(&application, "caller"))
            .expect("entry slot authority retains its actual pointer owner");

        assert_eq!(
            !checked.diagnostics().has_errors(),
            valid,
            "{source}: {checked:#?}"
        );
    }

    let source = r#"
        trusted module app;
        using core.memory;
        extern func opaque() executes(total);
        trusted predicate slot(owner: &DataPointer<u32>);
        trusted func caller(pos pointer: DataPointer<u32>) -> u32
            requires(
                trusted slot(owner = &pointer),
                trusted core.memory.valid_read<u32>(pointer = pointer, count = 1),
                trusted core.memory.aligned_for<u32>(pointer = pointer),
                trusted core.memory.initialized_as<u32>(pointer = pointer)
            )
            uses(raw_memory)
        {
            opaque();
            return core.memory.read<u32>(pointer);
        }
    "#;

    let application = compilation(source);

    assert!(
        !application.syntax_tree_result().diagnostics().has_errors(),
        "{source}"
    );

    let checked = application
        .lowered_unit(source_function_body_key(&application, "caller"))
        .expect("unowned memory permissions retain their checked failure");

    assert!(
        checked
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.kind()
                == DiagnosticKind::CheckingTrustedObligationNotProven),
        "{checked:#?}"
    );
}

#[test]
fn guarded_owner_authority_uses_entry_values_after_mutation_and_opaque_calls() {
    for (initial, valid) in [(true, true), (false, false)] {
        let source = format!(
            r#"
            trusted module app;
            struct Marker {{}}
            struct Owner {{ acquisition: Marker; }}
            trusted predicate live(owner: &Marker);
            trusted func acquire(pos enabled: bool) -> Owner
                when(enabled) {{ ensures(trusted live(owner = &result.acquisition)) }}
                executes(pure, total)
            {{ return {{ acquisition = Marker {{}} }}; }}
            extern func opaque() executes(total);
            func release_owner(pos owner: Owner) requires(trusted live(owner = &owner.acquisition)) executes(pure, total) {{}}
            func caller() {{
                let mut enabled = {initial};
                let owner = acquire(enabled);
                enabled = !enabled;
                opaque();
                release_owner(owner);
            }}
        "#
        );

        let application = compilation(&source);

        assert!(
            !application.syntax_tree_result().diagnostics().has_errors(),
            "{source}"
        );

        let checked = application
            .lowered_unit(source_function_body_key(&application, "caller"))
            .expect("owner guards retain their actual entry values");

        assert_eq!(
            !checked.diagnostics().has_errors(),
            valid,
            "{source}: {checked:#?}"
        );
    }
}

#[test]
fn callable_fields_borrow_their_owning_context_in_contracts() {
    for (predicate, subject) in [
        ("trusted predicate held(owner: &Owner);", "owner"),
        ("trusted predicate held(owner: &Token);", "&owner.token"),
    ] {
        let retirement_subject = if subject == "owner" {
            "&owner"
        } else {
            subject
        };

        for (body, valid) in [
            ("trusted owner.bind(owner);", true),
            (
                "let fake: Owner = { token = Token {}, bind = owner.bind, retire = owner.retire }; trusted fake.bind(&fake);",
                false,
            ),
        ] {
            let source = format!(
                r#"
                trusted module app;
                struct Token {{}}
                {predicate}
                callable Bind = trusted func(pos owner: &Owner)
                    requires(trusted held(owner = {subject}))
                    ensures(trusted held(owner = {subject}));
                callable Retire = trusted func(pos owner: Owner)
                    requires(trusted held(owner = {retirement_subject}));
                struct Owner {{ token: Token; bind: Bind; retire: Retire; }}
                trusted func caller(pos owner: &Owner)
                    requires(trusted held(owner = {subject}))
                    ensures(trusted held(owner = {subject}))
                {{ {body} }}
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

    for (body, valid) in [
        ("let retire = owner.retire; trusted retire(owner);", true),
        (
            "let fake: Owner = { retire = owner.retire }; trusted fake.retire(fake);",
            false,
        ),
    ] {
        let source = format!(
            r#"
            trusted module app;
            trusted predicate held(owner: &Owner);
            callable Retire = trusted func(pos owner: Owner)
                requires(trusted held(owner = &owner));
            struct Owner {{ retire: Retire; }}
            trusted func caller(pos owner: Owner)
                requires(trusted held(owner = &owner))
            {{ {body} }}
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
fn caught_results_preserve_only_the_successful_owner() {
    for (body, valid) in [
        ("let value = acquire(); forward(value);", true),
        (
            "let caught = catch acquire(); match consume caught { case Ok(value) { forward(value); } case Error(_) {} }",
            true,
        ),
        (
            "let caught = catch { yield acquire(); }; match consume caught { case Ok(value) { forward(value); } case Error(_) {} }",
            true,
        ),
        (
            "let outer = catch catch acquire(); match consume outer { case Ok(inner) { match consume inner { case Ok(value) { forward(value); } case Error(_) {} } } case Error(_) {} }",
            true,
        ),
        (
            "let caught = catch acquire(); let value = match consume caught { case Ok(value) { yield value; } case Error(_) { return; } }; forward(value);",
            true,
        ),
        (
            "let caught = catch acquire(); match consume caught { case Ok(_) { let fake: Guard = { active = true }; forward(fake); } case Error(_) {} }",
            false,
        ),
        (
            "let caught = catch acquire(); match consume caught { case Ok(_) {} case Error(_) { let fake: Guard = { active = true }; forward(fake); } }",
            false,
        ),
    ] {
        let body = body.replace("acquire()", "acquire(fail)");

        let source = format!(
            r#"
            trusted module app;
            struct Guard {{ active: bool; }}
            trusted predicate held(owner: &(Guard?));
            trusted func acquire(pos fail: bool) -> Guard?
                ensures(trusted held(owner = &result))
            {{ if fail {{ panic("acquisition failed"); }} let owner: Guard = {{ active = true }}; return owner; }}
            func forward(pos owner: Guard?) requires(trusted held(owner = &owner)) {{}}
            func caller(pos fail: bool) {{ {body} }}
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
                .expect("fixture callable must resolve its execution properties");

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
fn borrowed_scalar_authority_tracks_storage_independently_of_copied_values() {
    for (parameter, initial, action, valid) in [
        (
            "",
            "let mut value: u8 = 1; let mut other: u8 = 1;",
            "observe(&value);",
            true,
        ),
        (
            "",
            "let mut value: u8 = 1; let mut other: u8 = 1;",
            "observe(&other);",
            false,
        ),
        (
            "",
            "let mut value: u8 = 1; let mut other: u8 = 1;",
            "other = 2; observe(&value);",
            true,
        ),
        (
            "",
            "let mut value: u8 = 1;",
            "value = 2; observe(&value);",
            false,
        ),
        (
            "",
            "let mut value: u8 = 1;",
            "value = 1; observe(&value);",
            false,
        ),
        (
            "",
            "let mut value: u8 = 1;",
            "value = 2; value = 1; observe(&value);",
            false,
        ),
        (
            "",
            "let mut value: u8 = 1;",
            "value = 2; establish(&value); observe(&value);",
            true,
        ),
        (
            "pos mut value: u8",
            "let mut other = value;",
            "observe(&value);",
            true,
        ),
        (
            "pos mut value: u8",
            "let mut other = value;",
            "observe(&other);",
            false,
        ),
        (
            "pos mut value: u8",
            "let mut other = value;",
            "other = 2; observe(&value);",
            true,
        ),
        (
            "pos mut value: u8",
            "let mut other = value;",
            "value = other; observe(&value);",
            false,
        ),
    ] {
        let source = format!(
            r#"
            trusted module app;
            trusted predicate live(value: &u8);
            trusted func establish(pos value: &u8) executes(pure, total)
                ensures(trusted live(value)) {{}}
            func observe(pos value: &u8) requires(trusted live(value)) {{}}
            func caller({parameter}) {{ {initial} establish(&value); {action} }}
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
fn borrowed_postconditions_name_the_updated_storage_only() {
    let mut failures = Vec::new();
    let mut completed = 0;

    for (case, (action, observed, valid)) in [
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
            "{ let mut writer: Writer = { slot = &mut first }; forward(&mut writer); observe(&writer.slot); }",
            "first",
            true,
        ),
        (
            "{ let mut writer: Writer = { slot = &mut first }; forward(&mut writer); writer.slot.epoch = 2; observe(&writer.slot); }",
            "first",
            false,
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
    ].into_iter().enumerate() {
        let forward = if action.contains("forward(") { r#"
            func forward(pos writer: &mut Writer)
                ensures(trusted live(&writer.slot))
            {
                set(writer);
            }
        "# } else { "" };
        let source = format!(
            r#"
            trusted module app;

            struct Owner
            {{
                mut epoch: u64;
            }}

            trusted predicate live(value: &Owner);

            struct Writer
            {{
                slot: &mut Owner;

                trusted destruct()
                    ensures(trusted live(&self.slot))
                {{
                    self.slot.epoch = 1;
                }}
            }}

            trusted func set(pos writer: &mut Writer)
                ensures(trusted live(&writer.slot))
            {{
                writer.slot.epoch = 1;
            }}

            {forward}

            func observe(pos owner: &Owner)
                requires(trusted live(owner))
            {{
            }}

            func caller()
            {{
                let mut first: Owner = {{ epoch = 0 }};
                let second: Owner = {{ epoch = 0 }};

                {action}
                observe(&{observed});
            }}
        "#
        );

        let application = compilation(&source);
        let diagnostics = application.check_diagnostics();

        let boundaries = diagnostics.iter().flat_map(|diagnostic| diagnostic.args())
            .filter_map(|argument| match argument.value() {
                DiagnosticArgValue::ContractObligation(obligation) => Some(obligation.kind),
                _ => None,
            }).collect::<Vec<_>>();
        completed += 1;

        if diagnostics.has_errors() == valid {
            failures.push(format!("nested borrowed case={case}, valid={valid}, boundaries={boundaries:?}"));
        }

        if !valid && !boundaries.contains(&DiagnosticContractObligationKind::CallRequirement) {
            failures.push(format!("nested borrowed case={case} did not reject the caller independently: {boundaries:?}"));
        }
    }

    assert!(failures.is_empty(), "nested borrowed cases completed: count={completed}\n{}", failures.join("\n"));
}

#[test]
fn scoped_borrow_postconditions_survive_capability_retirement() {
    let mut failures = Vec::new();

    for asynchronous in ["", "async"] {
    for (case, (action, promised, valid)) in [
        ("observe(&first);", true, true),
        ("let moved = relay(first); observe(&moved);", true, true),
        ("first.epoch = 2; observe(&first);", true, false),
        ("observe(&second);", true, false),
        ("observe(&first);", false, false),
    ].into_iter().enumerate() {
        let guarantee = if promised { "ensures(trusted live(&lease.slot))" } else { "" };
        let source = format!(r#"
            trusted module app;

            struct Owner
            {{
                mut epoch: u64;
            }}

            trusted predicate live(value: &Owner);

            struct Lease
            {{
                slot: &mut Owner;
            }}

            struct Scope
            {{
                slot: &mut Owner;

                trusted consume enter() -> Lease
                    ensures(trusted live(&result.slot))
                {{
                    return {{ slot = self.slot }};
                }}

                trusted {asynchronous} exit(pos lease: Lease)
                    {guarantee}
                {{
                    lease.slot.epoch = 1;
                }}
            }}

            func relay(pos owner: Owner) -> Owner
                requires(trusted live(&owner))
                ensures(trusted live(&result))
            {{
                return owner;
            }}

            func observe(pos owner: &Owner)
                requires(trusted live(owner))
            {{
            }}

            {asynchronous} func caller()
            {{
                let mut first: Owner = {{ epoch = 0 }};
                let second: Owner = {{ epoch = 0 }};
                let scope: Scope = {{ slot = &mut first }};

                with lease = scope
                {{
                    observe(&lease.slot);
                }};

                {action}
            }}
        "#);
        let application = compilation(&source);
        let diagnostics = application.check_diagnostics();
        let kinds = diagnostics.iter().map(|diagnostic| diagnostic.kind()).collect::<Vec<_>>();

        if diagnostics.has_errors() == valid {
            failures.push(format!("scoped borrow asynchronous={asynchronous:?}, case={case}, valid={valid}, diagnostics={kinds:?}"));
        }

        if !valid && !kinds.contains(&DiagnosticKind::CheckingTrustedObligationNotProven) {
            failures.push(format!("scoped borrow asynchronous={asynchronous:?}, case={case} lacks its authority rejection: {kinds:?}"));
        }
    }

    }

    assert!(failures.is_empty(), "{}", failures.join("\n"));
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
            .expect("fixture callable must complete storage-flow analysis");

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
#[ignore = "tier:proof"]
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
        .expect("fixture callable must lower successfully");

    assert!(
        !checked.diagnostics().has_errors(),
        "{:?}",
        checked.diagnostics()
    );
}

#[test]
#[ignore = "tier:proof"]
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
            .expect("fixture callable must lower successfully");

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
#[ignore = "tier:proof"]
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
                .expect("fixture callable must lower successfully");

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
    let symbols = compilation
        .symbol_graph()
        .expect("fixture compilation must build its symbol graph");

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
#[ignore = "tier:proof"]
fn production_lock_declarations_check_their_own_bodies() {
    let compilation = production_sync_compilation(
        r#"module app;

using core.target;
"#,
    );

    let mut failures = Vec::new();

    for key in compilation
        .declared_unit_keys_for_test()
        .expect("fixture declarations must resolve their units")
    {
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
            .expect("fixture `semantic_unit_diagnostics_with_cancellation` call must succeed");

        if diagnostics.has_errors() {
            failures.push(format!("{key:?}: {diagnostics:?}"));
            continue;
        }

        let checked = compilation
            .lowered_unit(key.clone())
            .expect("fixture callable must lower successfully");

        if checked.diagnostics().has_errors() {
            failures.push(format!("{key:?}: {:?}", checked.diagnostics()));
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
#[ignore = "tier:proof"]
fn consuming_guard_release_keeps_its_destructors_reference_effects() {
    let application = production_sync_compilation("module app;");

    let symbols = application
        .symbol_graph()
        .expect("fixture compilation must build its symbol graph");

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

    let template = callable_dependencies(&application, callable.into());

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
#[ignore = "tier:proof"]
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
        .expect("fixture callable must lower successfully");

    assert!(
        !checked.diagnostics().has_errors(),
        "{:?}",
        checked.diagnostics()
    );
}

#[test]
#[ignore = "tier:proof"]
fn production_synchronization_declarations_check_their_own_bodies() {
    let compilation = production_sync_compilation(
        r#"module app;

using core.target;
"#,
    );

    for key in compilation
        .declared_unit_keys_for_test()
        .expect("fixture declarations must resolve their units")
    {
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

        let certified = compilation
            .execution_properties(key.clone())
            .expect("fixture callable must resolve its execution properties");

        assert!(
            !certified.diagnostics().has_errors(),
            "{key:?}: {:?}",
            certified.diagnostics()
        );

        let checked = compilation
            .lowered_unit(key.clone())
            .expect("fixture callable must lower successfully");

        assert!(
            !checked.diagnostics().has_errors(),
            "{key:?}: {:?}",
            checked.diagnostics()
        );
    }
}

#[test]
fn production_condition_variable_preserves_its_borrowed_queue_authority() {
    let compilation = production_sync_compilation("module app;");
    let mut failures = Vec::new();
    let mut bodies = 0;

    for key in compilation.declared_unit_keys_for_test().expect("production bodies are declared") {
        if key.kind() != bray_bound_tree::BoundUnitKind::CallableBody
            || !body_belongs_to(&compilation, &key, &[], &["ConditionVariable"]) {
            continue;
        }

        bodies += 1;
        let properties = compilation.execution_properties(key.clone()).expect("production body is certified");
        let checked = compilation.lowered_unit(key.clone()).expect("production body lowers");

        if properties.diagnostics().has_errors() || checked.diagnostics().has_errors() {
            failures.push((key, properties.diagnostics().clone(), checked.diagnostics().clone()));
        }
    }

    assert!(bodies > 0, "the actual condition-variable bodies must be selected");
    assert!(failures.is_empty(), "{failures:?}");
}

#[test]
#[ignore = "tier:proof"]
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
        .expect("fixture callable must lower successfully");

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
fn invoking_a_callable_does_not_publish_its_code_address() {
    let application = production_sync_compilation(
        r#"
        trusted module app;
        using core.memory;
        using std.memory;
        callable Callback = @abi(c) func();
        trusted func invoke(pos pointer: FunctionPointer<Callback>)
            requires(trusted core.memory.callable_address_valid<Callback>(pointer = pointer))
            uses(layout_reinterpret, foreign_call)
        {
            let operation = trusted std.memory.callable_from_pointer<Callback>(pointer);
            operation();
        }
    "#,
    );

    let checked = application
        .lowered_unit(source_function_body_key(&application, "invoke"))
        .expect("callable invocation must lower successfully");

    assert!(
        !checked.diagnostics().has_errors(),
        "{:?}",
        checked.diagnostics()
    );

    let callable = crate::test_support::source_function(&application, "invoke");
    let template = callable_dependencies(&application, callable.into());

    assert!(
        !template.effects().iter().any(|effect| matches!(
            effect,
            bray_symbols::DependencyStorageEffect::Escape
                | bray_symbols::DependencyStorageEffect::Direct {
                    kind: bray_symbols::DependencyMutationKind::Escape,
                    ..
                }
                | bray_symbols::DependencyStorageEffect::Indirect {
                    kind: bray_symbols::DependencyMutationKind::Escape,
                    ..
                }
        )),
        "capture-free invocation must not publish its code address: {template:#?}"
    );

    assert!(
        !template.preserves_storage_on_normal_return(),
        "foreign invocation still has opaque effects"
    );
}

#[test]
fn rewritten_raw_pointer_fields_publish_opaque_output_identities() {
    let application = production_sync_compilation(
        r#"
        trusted module app;
        using core.memory;
        using core.target;
        struct Cursor { mut pointer: DataPointer<u8>; }
        trusted func replace_pointer(pos cursor: &mut Cursor, pos address: usize)
            uses(intrinsic)
        {
            cursor.pointer = trusted core.target.from_exposed_address<u8>(address = address);
        }
    "#,
    );

    let checked = application
        .lowered_unit(source_function_body_key(&application, "replace_pointer"))
        .expect("pointer replacement must lower successfully");

    assert!(
        !checked.diagnostics().has_errors(),
        "{:?}",
        checked.diagnostics()
    );

    let callable = crate::test_support::source_function(&application, "replace_pointer");
    let template = callable_dependencies(&application, callable.into());

    let [reference] = template.borrows() else {
        panic!("rewritten pointer retains one typed output identity: {template:#?}");
    };

    assert_eq!(reference.entry(), None);

    assert_eq!(
        reference.kind(),
        bray_symbols::DependencyReferenceKind::Address
    );

    assert_eq!(
        reference.exit().subject_root(),
        bray_symbols::DependencySubjectRoot::Parameter(bray_symbols::SymbolOrdinal::new(0))
    );

    assert_eq!(reference.exit().projections().len(), 1);
    assert!(!reference.is_exact());
}

#[test]
#[ignore = "tier:proof"]
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
        .expect("fixture callable must lower successfully");

    assert!(
        !checked.diagnostics().has_errors(),
        "{:?}",
        checked.diagnostics()
    );

    let callable = crate::test_support::source_function(&application, "exact");

    let template = callable_dependencies(&application, callable.into());

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
#[ignore = "tier:proof"]
fn production_guard_borrow_origin_reaches_the_protected_payload() {
    let application = production_sync_compilation("module app;");

    let symbols = application
        .symbol_graph()
        .expect("fixture compilation must build its symbol graph");

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

    let template = callable_dependencies(&application, callable.into());

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
            symbols
                .member_name((*field).into())
                .expect("fixture `member_name` call must succeed")
                .as_str(),
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
        ("let original = make_stamp(); let copied = original;", false),
        ("let original = make_stamp(); let copied = (0, original);", false),
        ("let original = make_stamp(); discard_stamp(original);", false),
    ] {
        let source = format!(
            r#"
            trusted module app;
            struct Marker {{}}
            struct Guard {{ marker: Marker; }}
            @copy struct Stamp {{ epoch: u32; }}
            trusted predicate held(marker: &Marker);
            trusted predicate stamped(owner: &Stamp);
            trusted func make_stamp() -> Stamp
                ensures(trusted stamped(&result)) executes(pure, total)
            {{ return {{ epoch = 1 }}; }}
            func discard_stamp(pos value: Stamp) {{}}
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
    for (input, subject) in [("Guard", "guard"), ("Envelope", "guard.inner")] {
        for (result, body, valid) in [
            ("Guard", "return guard;", true),
            ("Marker", "return guard.marker;", false),
            (
                "Guard",
                "return { owner = other, marker = guard.marker };",
                false,
            ),
        ] {
            let body = body.replace("guard", subject);

            let borrowed_subject = subject.replace("guard", "whole");

            let source = format!(
                r#"
                trusted module app;
                struct Lock {{ tag: u32; }}
                struct Marker {{}}
                struct Guard {{ owner: &Lock; marker: Marker; }}
                struct Envelope {{ inner: Guard; label: u32; }}
                trusted predicate held(marker: &Marker);
                func preserve(pos whole: &{input})
                    requires(trusted held(&{borrowed_subject}.marker))
                    ensures(trusted held(&{borrowed_subject}.marker))
                    executes(pure, total) {{}}
                func transfer(pos guard: {input}, pos other: &Lock) -> {result}
                    requires(trusted held(&{subject}.marker)) {{ preserve(&guard); {body} }}
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
}

#[test]
fn input_postconditions_retain_the_complete_owner_after_replacement() {
    for (input, replacement, subject) in [
        ("Parent", "replace_child(&mut parent)", "parent.child"),
        ("Child", "replace_root(&mut parent)", "parent"),
    ] {
        for (output, expression, valid) in [
            ("Child", subject.to_owned(), true),
            ("Marker", format!("{subject}.marker"), false),
        ] {
            let source = format!(r#"
                trusted module app;
                struct Marker {{}}
                struct Child {{ marker: Marker; number: u32; }}
                struct Parent {{ mut child: Child; }}
                trusted predicate held(marker: &Marker);
                trusted func replace_child(pos parent: &mut Parent)
                    ensures(trusted held(marker = &parent.child.marker))
                {{ parent.child = Child {{ marker = Marker {{}}, number = 0 }}; }}
                extern trusted func replace_root(pos child: &mut Child)
                    ensures(trusted held(marker = &child.marker));
                func caller(pos mut parent: {input}) -> {output}
                {{ trusted {replacement}; return {expression}; }}
            "#);

            let application = compilation(&source);
            let diagnostics = application.check_diagnostics();

            assert_eq!(!diagnostics.has_errors(), valid, "{source}: {diagnostics:#?}");

            if !valid {
                bray_testing::assert_goal_state_diagnostic_kind(
                    diagnostics,
                    DiagnosticKind::CheckingTrustedWitnessTransferNotProven,
                );
            }
        }
    }
}

#[test]
fn field_projection_preserves_the_evidence_of_a_borrowed_call_result() {
    for binding in [
        "let token = &project(owner).token;",
        "let value = project(owner); let token = &value.token;",
    ] {
        let source = format!(
            r#"
            trusted module app;
            struct Token {{}}
            struct Owner {{ token: Token; }}
            struct Holder {{ current: Owner; label: u32; }}
            extern func opaque();
            impl Owner {{ func check() executes(pure, total) {{}} }}
            trusted predicate live(owner: &Token);
            func project(pos owner: &Owner) -> &Owner
                requires(trusted live(&owner.token))
                ensures(trusted live(&result.token))
                executes(pure, total)
            {{ return owner; }}
            func observe(pos token: &Token)
                requires(trusted live(token)) executes(pure, total) {{}}
            trusted func nested(pos owner: &Holder) -> &Holder
                ensures(trusted live(&result.current.token))
                executes(pure, total)
            {{ return owner; }}
            extern trusted func introduced(pos owner: &Holder) -> &Holder
                ensures(trusted live(&result.current.token));
            func nested_caller(pos owner: &Holder)
            {{
                let selected = nested(owner);
                opaque();
                observe(&selected.current.token);

                let fresh = introduced(owner);

                opaque();
                observe(&fresh.current.token);
            }}
            func caller(pos owner: &Owner)
                requires(trusted live(&owner.token))
            {{
                {binding}
                observe(token);
                project(owner).check();
            }}
            "#
        );

        let application = compilation(&source);
        let diagnostics = application.check_diagnostics();

        assert!(!diagnostics.has_errors(), "{source}: {diagnostics:#?}");
    }
}

#[test]
fn const_borrowed_getters_preserve_the_existing_carrier_identity() {
    let source = r#"
        trusted module app;
        struct Marker {}
        struct Owner
        {
            marker: Marker;
            const func token() -> &Marker executes(pure, total) { return &self.marker; }
            func inspect_after_argument(pos sequence: u32)
                requires(trusted live(&self.marker)) executes(pure, total) {}
        }
        union Backing { Empty; Owned(pos owner: Owner); }
        trusted predicate live(owner: &Marker);
        const func owner(pos backing: &Backing) -> &Owner
            requires(backing matches Backing.Owned(_))
            executes(pure, total)
        {
            return match backing
            {
                case Backing.Owned(value) { yield &value; }
                case Backing.Empty { panic("missing owner"); }
            };
        }
        predicate retained(backing: &Backing) = backing matches Backing.Owned(_) && live(owner(backing).token());
        func observe(pos marker: &Marker) requires(trusted live(marker)) executes(pure, total) {}
        func argument_value() -> u32 executes(pure, total) { return 0; }
        func caller(pos backing: &Backing) requires(trusted retained(backing))
        {
            if backing matches Backing.Owned(_)
            {
                observe(owner(backing).token());
                let selected = owner(backing);
                observe(selected.token());
                owner(backing).inspect_after_argument(argument_value());
                selected.inspect_after_argument(argument_value());
                let copied = selected;
                copied.inspect_after_argument(argument_value());
            }
        }
    "#;

    let application = compilation(source);
    let diagnostics = application.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:#?}");
}

#[test]
fn raw_entry_addresses_do_not_replace_borrowed_owners_at_completion() {
    for (body, valid) in [
        (
            r#"
            let recognize = trusted lambda(pos input: &DataPointer<Owner>, pos address: DataPointer<Owner>)
                requires(trusted slot(pointer = address))
                ensures(trusted borrowed_slot(input = input))
                executes(pure, total) {};
            let observe = lambda(pos input: &DataPointer<Owner>)
                requires(trusted borrowed_slot(input = input))
                executes(pure, total) {};
            trusted recognize(&pointer, pointer);
            observe(&pointer);
            "#,
            true,
        ),
        (
            "let owner = borrow_owner(&pointer, pointer); visit(owner, callback); visit(owner, callback);",
            true,
        ),
        (
            "let owner = borrow_owner(&pointer, pointer); loop { visit(owner, callback); if !again { return; } }",
            true,
        ),
        (
            "let owner = borrow_owner(&pointer, pointer); clear_slot(capability, pointer); visit(owner, callback);",
            true,
        ),
        (
            "let owner = borrow_owner(&pointer, pointer); pointer = std.memory.null<Owner>(); visit(owner, callback);",
            false,
        ),
    ] {
        let source = format!(
            r#"
            trusted module app;
            using core.memory;
            struct Marker {{}}
            struct Owner {{ marker: Marker; backing: DataPointer<u8>; mut slot: u32?; }}
            callable Callback = @abi(c) trusted func();
            trusted predicate slot(pointer: DataPointer<Owner>);
            trusted predicate borrowed_slot(input: &DataPointer<Owner>);
            trusted predicate held(marker: &Marker);
            trusted func borrow_owner(pos input: &DataPointer<Owner>, pos address: DataPointer<Owner>) -> &Owner
                requires(trusted slot(pointer = address))
                ensures(trusted held(marker = &result.marker))
                executes(pure, total)
                uses(raw_memory, unchecked_alias)
            {{
                let permit = trusted lambda(pos address: DataPointer<Owner>)
                    requires(trusted slot(pointer = address))
                    ensures(
                        trusted core.memory.valid_read<Owner>(pointer = address, count = 1),
                        trusted core.memory.aligned_for<Owner>(pointer = address),
                        trusted core.memory.initialized_as<Owner>(pointer = address),
                        trusted core.memory.shared_alias_valid<Owner>(pointer = address),
                        trusted core.memory.epoch_current<Owner>(pointer = address),
                        trusted core.memory.synchronized_access<Owner>(pointer = address),
                        trusted core.memory.movement_stable<Owner>(pointer = address),
                        trusted core.memory.finalization_pending<Owner>(pointer = address)
                    )
                    executes(pure, total) {{}};
                trusted permit(address);
                return trusted std.memory.borrow_from<Owner, DataPointer<Owner>>(input, address);
            }}
            trusted func clear_slot(pos capability: &mut bool, pos address: DataPointer<Owner>)
                requires(trusted slot(pointer = address))
                uses(raw_memory, unchecked_alias)
            {{
                let permit = trusted lambda(pos address: DataPointer<Owner>)
                    requires(trusted slot(pointer = address))
                    ensures(
                        trusted core.memory.valid_write<Owner>(pointer = address, count = 1),
                        trusted core.memory.aligned_for<Owner>(pointer = address),
                        trusted core.memory.initialized_as<Owner>(pointer = address),
                        trusted core.memory.exclusive_alias_valid<Owner>(pointer = address),
                        trusted core.memory.epoch_current<Owner>(pointer = address),
                        trusted core.memory.synchronized_access<Owner>(pointer = address),
                        trusted core.memory.movement_stable<Owner>(pointer = address),
                        trusted core.memory.finalization_pending<Owner>(pointer = address)
                    )
                    executes(pure, total) {{}};
                trusted permit(address);
                let borrowed = trusted std.memory.borrow_mut_from<Owner, bool>(capability, address);
                borrowed.slot = none;
            }}
            trusted func visit(pos owner: &Owner, pos callback: Callback)
                requires(trusted held(marker = &owner.marker))
                ensures(trusted held(marker = &owner.marker))
                uses(foreign_call)
            {{
                callback();
            }}
            func caller(pos mut pointer: DataPointer<Owner>, pos callback: Callback, pos again: bool, pos capability: &mut bool)
                requires(trusted slot(pointer = pointer))
            {{
                {body}
            }}
        "#
        );

        let application = production_sync_compilation(&source);

        let checked = application
            .lowered_unit(source_function_body_key(&application, "caller"))
            .expect("fixture callable must lower successfully");

        assert_eq!(
            !checked.diagnostics().has_errors(),
            valid,
            "{source}: {:?}",
            checked.diagnostics()
        );
    }
}

#[test]
fn mutable_union_completion_observes_the_updated_owner() {
    for (preserved, valid) in [("trusted held(value = &value),", true), ("", false)] {
        let source = r#"trusted module app;
using std.memory;
struct Marker {}
struct Reference { marker: Marker; }
struct Row { mut provider: Reference; }
union Value { First(pos mut row: Row); Second(pos mut row: Row); }
trusted predicate held(value: &Value);
trusted predicate reference_held(value: &Marker);
trusted func fresh() -> Reference ensures(trusted reference_held(value = &result.marker)) executes(pure,total) { return { marker = Marker{}};}
trusted func take(pos value: &mut Value) -> Reference
    requires(trusted held(value = &value))
    ensures( $PRESERVED trusted reference_held(value = &result.marker))
{
    return match value {
        case Value.First(row) {yield trusted std.memory.replace_value<Reference>(&mut row.provider, fresh());}
        case Value.Second(row) {yield trusted std.memory.replace_value<Reference>(&mut row.provider, fresh());}
    };
}
func observe(pos value: &mut Value, pos provider: &Reference)
    requires(trusted held(value = &value), trusted reference_held(value = &provider.marker)) executes(pure,total) {}
func caller(pos mut value: Value) requires(trusted held(value = &value)) {
    let provider = trusted take(&mut value);
    observe(&mut value, &provider);
}
"#.replace("$PRESERVED", preserved);

        let application = compilation(&source);
        let diagnostics = application.check_diagnostics();

        assert_eq!(!diagnostics.has_errors(), valid, "{source}: {diagnostics:#?}");
    }
}

#[test]
fn typed_raw_transfers_preserve_only_unmoved_marker_free_owners() {
    for field in ["u64", "DataPointer<u8>"] {
        for (pointee, operation, valid) in [
            ("Owner", "let taken = trusted core.memory.read<$POINTEE>(address);", false),
            ("Other", "let taken = trusted core.memory.read<$POINTEE>(address);", true),
            ("Containing", "let taken = trusted core.memory.read<$POINTEE>(address);", false),
            ("u64", "trusted core.memory.write<u64>(address, 0);", false),
        ] {
        let source = r#"trusted module app;
using core.memory;
struct Owner { address: $FIELD; }
struct Other { value: u64; }
struct Containing { owner: Owner; }
trusted predicate live(owner: &Owner);
extern trusted func pointer(pos owner: &Owner) -> DataPointer<$POINTEE>
    requires(trusted live(owner))
    ensures(
        trusted live(owner),
        trusted core.memory.valid_read<$POINTEE>(pointer = result, count = 1),
        trusted core.memory.valid_write<$POINTEE>(pointer = result, count = 1),
        trusted core.memory.initialized_as<$POINTEE>(pointer = result),
        trusted core.memory.aligned_for<$POINTEE>(pointer = result)
    )
    executes(pure, total);
func observe(pos owner: &Owner) requires(trusted live(owner)) executes(pure, total) {}
trusted func caller(pos owner: &Owner)
    requires(trusted live(owner))
    uses(raw_memory, unchecked_init)
{
    let address = pointer(owner);
    $OPERATION
    observe(owner);
}
"#.replace("$FIELD", field).replace("$OPERATION", operation).replace("$POINTEE", pointee);

        let application = compilation(&source);
        let diagnostics = application.check_diagnostics();

        assert_eq!(!diagnostics.has_errors(), valid, "{source}: {diagnostics:#?}");
        }
    }
}

#[test]
fn typed_data_transfers_preserve_borrowed_ownership_markers() {
    let mut failures = Vec::new();

    for (pointee, value, valid) in [
        ("u32", "1", true),
        ("(u32?)", "none", true),
        ("Data", "Data { value = 1 }", true),
        ("OtherPayload", "OtherPayload { marker = OtherMarker {}, value = 1 }", true),
        ("[u32; 2]", "[1, 2]", true),
        ("Uninit<[u32; 2]>", "core.memory.uninit_from<[u32; 2]>([1, 2])", true),
        ("Uninit<Payload>", "core.memory.uninit_from<Payload>(Payload { marker = Marker {}, value = 1 })", false),
        (
            "Payload",
            "Payload { marker = Marker {}, value = 1 }",
            false,
        ),
    ] {
        for (operation, capabilities) in [
            (
                format!("trusted core.memory.write<{pointee}>(destination, {value});"),
                "raw_memory, unchecked_init",
            ),
            (
                format!("_ = trusted core.memory.read<{pointee}>(destination);"),
                "raw_memory",
            ),
        ] {
            for (borrowed, mutable, expose, call) in [
                ("owner", "mut ", "_ = core.memory.address_of<Owner>(owner);", None),
                ("forward(owner)", "mut ", "_ = core.memory.address_of<Owner>(owner);", None),
                ("forward(owner)", "", "", None),
                ("forward(owner)", "", "", Some("trusted transfer(destination);")),
                ("forward(owner)", "", "", Some("trusted forward_transfer(borrowed);")),
                ("forward(owner)", "", "", Some("trusted transfer_external();")),
                ("forward(owner)", "", "", Some("trusted transfer_unresolved(address);")),
            ] {
            let invoke = call.unwrap_or(operation.as_str());
            let caller_capabilities = if call.is_some() { String::new() } else { format!("uses({capabilities})") };

            let source = format!(
                r#"
            trusted module app;
            using core.memory;
            using core.target;
            struct Marker {{}}
            struct OtherMarker {{}}
            struct Data {{ value: u32; }}
            struct Payload {{ marker: Marker; value: u32; }}
            struct OtherPayload {{ marker: OtherMarker; value: u32; }}
            struct Owner {{
                marker: Marker;
                {mutable}data: u64;
                trusted destruct() requires(trusted held(&self.marker)) executes(total) {{}}
            }}
            trusted predicate held(marker: &Marker);
            extern trusted func pointer(pos owner: &Owner) -> DataPointer<{pointee}>
                requires(trusted held(&owner.marker))
                ensures(
                    trusted held(&owner.marker),
                    trusted core.memory.valid_write<{pointee}>(pointer = result, count = 1),
                    trusted core.memory.valid_read<{pointee}>(pointer = result, count = 1),
                    trusted core.memory.initialized_as<{pointee}>(pointer = result),
                    trusted core.memory.aligned_for<{pointee}>(pointer = result)
                )
                executes(pure, total);
            trusted predicate known_address(address: usize);
            extern trusted func external_pointer() -> DataPointer<{pointee}>
                ensures(
                    trusted core.memory.valid_write<{pointee}>(pointer = result, count = 1),
                    trusted core.memory.valid_read<{pointee}>(pointer = result, count = 1),
                    trusted core.memory.initialized_as<{pointee}>(pointer = result),
                    trusted core.memory.aligned_for<{pointee}>(pointer = result)
                ) executes(pure, total);
            trusted func transfer_external() uses({capabilities})
            {{
                let destination = external_pointer();
                {operation}
            }}
            trusted func transfer_unresolved(pos address: usize)
                requires(trusted known_address(address))
                uses(intrinsic, {capabilities})
            {{
                let destination = core.target.from_exposed_address<{pointee}>(address = address);
                let permit = trusted lambda(pos destination: DataPointer<{pointee}>, pos address: usize)
                    requires(trusted known_address(address))
                    ensures(
                        trusted core.memory.valid_write<{pointee}>(pointer = destination, count = 1),
                        trusted core.memory.valid_read<{pointee}>(pointer = destination, count = 1),
                        trusted core.memory.initialized_as<{pointee}>(pointer = destination),
                        trusted core.memory.aligned_for<{pointee}>(pointer = destination)
                    ) executes(pure, total) {{}};
                trusted permit(destination, address);
                {operation}
            }}
            trusted func transfer(pos destination: DataPointer<{pointee}>)
                requires(
                    trusted core.memory.valid_write<{pointee}>(pointer = destination, count = 1),
                    trusted core.memory.valid_read<{pointee}>(pointer = destination, count = 1),
                    trusted core.memory.initialized_as<{pointee}>(pointer = destination),
                    trusted core.memory.aligned_for<{pointee}>(pointer = destination)
                ) uses({capabilities}) {{ {operation} }}
            trusted func transfer_owner(pos owner: &Owner)
                requires(trusted held(&owner.marker))
                uses({capabilities})
            {{
                let destination = pointer(owner);
                {operation}
            }}
            trusted func forward_transfer(pos owner: &Owner)
                requires(trusted held(&owner.marker))
            {{
                trusted transfer_owner(owner);
            }}
            func forward(pos owner: &Owner) -> &Owner
                requires(trusted held(&owner.marker))
                ensures(trusted held(&result.marker)) executes(pure, total) {{ return owner; }}
            func observe(pos owner: &Owner)
                requires(trusted held(&owner.marker)) executes(pure, total) {{}}
            trusted func caller(pos owner: &Owner, pos address: usize)
                requires(trusted held(&owner.marker), trusted known_address(address))
                {caller_capabilities}
            {{
                {expose}
                let borrowed = {borrowed};
                let destination = pointer(borrowed);
                {invoke}
                observe(borrowed);
            }}
            "#
            );

            let application = compilation(&source);
            let diagnostics = application.check_diagnostics();

            if diagnostics.has_errors() == valid {
                failures.push(format!("{source}: {diagnostics:#?}"));
                continue;
            }

            if !valid {
                bray_testing::assert_goal_state_diagnostic_kind(
                    diagnostics,
                    DiagnosticKind::CheckingTrustedObligationNotProven,
                );
            }
            }
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
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
            "let index: usize = if flag { yield 0; } else { yield 1; }; guard.payload.counts[index] += 1; opaque();",
        ] {
            let source = format!(
                r#"
                trusted module app;
                struct Marker {{}}
                struct Payload {{ mut value: bool; mut counts: [u32; 2]; }}
                struct Guard {{ marker: Marker; payload: &mut Payload; }}
                trusted predicate held(value: &{ty});
                trusted func acquire(pos payload: &mut Payload) -> Guard
                    ensures(trusted held({result})) executes(pure, total)
                {{ return {{ marker = Marker {{}}, payload = payload }}; }}
                func observe(pos guard: &Guard)
                    requires(trusted held({argument})) executes(pure, total) {{}}
                extern func opaque();
                func caller(pos flag: bool) {{
                    let zero: u32 = 0;
                    let mut payload: Payload = {{ value = false, counts = [zero, zero] }};
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
            .expect("fixture callable must resolve its execution properties");

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
    for (mutated, right_type, field, valid) in [
        ("left", "&mut Pair", "first", true),
        ("right", "&mut Pair", "first", true),
        ("left", "&mut Pair", "second", false),
        ("right", "&mut Pair", "second", true),
        ("right", "&mut (&mut Pair)", "first", false),
    ] {
        let source = format!(
            r#"
            trusted module app;
            struct Cell {{ mut value: bool; }}
            struct Pair {{ mut first: Cell; mut second: Cell; }}
            trusted predicate held(value: &Cell);
            func observe(pos value: &Cell)
                requires(trusted held(value)) executes(pure, total) {{}}
            func caller(pos left: &mut Pair, pos right: {right_type})
                requires(trusted held(&left.second)) {{
                {mutated}.{field}.value = false;
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
#[ignore = "tier:proof"]
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
            .expect("fixture callable must lower successfully");

        assert!(
            checked.diagnostics().has_errors(),
            "accepted forged authority: {source}"
        );

        bray_testing::assert_goal_state_diagnostic_kind(checked.diagnostics(), expected);
    }
}

#[test]
#[ignore = "tier:proof"]
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
        .expect("fixture callable must lower successfully");

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
#[ignore = "tier:exhaustive"]
fn native_atomic_word_declarations_check_for_every_target() {
    for target in bray_target::NativeTarget::ALL {
        let compilation = production_sync_compilation_for_target(
            "module app;",
            crate::SelectedTarget::for_native(target),
        );

        let mut checked_bodies = 0;

        for key in compilation
            .declared_unit_keys_for_test()
            .expect("fixture declarations must resolve their units")
        {
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
                .expect("fixture `semantic_unit_diagnostics_with_cancellation` call must succeed");

            assert!(
                !diagnostics.has_errors(),
                "{target:?} {key:?}: {diagnostics:?}"
            );

            let checked = compilation
                .lowered_unit(key)
                .expect("fixture callable must lower successfully");

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
#[ignore = "tier:proof"]
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
            .expect("fixture callable must lower successfully");

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
#[ignore = "tier:proof"]
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
        .expect("fixture callable must lower successfully");

    assert!(
        !checked.diagnostics().has_errors(),
        "{:?}",
        checked.diagnostics()
    );
}

#[test]
#[ignore = "tier:proof"]
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
        .expect("fixture callable must lower successfully");

    assert!(
        !checked.diagnostics().has_errors(),
        "{:?}",
        checked.diagnostics()
    );
}

#[test]
#[ignore = "tier:proof"]
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
            .expect("fixture callable must lower successfully");

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
#[ignore = "tier:proof"]
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
            .expect("fixture callable must lower successfully");

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
        .expect("fixture callable must lower successfully");

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
            .expect("fixture callable must lower successfully");

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
        .expect("fixture callable must lower successfully");

    bray_testing::assert_goal_state_diagnostic_kind(
        checked.diagnostics(),
        DiagnosticKind::CheckingTrustedObligationNotProven,
    );
}

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
        .expect("fixture callable must lower successfully");

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
            .expect("fixture callable must lower successfully");

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
        .expect("fixture callable must lower successfully");

    assert!(
        !checked.diagnostics().has_errors(),
        "{:?}",
        checked.diagnostics()
    );

    {
        let name = "result_marker";

        let checked = compilation
            .execution_properties(crate::test_support::source_type_callable_member_body_key(
                &compilation,
                name,
            ))
            .expect("fixture callable must resolve its execution properties");

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
            .expect("fixture callable must resolve its execution properties");

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
            .expect("fixture callable must resolve its execution properties");

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
            .expect("fixture callable must resolve its execution properties");

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
fn equal_const_results_retain_independent_owner_authority() {
    for (observed, valid) in [("first", true), ("second", false)] {
        let application = compilation(&format!(r#"
            trusted module app;

            struct Marker {{}}
            struct Owner
            {{
                marker: Marker;
            }}

            trusted predicate active(marker: &Marker);

            const func create() -> Owner executes(pure, total)
            {{
                return {{ marker = Marker {{}} }};
            }}

            trusted func activate(pos owner: &Owner)
                ensures(trusted active(&owner.marker))
                executes(pure, total)
            {{}}

            func observe(pos owner: &Owner)
                requires(trusted active(&owner.marker))
            {{}}

            func caller()
            {{
                let first = create();
                let second = create();
                trusted activate(&first);
                observe(&{observed});
            }}
        "#));
        let body = application.lowered_unit(source_function_body_key(&application, "caller"))
            .expect("const-owner fixture lowers");
        assert_eq!(!body.diagnostics().has_errors(), valid, "{observed}: {:?}", body.diagnostics());

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(body.diagnostics(), DiagnosticKind::CheckingTrustedObligationNotProven);
        }
    }
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
            .expect("fixture callable must lower successfully");

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
#[ignore = "tier:proof"]
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
        .expect("fixture callable must lower successfully");

    assert!(
        !checked.diagnostics().has_errors(),
        "{:?}",
        checked.diagnostics()
    );
}

#[test]
#[ignore = "tier:proof"]
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
        .expect("fixture callable must lower successfully");

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
        .expect("fixture callable must lower successfully");

    bray_testing::assert_goal_state_diagnostic_kind(
        checked.diagnostics(),
        DiagnosticKind::CheckingTrustedObligationNotProven,
    );
}

#[test]
#[ignore = "tier:proof"]
fn production_channel_declarations_check_their_own_bodies() {
    let application = crate::test_support::production_standard_library_compilation(
        crate::SelectedTarget::for_native(bray_target::NativeTarget::X86_64WindowsMsvc),
        "module app;",
    );

    let mut failures = Vec::new();

    for key in application
        .declared_unit_keys_for_test()
        .expect("fixture declarations must resolve their units")
    {
        if key.kind() != bray_bound_tree::BoundUnitKind::CallableBody {
            continue;
        }

        let source = application
            .source(key.source().syntax().source_id())
            .expect("fixture `source` call must succeed");

        if !source.text().starts_with("trusted module std.channel") {
            continue;
        }

        let checked = application
            .lowered_unit(key.clone())
            .expect("fixture callable must lower successfully");

        if checked.diagnostics().has_errors() {
            failures.push(format!("{key:?}: {:?}", checked.diagnostics()));
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
#[ignore = "tier:proof"]
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
            .expect("fixture callable must lower successfully");

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
            .expect("fixture callable must lower successfully");

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
fn callable_fields_do_not_expose_their_containing_owner() {
    for abi in ["", "@abi(c)"] {
        for (operation, valid) in [("touch(&owner);", true), ("mutate(&mut owner);", false)] {
            let application = compilation(&format!(
                r#"
                trusted module app;
                struct Marker {{}}
                trusted predicate live(marker: &Marker);
                callable Callback = {abi} func();
                struct Owner {{
                    marker: Marker;
                    callback: Callback;
                    trusted destruct() requires(trusted live(&self.marker)) {{}}
                }}
                {abi} func inert() {{}}
                trusted func make() -> Owner ensures(trusted live(&result.marker)) {{
                    return {{ marker = Marker {{}}, callback = inert }};
                }}
                extern func publish(pos callback: Callback);
                extern func mutate(pos owner: &mut Owner);
                func touch(pos owner: &Owner) {{ publish(owner.callback); }}
                func caller() {{ let mut owner = make(); {operation} }}
            "#
            ));

            let diagnostics = application.check_diagnostics();

            assert_eq!(
                !diagnostics.has_errors(),
                valid,
                "{abi} {operation}: {diagnostics:?}"
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
        .expect("fixture callable must lower successfully");

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
                diagnostics,
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
            .expect("fixture callable must lower successfully");

        assert!(
            !checked.diagnostics().has_errors(),
            "{operation}: {:?}",
            checked.diagnostics()
        );
    }
}

#[test]
#[ignore = "tier:proof"]
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
            .expect("fixture callable must lower successfully");

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
            .expect("fixture callable must resolve its execution properties");

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
            .expect("fixture callable must lower successfully");

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
        .expect("fixture callable must lower successfully");

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
#[ignore = "tier:proof"]
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

        assert!(!summary.owned_result().is_empty(), "{name}: {summary:?}");
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
            .expect("fixture callable must lower successfully");

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
            .expect("fixture callable must lower successfully");

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
#[ignore = "tier:proof"]
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
                .expect("fixture callable must lower successfully");

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

#[test]
fn independent_witness_survives_a_released_production_guard_in_a_loop() {
    let application = production_sync_compilation(
        r#"
        trusted module app;
        using core.memory;
        using std.sync;
        struct Marker {}
        struct Receipt { marker: Marker; pointer: DataPointer<u32>; }
        trusted predicate held(owner: &Marker);
        trusted func produce(pos payload: &u32) -> Receipt
            ensures(trusted held(owner = &result.marker))
        { return { marker = Marker {}, pointer = core.memory.address_of<u32>(payload) }; }
        func observe(pos receipt: Receipt) requires(trusted held(owner = &receipt.marker)) {}
        func production_probe() {
            let lock = std.sync.SpinLock<u32>(7);
            loop {
                let receipt = {
                    let mut guard = lock.lock();
                    let payload = guard.get();
                    yield produce(&payload);
                };
                observe(receipt);
            }
        }
        "#,
    );

    let checked = application
        .lowered_unit(source_function_body_key(&application, "production_probe"))
        .expect("guarded witness consumer lowers");

    assert!(
        !checked.diagnostics().has_errors(),
        "{:#?}",
        checked.diagnostics()
    );
}

#[test]
fn forwarding_a_union_owner_preserves_the_checked_payload_projection() {
    for (operation, valid) in [("", true), ("replace(&mut owner);", false)] {
        let source = format!(
            r#"
            trusted module app;
            using core.memory;

            struct Marker {{}}
            struct Owner {{ marker: Marker; pointer: DataPointer<u8>; }}
            union Backing {{ Empty; Owned(pos owner: Owner); }}
            trusted predicate live(owner: &Marker);
            const func backing_owner(pos backing: &Backing) -> &Owner
                requires(backing matches Backing.Owned(_))
                executes(pure, total)
            {{
                return match backing
                {{
                    case Backing.Owned(value) {{ yield &value; }}
                    case Backing.Empty {{ panic("empty backing has no owner"); }}
                }};
            }}
            predicate retained(owner: &Backing) = owner matches Backing.Owned(_) && live(&backing_owner(owner).marker);
            extern trusted func native_allocate() -> DataPointer<u8>
                ensures(trusted core.memory.owned_allocation(pointer = result, bytes = 1, align = 1));
            trusted func allocate(pos owner: &Backing) -> DataPointer<u8>
                requires(trusted retained(owner))
                ensures(trusted retained(owner), trusted core.memory.owned_allocation(pointer = result, bytes = 1, align = 1))
            {{
                return trusted native_allocate();
            }}
            extern func replace(pos owner: &mut Backing);
            func consume_owner(pos owner: Backing) requires(trusted retained(&owner)) {{}}
            trusted func caller(pos mut owner: Backing) requires(trusted retained(&owner)) uses(manual_alloc)
            {{
                let allocation = trusted allocate(&owner);
                {operation}
                consume_owner(owner);
                trusted core.memory.deallocate(allocation, bytes = 1, align = 1);
            }}
            "#,
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
fn owned_return_does_not_preserve_changed_boolean_contents() {
    for (mutation, valid) in [("", true), ("flags.ready = false;", false)] {
        let source = format!(
            r#"
            module app;
            struct Flags {{ mut ready: bool; }}
            func clear_flags(pos mut flags: Flags) -> Flags executes(total) {{
                {mutation}
                return flags;
            }}
            func require_old_ready() executes(total) {{
                let flags: Flags = {{ ready = true }};
                let changed_flags = clear_flags(flags);
                assert(changed_flags.ready);
            }}
            "#,
        );

        let application = compilation(&source);
        let diagnostics = application.check_diagnostics();

        assert_eq!(!diagnostics.has_errors(), valid, "{source}: {diagnostics:#?}");

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                DiagnosticKind::CheckingExecutionGuaranteeNotProven,
            );
        }
    }
}

#[test]
fn returned_owner_facts_follow_only_the_active_result_payload() {
    for (mutation, valid) in [("", true), ("if change { owner.active = false; }", false)] {
        let source = format!(
            r#"
            trusted module app;
            using core.memory;
            struct Owner {{ mut active: bool; pointer: DataPointer<u8>; }}
            trusted predicate held(owner: &Owner);
            extern trusted func opaque(pos owner: &Owner) -> DataPointer<u8>
                requires(trusted held(owner))
                ensures(trusted held(owner));
            func forward(pos mut owner: Owner, pos reject: bool, pos change: bool) -> Result<u8, Owner>
                requires(trusted held(&owner))
            {{
                _ = trusted opaque(&owner);

                if reject
                {{
                    {mutation}
                    return Error(owner);
                }}

                return Ok(0);
            }}
            func use_owner(pos owner: Owner) requires(trusted held(&owner)) {{}}
            func caller(pos owner: Owner, pos reject: bool, pos change: bool)
                requires(trusted held(&owner))
            {{
                match consume forward(owner, reject, change)
                {{
                    case Ok(_) {{}}
                    case Error(rejected) {{ use_owner(rejected); }}
                }}
            }}
            "#,
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
fn opaque_returned_owners_keep_their_carrier_inside_optional_result_payloads() {
    for (body, valid) in [
        ("release(owner);", true),
        (
            "let fake: Owner = { storage = core.memory.uninit<[u64; 16]>(), finish = finish }; release(fake);",
            false,
        ),
    ] {
        let source = format!(
            r#"
            trusted module app;
            using core.memory;
            callable Native = @abi(c) func();
            @abi(c) func finish() {{}}
            struct Owner {{ storage: Uninit<[u64; 16]>; finish: Native; }}
            trusted predicate held(owner: &Owner);
            extern func fill(pos storage: &mut Uninit<[u64; 16]>);
            trusted func mark(pos owner: &Owner) ensures(trusted held(owner)) executes(pure, total) {{}}
            const func present(pos owner: &(Owner?)) -> &Owner
                requires(owner matches ?_) executes(pure, total) {{
                return match owner {{ case ?value {{ yield &value; }} case none {{ panic("missing owner"); }} }};
            }}
            predicate optional_held(owner: &(Owner?)) = owner matches none || held(present(owner));
            const func successful(pos owner: &Result<(Owner?), u32>) -> &(Owner?)
                requires(owner matches Ok(_)) executes(pure, total) {{
                return match owner {{ case Ok(value) {{ yield &value; }} case Error(_) {{ panic("missing success"); }} }};
            }}
            trusted func create(pos mode: u8) -> Result<(Owner?), u32>
                ensures(!(result matches Ok(_)) || trusted optional_held(successful(&result))) {{
                if mode == 1 {{ return Ok(none); }}
                if mode == 2 {{ return Error(1); }}
                let native: Native = finish;
                let mut storage = core.memory.uninit<[u64; 16]>();
                fill(&mut storage);
                let owner: Owner = {{ storage = storage, finish = native }};
                mark(&owner);
                return Ok(owner);
            }}
            func release(pos owner: Owner) requires(trusted held(&owner)) {{}}
            func caller(pos mode: u8) {{
                match consume create(mode) {{
                    case Ok(optional) {{
                        match consume optional {{ case ?owner {{ {body} }} case none {{}} }}
                    }}
                    case Error(_) {{}}
                }}
            }}
        "#
        );

        let application = compilation(&source);

        assert!(
            !application.syntax_tree_result().diagnostics().has_errors(),
            "{source}"
        );

        let producer = application
            .lowered_unit(source_function_body_key(&application, "create"))
            .expect("the optional owner producer has a complete checked contract");

        assert!(
            !producer.diagnostics().has_errors(),
            "{source}: {producer:#?}"
        );

        let checked = application
            .lowered_unit(source_function_body_key(&application, "caller"))
            .expect("returned owners retain their complete producer context");

        assert_eq!(
            !checked.diagnostics().has_errors(),
            valid,
            "{source}: {checked:#?}"
        );
    }
}

#[test]
fn nullable_input_adaptation_preserves_physical_owners_and_rejected_values() {
    for (operation, valid) in [("", true), ("replace(&mut owner);", false)] {
        let source = format!(
            r#"
            trusted module app;
            using core.memory;

            struct Marker {{}}
            struct Owner {{ marker: Marker; pointer: DataPointer<u8>; }}
            struct Holder {{ value: Owner; }}
            trusted predicate live(owner: &Marker);
            trusted func make(pos pointer: DataPointer<u8>) -> Owner
                ensures(trusted live(&result.marker))
            {{
                return {{ marker = Marker {{}}, pointer = pointer }};
            }}
            const func present(pos owner: &(Owner?)) -> &Owner
                requires(owner matches ?_) executes(pure, total)
            {{
                return match owner {{
                    case ?value {{ yield &value; }}
                    case none {{ panic("missing owner"); }}
                }};
            }}
            predicate optional_live(owner: &(Owner?)) =
                owner matches none || live(&present(owner).marker);

            extern func exchange(pos pointer: DataPointer<u8>?) -> DataPointer<u8>;
            extern func replace(pos owner: (&mut Owner)?);
            extern func activity();
            func observe(pos owner: &Owner) requires(trusted live(&owner.marker)) {{}}
            func shared_view(pos owner: &mut Holder)
                requires(trusted live(&owner.value.marker))
                ensures(trusted live(&owner.value.marker))
            {{
                let selected = &owner.value;
                _ = exchange(selected.pointer);
                activity();
                observe(selected);
            }}
            func release(pos owner: Owner) requires(trusted live(&owner.marker)) {{}}
            func forward(pos owner: Owner?, pos reject: bool) -> Result<unit, (Owner?, u32)>
                requires(trusted optional_live(&owner))
            {{
                activity();
                if reject {{
                    let rejected: (Owner?, u32) = (owner, 4);
                    return Error(rejected);
                }}
                return Ok(unit);
            }}
            func caller_existing(pos mut owner: Owner, pos reject: bool)
                requires(trusted live(&owner.marker))
            {{
                _ = exchange(owner.pointer);
                {operation}
                match consume forward(owner, reject) {{
                    case Ok(_) {{}}
                    case Error((rejected, _)) {{
                        match consume rejected {{
                            case ?value {{ release(value); }}
                            case none {{}}
                        }}
                    }}
                }}
            }}
            func caller(pos pointer: DataPointer<u8>, pos reject: bool) {{
                caller_existing(make(pointer), reject);
            }}
            "#
        );

        let application = compilation(&source);

        assert!(!application.syntax_tree_result().diagnostics().has_errors(), "{source}");

        let diagnostics = application.check_diagnostics();

        assert_eq!(!diagnostics.has_errors(), valid, "{source}: {diagnostics:#?}");

        if !valid {
            assert!(diagnostics.iter().any(|diagnostic| matches!(diagnostic.kind(),
                bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven
                    | bray_diagnostics::DiagnosticKind::CheckingTrustedWitnessTransferNotProven
            )), "{diagnostics:#?}");
        }
    }
}

#[test]
fn raw_extraction_transfers_a_complete_nullable_witness() {
    for (extra_read, use_owner, valid) in [
        ("", "observe(&complete);", true),
        ("", "let fabricated: Owner = { epoch = complete.epoch }; observe(&fabricated);", false),
        ("_ = trusted core.memory.read<T>(pointer);", "observe(&complete);", false),
    ] {
        let source = format!(
            r#"
            trusted module app;
            using core.memory;
            struct Owner {{ epoch: u64; }}
            trusted predicate live(owner: &Owner);
            trusted func acquire() -> Owner ensures(trusted live(owner = &result)) {{
                return {{ epoch = 1 }};
            }}
            func observe(pos owner: &Owner) requires(trusted live(owner = owner)) {{}}
            trusted func exchange<T>(pos storage: &mut T, pos value: T) -> T
                uses(raw_memory, unchecked_init)
            {{
                let pointer = core.memory.address_of_mut<T>(storage);
                let previous = trusted core.memory.read<T>(pointer);
                {extra_read}
                trusted core.memory.write<T>(pointer, value);
                return previous;
            }}
            func caller(pos present: bool) {{
                let mut owner: Owner? = if present {{ yield acquire(); }} else {{ yield none; }};
                let moved = exchange<(Owner?)>(&mut owner, none);
                if let ?complete = moved {{ {use_owner} }}
            }}
            "#
        );

        let application = compilation(&source);
        let diagnostics = application.check_diagnostics();

        assert_eq!(!diagnostics.has_errors(), valid, "{source}: {diagnostics:#?}");
    }
}

#[test]
fn copied_pointer_components_keep_storage_facts_without_owning_the_tuple() {
    let application = compilation(
        r#"
        trusted module app;
        using core.memory;
        struct BorrowedHolder { items: &[u8; 1]; }
        func read_borrowed(pos value: BorrowedHolder) -> u8 { return value.items[0]; }
        func read_holder(pos items: &[u8; 1]) -> u8 {
            return read_borrowed(BorrowedHolder { items = items });
        }
        struct Marker {}
        struct Access { marker: Marker; context: usize; pointer: DataPointer<u8>; }
        trusted predicate held(owner: &Marker);
        trusted predicate native_callback(context: usize);
        func invoke(pos context: usize)
            requires(trusted native_callback(context)) executes(pure, total) {}
        trusted func permit(pos owner: &Access, pos pointer: DataPointer<u8>)
            requires(trusted held(owner = &owner.marker))
            ensures(
                trusted held(owner = &owner.marker),
                trusted native_callback(owner.context),
                trusted core.memory.valid_read<u8>(pointer = pointer, count = 1)
            ) executes(pure, total) {}
        func address(pos owner: &Access) -> DataPointer<u8>
            requires(trusted held(owner = &owner.marker))
            ensures(
                trusted held(owner = &owner.marker),
                trusted core.memory.valid_read<u8>(pointer = result, count = 1)
            ) executes(pure, total)
        {
            let pointer = owner.pointer;
            permit(owner, pointer);
            return pointer;
        }
        func copy_pointer(pos owner: &Access)
            requires(trusted held(owner = &owner.marker))
        {
            permit(owner, owner.pointer);
            let context = owner.context;
            invoke(context);
            observe(address(owner));
        }
        trusted func pair(pos left: DataPointer<u8>, pos right: DataPointer<u8>)
            -> (DataPointer<u8>, DataPointer<u8>)
            requires(
                trusted core.memory.valid_read<u8>(pointer = left, count = 1),
                trusted core.memory.valid_read<u8>(pointer = right, count = 1)
            )
            ensures(
                trusted core.memory.valid_read<u8>(pointer = result.0, count = 1),
                trusted core.memory.valid_read<u8>(pointer = result.1, count = 1)
            )
        { return (left, right); }
        func observe(pos pointer: DataPointer<u8>)
            requires(trusted core.memory.valid_read<u8>(pointer = pointer, count = 1))
            executes(pure, total) {}
        func caller(pos left: DataPointer<u8>, pos right: DataPointer<u8>)
            requires(
                trusted core.memory.valid_read<u8>(pointer = left, count = 1),
                trusted core.memory.valid_read<u8>(pointer = right, count = 1)
            )
        {
            let pointers = pair(left, right);
            observe(pointers.0);
            observe(pointers.1);
        }
        "#,
    );

    let diagnostics = application.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:#?}");
}

#[test]
fn returned_shared_views_do_not_freeze_mutable_referents() {
    for (input, subject) in [("Owner", "borrowed"), ("Holder", "&borrowed.current")] {
        let observed = subject.replace("borrowed", "result");

        let source = format!(
            r#"
            trusted module app;
            using core.atomic;
            struct Owner {{ value: core.atomic.Atomic<u32>; }}
            struct Holder {{ current: Owner; label: u32; }}
            trusted predicate observed(owner: &Owner);
            extern trusted func selected(pos owner: &{input}) -> &{input}
                ensures(trusted observed({observed})) executes(pure, total);
            extern func change();
            func observe(pos owner: &Owner)
                requires(trusted observed(owner)) executes(pure, total) {{}}
            func caller(pos owner: &{input}) {{
                let borrowed = selected(owner);
                change();
                observe({subject});
            }}
            "#
        );

        let application = compilation(&source);
        let diagnostics = application.check_diagnostics();

        bray_testing::assert_goal_state_diagnostic_kind(
            diagnostics,
            DiagnosticKind::CheckingTrustedObligationNotProven,
        );
    }
}

#[test]
fn consuming_transitions_introduce_child_owners_without_detaching_parent_authority() {
    for (result, field, guarantee, valid) in [
        ("Child", "child", "trusted child_owned(&result.marker)", true),
        ("Marker", "marker", "trusted parent_owned(&result)", false),
        ("Marker", "marker", "!flag || trusted parent_owned(&result)", false),
    ] {
        let source = format!(
            r#"
            trusted module app;
            using core.memory;
            struct Marker {{}}
            struct ChildMarker {{}}
            struct Child {{ marker: ChildMarker; value: u32; }}
            struct Parent {{ marker: Marker; mut child: Child; }}
            trusted predicate parent_owned(marker: &Marker);
            trusted predicate child_owned(marker: &ChildMarker);
            trusted func make() -> Parent
                ensures(trusted parent_owned(&result.marker)) executes(pure, total)
            {{ return {{ marker = Marker {{}}, child = {{ marker = ChildMarker {{}}, value = 1 }} }}; }}
            trusted func extract(pos owner: &mut Parent) -> Child
                requires(trusted parent_owned(&owner.marker))
                ensures(trusted parent_owned(&owner.marker), trusted child_owned(&result.marker))
                uses(raw_memory, unchecked_init)
            {{
                let pointer = core.memory.address_of_mut<Child>(&mut owner.child);
                let previous = trusted core.memory.read<Child>(pointer);
                trusted core.memory.write<Child>(pointer, {{ marker = ChildMarker {{}}, value = 0 }});
                return previous;
            }}
            func observe_parent(pos owner: &Parent)
                requires(trusted parent_owned(&owner.marker)) executes(pure, total) {{}}
            func local_transition() -> Child
                ensures(trusted child_owned(&result.marker))
            {{
                let mut owner = make();
                let child = extract(&mut owner);
                observe_parent(&owner);
                return child;
            }}
            trusted func take(pos owner: Parent, pos flag: bool) -> {result}
                requires(trusted parent_owned(&owner.marker))
                ensures({guarantee})
                uses(raw_memory)
            {{
                return trusted core.memory.read<{result}>(core.memory.address_of<{result}>(&owner.{field}));
            }}
            func caller(pos owner: Parent, pos flag: bool) -> {result}
                requires(trusted parent_owned(&owner.marker))
            {{ return take(owner, flag); }}
            "#
        );

        let application = compilation(&source);
        let diagnostics = application.check_diagnostics();

        assert_eq!(!diagnostics.has_errors(), valid, "{source}: {diagnostics:#?}");

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                DiagnosticKind::CheckingTrustedWitnessTransferNotProven,
            );
        }
    }
}


const COPIED_SCALAR_SOURCE: &str = r#"
    trusted module app;

    struct Value
    {
        tag: bool;
    }

    trusted predicate live(value: &bool);

    func copied(pos source: &Value) -> Value
        ensures(result.tag == source.tag)
        executes(pure, total)
    {
        return
        {
            tag = source.tag
        };
    }

    func observe(pos value: &bool)
        requires(trusted live(value))
    {
    }
"#;

#[test]
fn copied_scalar_result_preserves_ordinary_value() {
    let compilation = compilation(COPIED_SCALAR_SOURCE);
    assert!(!compilation.check_diagnostics().has_errors(), "{:?}", compilation.check_diagnostics());
}

#[test]
fn copied_scalar_result_does_not_copy_borrowed_authority() {
    let compilation = compilation(&format!("{COPIED_SCALAR_SOURCE} {body}", body = r#"
        func caller(pos source: &Value)
            requires(trusted live(&source.tag))
        {
            let result = copied(source);
            observe(&result.tag);
        }
    "#));
    bray_testing::assert_goal_state_diagnostic_kind(compilation.check_diagnostics(),
        DiagnosticKind::CheckingTrustedObligationNotProven);
}

const INDEX_COPY_SOURCE: &str = r#"
    trusted module app;

    @copy
    struct Item
    {
        value: i32;
    }

    struct Values
    {
        value: Item;
    }

    trusted predicate live(value: &Item);

    impl Values(ElementIndex<i32>)
    {
        type Output = Item;

        func index(pos selector: &i32) -> &Item
        {
            return &self.value;
        }
    }

    func copy(pos values: &Values) -> Item
    {
        return values[0];
    }
"#;

#[test]
fn custom_index_copy_waits_for_the_returned_borrow() {
    let compilation = compilation(INDEX_COPY_SOURCE);
    let diagnostics = compilation.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:#?}");

    let lowered = compilation.lowered_unit(source_function_body_key(&compilation, "copy"))
        .expect("copy through a returned index borrow lowers");
    assert!(!lowered.diagnostics().has_errors(), "{:?}", lowered.diagnostics());
}

#[test]
fn custom_index_copy_rejects_witness_duplication() {
    for selected in ["values.value", "values[0]"] {
        let source = format!(r#"{INDEX_COPY_SOURCE}

            trusted func acquire() -> Item
                ensures(trusted live(&result))
            {{
                return
                {{
                    value = 7
                }};
            }}

            func duplicate()
            {{
                let values: Values =
                {{
                    value = acquire()
                }};

                let copied = {selected};
            }}
        "#);
        let compilation = compilation(&source);
        bray_testing::assert_goal_state_diagnostic_kind(compilation.check_diagnostics(),
            DiagnosticKind::CheckingTrustedWitnessTransferNotProven);
    }
}

#[test]
fn direct_value_copy_cannot_rebind_borrowed_field_authority() {
    let compilation = compilation(r#"
        trusted module app;

        struct Owner
        {
            tag: bool;
        }

        trusted predicate live(value: &bool);

        func observe_copy(pos value: bool)
            requires(trusted live(&value))
        {
        }

        func caller(pos owner: &Owner)
            requires(trusted live(&owner.tag))
        {
            observe_copy(owner.tag);
        }
    "#);

    bray_testing::assert_goal_state_diagnostic_kind(compilation.check_diagnostics(),
        DiagnosticKind::CheckingTrustedObligationNotProven);
}
