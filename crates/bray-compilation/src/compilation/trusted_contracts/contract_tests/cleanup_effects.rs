use bray_diagnostics::{DiagnosticKind};
use crate::test_support::{compilation, source_function_body_key};
use super::fixtures::{production_sync_compilation};

#[test]
fn cleanup_keeps_completion_guarantees_for_represented_children() {
    for (guarantee, valid) in [("ensures(trusted live(&self.child))", true), ("", false)] {
        let source = format!(
            r#"
            trusted module app;
            trusted predicate live(value: &Child);
            struct Child
            {{
                value: u32;
                trusted destruct() requires(trusted live(&self))
                {{
                }}
            }}
            struct Parent
            {{
                child: Child;
                trusted finalize() {guarantee} executes(total) {{}}
            }}
            func caller()
            {{
                let value: Parent =
                {{
                    child =
                    {{
                        value = 1
                    }}
                }};
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
                struct Owner
                {{
                    mut epoch: u64;
                    trusted destruct() requires(trusted live(&self))
                    {{
                    }}
                }}
                struct Writer
                {{
                    slot: &mut Owner;
                    destruct()
                    {{
                        {effect}
                    }}
                }}
                trusted func make() -> Owner
                    ensures(trusted live(&result))
                {{
                    return
                    {{
                        epoch = 1
                    }};
                }}
                func caller(pos flag: bool)
                {{
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
fn cleanup_reference_outputs_feed_the_next_lifecycle() {
    for (observed, valid) in [("first", false), ("second", true)] {
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
                mut slot: &Owner;
                other: &Owner;
                finalize() executes(total) {{ self.slot = self.other; }}
                trusted destruct() ensures(trusted live(&self.slot))
                {{
                }}
            }}
            func observe(pos owner: &Owner)
                requires(trusted live(owner))
            {{
            }}
            func caller()
            {{
                let mut first: Owner = {{ epoch = 0 }};
                let mut second: Owner = {{ epoch = 0 }};
                {{
                    let writer: Writer =
                    {{
                        slot = &first, other = &second
                    }};
                }}
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

            struct Writer
            {{
                slot: &mut Uninit<u32>;

                trusted destruct()
                    ensures(trusted core.memory.uninit_initialized<u32>(storage = &self.slot))
                {{
                    core.memory.uninit_write<u32>(self.slot, 42);
                }}
            }}

            trusted func caller(pos flag: bool) -> u32
                uses(unchecked_init)
            {{
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
            struct Producer
            {{
                tag: u8;
                trusted destruct() ensures(trusted ready()) {total}
                {{
                }}
            }}
            struct Interference
            {{
                tag: u8;
                destruct() executes(total)
                {{
                }}
            }}
            func observe()
                requires(trusted ready())
                ensures(trusted ready())
                executes(pure, total)
            {{
            }}
            func caller(pos branch: bool)
            {{
                {body}
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
            struct Producer
            {{
                payload: {payload};
                trusted destruct() ensures(trusted ready()) {{}}
            }}
            func observe()
                requires(trusted ready())
                executes(pure, total)
            {{
            }}
            func caller(pos value: {owner})
            {{
                {{
                    let released = value;
                }} observe();
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
            struct Producer
            {{
                tag: u8;
                trusted destruct() ensures(trusted ready()) executes(total)
                {{
                }}
            }}
            func observe()
                requires(trusted ready())
                executes(pure, total)
            {{
            }}
            func caller(pos flag: bool)
            {{
                let mut retained: Producer? = none;
                {body}
            }}
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
fn consuming_cleanup_retains_only_proven_external_borrow_identity() {
    for (cleanup, valid) in [("", true), ("self.reference = self.other;", false)] {
        let source = format!(
            r#"
            trusted module app;
            struct Payload
            {{
                value: bool;
            }}
            trusted predicate live(value: &Payload);
            trusted predicate acquired(value: &Guard);
            struct Guard
            {{
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
            struct Writer
            {{
                value: &core.atomic.Atomic<u32>;
                destruct()
                {{
                    core.atomic.store<u32, 0>(self.value, 1);
                }}
            }}
            func dispose<T>(pos value: T)
            {{
            }}
            func forward<T>(pos value: T)
            {{
                dispose<T>(value);
            }}
            func production_probe()
            {{
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
        func dispose<T>(pos value: T, pos stop: bool)
        {
            if stop
            {
                return;
            }
            dispose<(T,)>((value,), stop);
        }
        func production_probe()
        {
            dispose<u32>(7, true);
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
fn unexposed_owned_marker_survives_opaque_callback_cleanup() {
    let source = r#"
        trusted module app;
        struct Marker
        {
        }
        trusted predicate held(marker: &Marker);
        struct Attempt
        {
            marker: Marker;
            trusted destruct() requires(trusted held(&self.marker)) executes(total)
            {
            }
        }
        trusted func acquire() -> Attempt
            ensures(trusted held(&result.marker))
        {
            return
            {
                marker = Marker {}
            };
        }
        func caller(pos callback: func() -> unit)
        {
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
fn nested_owner_cleanup_preserves_its_required_authority() {
    let application = compilation(
        r#"
        trusted module app;
        extern trusted func opaque();
        struct Marker
        {
        }
        trusted predicate live(marker: &Marker);
        struct Owner
        {
            instance: Marker;
            trusted destruct() requires(trusted live(&self.instance))
            {
                opaque();
            }
        }
        struct Outer
        {
            owner: Owner;
            trusted destruct() requires(trusted live(&self.owner.instance))
                ensures(trusted live(&self.owner.instance)) {}
            consume func close()
                requires(trusted live(&self.owner.instance))
            {
            }
        }
        trusted func make() -> Outer
            ensures(trusted live(&result.owner.instance))
        {
            return
            {
                owner =
                {
                    instance = Marker {}
                }
            };
        }
        func caller()
        {
            let value = make();
            value.close();
        }
    "#,
    );

    let diagnostics = application.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:?}");
}

#[test]
fn borrowed_accessor_does_not_prepare_its_referents_cleanup() {
    let application = compilation(
        r#"
        trusted module app;
        struct Marker
        {
        }
        trusted predicate live(marker: &Marker);
        struct Owner
        {
            instance: Marker;
            const func marker() -> &Marker
                executes(pure, total)
            {
                return &self.instance;
            }
            trusted destruct() requires(trusted live(self.marker()))
            {
            }
        }
        trusted func make() -> Owner
            ensures(trusted live(result.marker()))
        {
            return
            {
                instance = Marker {}
            };
        }
        func caller()
        {
            let owner = make();
        }
    "#,
    );

    let diagnostics = application.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:?}");
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
            struct Marker
            {{
            }}
            trusted predicate live(marker: &Marker);
            struct Owner
            {{
                marker: Marker;
                trusted destruct() requires(trusted live(&self.marker))
                {{
                }}
            }}
            trusted func make() -> Owner
                ensures(trusted live(&result.marker))
            {{
                return
                {{
                    marker = Marker {{}}
                }};
            }}
            extern trusted async func work();
            async func caller()
            {{
                let owner = make();
                {{
                    {cleanup}
                }}
            }}
        "#
        ));

        let diagnostics = application.check_diagnostics();

        assert!(!diagnostics.has_errors(), "{cleanup}: {diagnostics:?}");
    }
}

#[test]
fn conditional_cleanup_requires_authority_for_the_selected_payload() {
    for cleanup in ["", "_ = core.atomic.fetch_sub<usize, 0>(&self.count, 1);"] {
    for (construction, valid) in [
        ("make();", true),
        ("let owner = make();", true),
        ("let _: Owner? = make();", true),
        ("let _: Owner? = Owner { marker = Marker {} };", false),
        ("let owner: Owner? = Owner { marker = Marker {} };", false),
        ("let owner = pair();", true),
        ("let mut owner = pair(); owner.operation = none;", true),
        ("let mut owner = pair(); owner.operation = none; owner.code = 0; update(&mut owner);", true),
    ] {
        let application = compilation(&format!(
            r#"
            trusted module app;
            using core.atomic;

            struct Marker
            {{
            }}

            trusted predicate live(marker: &Marker);

            struct Owner
            {{
                marker: Marker;
                count: core.atomic.Atomic<usize> = core.atomic.initialize<usize>(0);

                trusted destruct()
                    requires(trusted live(&self.marker))
                    executes(total)
                {{
                    {cleanup}
                }}
            }}

            struct Pair
            {{
                current: Owner;
                mut operation: Owner?;
                mut code: u32;
            }}
            extern func opaque();
            func update(pos owner: &mut Pair)
                requires(owner.code == 0)
            {{
                opaque();
                owner.code = 1;
            }}
            const func marker(pos value: &(Owner?)) -> &Marker
                requires(!(value matches none)) executes(pure, total)
            {{
                match value
                {{
                    case ?owner
                    {{
                        return &owner.marker;
                    }}
                    case none
                    {{
                        panic("absent owner");
                    }}
                }}
            }}
            extern trusted func make() -> Owner?
                ensures(result matches none || trusted live(marker(&result)));

            extern trusted func pair() -> Pair
                ensures(trusted live(&result.current.marker), result.operation matches none || trusted live(marker(&result.operation)));

            func caller()
            {{
                {construction}
            }}
        "#
        ));

        let diagnostics = application.check_diagnostics();

        if valid {
            assert!(!diagnostics.has_errors(), "{construction}, cleanup={cleanup}: {diagnostics:?}");

            let candidates = application.execution_candidates_with_cancellation(
                source_function_body_key(&application, "caller"), &crate::CancellationToken::new())
                .expect("conditional cleanup candidates publish");

            assert!(!candidates.result().diagnostics().has_errors(), "{construction}, cleanup={cleanup}: {candidates:#?}");
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
    }
}
