use bray_diagnostics::{DiagnosticKind};
use crate::compilation::result_dependencies::{callable_dependencies};
use crate::test_support::{source_function_body_key};
use super::fixtures::{body_belongs_to, production_sync_compilation};

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
            struct Payload
            {{
                mut value: u32;
            }}
            func production_probe()
            {{
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

#[test]
#[ignore = "tier:proof"]
fn production_spin_lock_nullable_acquisition_retains_storage() {
    let compilation = production_sync_compilation(
        r#"
        module app;
        using std.sync;
        func production_probe()
        {
            let lock = std.sync.SpinLock<u32>(7);
            let candidate = lock.try_lock();
            match consume candidate
            {
                case ?guard
                {
                    let mut held = guard;
                    let value = held.get();
                }
                case none
                {
                }
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
                func production_probe()
                    requires(blocking_execution())
                {{
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
        func production_probe()
        {
            let first = std.sync.SpinLock<u32>(0);
            let second = std.sync.SpinLock<u32>(0);
            let mut guard = first.lock();
            {
                let other = second.lock();
            }
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
        func production_probe()
            requires(blocking_execution())
        {
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
            func production_probe()
            {{
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
            struct Pending<T>
            {{
                mut value: T?;
            }}
            struct Payload<T>
            {{
                mut inner: Inner;
                mut value: T?;
            }}
            struct Inner
            {{
                mut value: u32;
                mut func change()
                {{
                    opaque();
                    self.value += 1;
                }}
            }}
            extern trusted func opaque();
            extern trusted func touch<T>(pos value: &mut T);
            extern trusted func stop() -> bool;
            extern trusted func take<T>(pos pending: &mut Pending<T>) -> T;
            extern trusted async func suspend();
            async func production_probe<T>(pos lock: &std.sync.SpinLock<Payload<T>>, pos mut pending: Pending<T>)
                requires(trusted std.sync.spin_lock.valid(&lock.prepared.instance))
            {{
                while true
                {{
                    let generation: u32 =
                    {{
                        let mut guard = lock.lock();
                        let payload: &mut Payload<T> = guard.get();
                        if stop() && stop()
                        {{
                            return unit;
                        }}
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
fn independent_witness_survives_a_released_production_guard_in_a_loop() {
    let application = production_sync_compilation(
        r#"
        trusted module app;
        using core.memory;
        using std.sync;
        struct Marker
        {
        }
        struct Receipt
        {
            marker: Marker;
            pointer: DataPointer<u32>;
        }
        trusted predicate held(owner: &Marker);
        trusted func produce(pos payload: &u32) -> Receipt
            ensures(trusted held(owner = &result.marker))
        {
            return
            {
                marker = Marker {}, pointer = core.memory.address_of<u32>(payload)
            };
        }
        func observe(pos receipt: Receipt)
            requires(trusted held(owner = &receipt.marker))
        {
        }
        func production_probe()
        {
            let lock = std.sync.SpinLock<u32>(7);
            loop
            {
                let receipt =
                {
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
