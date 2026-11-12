use bray_diagnostics::{DiagnosticKind};
use crate::test_support::{source_function_body_key};
use super::fixtures::{body_belongs_to, production_host_compilation, production_sync_compilation, production_sync_compilation_for_target};

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
fn production_raw_synchronization_pointer_does_not_grant_safe_atomic_access() {
    use bray_binder::SymbolQueryProvider;
    use bray_symbols::{CallableContractsQuery, CallableSymbolId, SymbolQueryRequest};

    for memory in ["core.memory", "std.memory"] {
        let source = r#"
        module app;

        using core.atomic;
        using MEMORY;
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

        let key = source_function_body_key(&compilation, "production_probe");

        let source_id = key.source().syntax().source_id();

        let callable = compilation
            .symbol_graph()
            .expect("the fixture retains its symbols")
            .symbol_for_key(key.declared_owner())
            .and_then(CallableSymbolId::try_from_any)
            .expect("the probe body has its declared callable");

        let checked = compilation
            .binding_context(&compilation.state.cancellation)
            .expect("the fixture retains its binding context")
            .resolve_symbol_query(SymbolQueryRequest::<CallableContractsQuery>::new(callable))
            .expect("the production caller retains its capability contract");

        let diagnostics = checked
            .diagnostics()
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
