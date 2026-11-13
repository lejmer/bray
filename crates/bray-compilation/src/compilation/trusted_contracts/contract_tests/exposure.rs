use bray_diagnostics::{DiagnosticKind};
use crate::compilation::result_dependencies::{callable_dependencies};
use crate::test_support::{compilation, source_function_body_key};
use super::fixtures::{production_sync_compilation};

#[test]
fn rewritten_raw_pointer_fields_publish_opaque_output_identities() {
    let application = production_sync_compilation(
        r#"
        trusted module app;
        using core.memory;
        using core.target;
        struct Cursor
        {
            mut pointer: DataPointer<u8>;
        }
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
fn opaque_carrier_predicates_are_revoked_by_referent_mutation() {
    for transport in [false, true] {
    for (mutation, valid) in [("", true), ("carrier.payload.value = 1;", false)] {
        let acquire = if transport { "forward(forward(make(&mut payload)))" } else { "make(&mut payload)" };
        let source = format!(
            r#"
            trusted module app;
            struct Payload
            {{
                mut value: u8;
            }}
            struct Carrier
            {{
                payload: &mut Payload;
            }}
            func forward<T>(pos value: T) -> T {{ return value; }}
            trusted predicate live(carrier: &Carrier);
            trusted func make(pos payload: &mut Payload) -> Carrier
                ensures(trusted live(&result)) executes(pure, total)
            {{
                return
                {{
                    payload = payload
                }};
            }}
            func observe(pos carrier: &Carrier)
                requires(trusted live(carrier)) executes(pure, total) {{}}
            func caller()
            {{
                let mut payload: Payload = {{ value = 0 }};
                let mut carrier = {acquire};
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
            struct Marker
            {{
            }}
            struct Payload
            {{
                mut value: bool;
            }}
            struct Guard
            {{
                marker: Marker;
                payload: &mut Payload;
            }}
            trusted predicate held(value: &{ty});
            trusted func acquire(pos payload: &mut Payload) -> Guard
                ensures(trusted held({observation})) executes(pure, total)
            {{
                return
                {{
                    marker = Marker {{}}, payload = payload
                }};
            }}
            func observe(pos guard: &Guard)
                requires(trusted held({argument})) executes(pure, total) {{}}
            func caller(pos callback: func() -> unit)
            {{
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
        struct Marker
        {
        }
        struct Payload
        {
            mut value: bool;
        }
        struct Guard
        {
            marker: Marker;
            payload: &mut Payload;
        }
        trusted predicate held(value: &Marker);
        trusted predicate recorded(value: bool);
        trusted func acquire(pos payload: &mut Payload) -> Guard
            ensures(trusted held(&result.marker), trusted recorded(result.payload.value))
            executes(pure, total)
        {
            return
            {
                marker = Marker {}, payload = payload
            };
        }
        func observe(pos value: bool)
            requires(trusted recorded(value))
            executes(pure, total)
        {
        }
        func caller(pos payload: &mut Payload, pos callback: func() -> unit)
        {
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
            struct Marker
            {{
            }}

            struct Owner
            {{
                instance: Marker;
            }}

            trusted predicate live(marker: &Marker);

            trusted func make() -> Owner
                ensures(trusted live(&result.instance))
            {{
                return
                {{
                    instance = Marker {{}}
                }};
            }}

            func borrow(pos owner: &Owner) -> &Owner
            {{
                return owner;
            }}

            func observe(pos owner: &Owner)
                requires(trusted live(&owner.instance)) {{}}

            extern trusted func opaque();
            extern trusted func replace(pos owner: &mut &Owner);
            extern trusted func replace_owner(pos owner: &mut Owner);
            extern trusted func publish(pos pointer: DataPointer<Owner>);
            struct Holder
            {{
                owner: &mut Owner;
            }}

            extern trusted func replace_holder(pos holder: Holder);

            trusted func caller()
                uses(raw_memory)
            {{
                {body}
            }}
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
            func signal()
            {{
                core.atomic.store<u32, 2>(&WORD, 1);
            }}
            extern trusted func replace(pos value: &mut u32);
            extern trusted func publish(pos pointer: DataPointer<u32>);
            trusted func caller(pos initial: u32) -> u32
                ensures(result == initial)
                uses(raw_memory)
            {{
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
            struct Marker
            {{
            }}
            struct Owner
            {{
                marker: Marker;
            }}
            trusted predicate live(marker: &Marker);
            trusted func make() -> Owner
                ensures(trusted live(&result.marker))
            {{
                return
                {{
                    marker = Marker {{}}
                }};
            }}
            extern trusted func choose(pos first: &mut Owner, pos second: &mut Owner) -> &mut Owner;
            extern trusted func replace(pos owner: &mut Owner);
            func observe(pos owner: &Owner)
                requires(trusted live(&owner.marker))
            {{
            }}
            func caller()
            {{
                let mut first = make();
                let mut second = make();
                let third = make();
                {{
                    let selected = choose(&mut first, &mut second);
                    replace(selected);
                }}
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
            @abi(c) func finish()
            {{
            }}
            struct Owner
            {{
                storage: Uninit<[u64; 16]>;
                finish: Native;
            }}
            trusted predicate held(owner: &Owner);
            extern func fill(pos storage: &mut Uninit<[u64; 16]>);
            trusted func mark(pos owner: &Owner)
                ensures(trusted held(owner))
                executes(pure, total)
            {{
            }}
            const func present(pos owner: &(Owner?)) -> &Owner
                requires(owner matches ?_) executes(pure, total)
            {{
                return match owner
                {{
                    case ?value
                    {{
                        yield &value;
                    }}
                    case none
                    {{
                        panic("missing owner");
                    }}
                }};
            }}
            predicate optional_held(owner: &(Owner?)) = owner matches none || held(present(owner));
            const func successful(pos owner: &Result<(Owner?), u32>) -> &(Owner?)
                requires(owner matches Ok(_)) executes(pure, total)
            {{
                return match owner
                {{
                    case Ok(value) {{ yield &value; }}
                    case Error(_) {{ panic("missing success"); }}
                }};
            }}
            trusted func create(pos mode: u8) -> Result<(Owner?), u32>
                ensures(!(result matches Ok(_)) || trusted optional_held(successful(&result)))
            {{
                if mode == 1
                {{
                    return Ok(none);
                }}
                if mode == 2
                {{
                    return Error(1);
                }}
                let native: Native = finish;
                let mut storage = core.memory.uninit<[u64; 16]>();
                fill(&mut storage);
                let owner: Owner = {{ storage = storage, finish = native }};
                mark(&owner);
                return Ok(owner);
            }}
            func release(pos owner: Owner)
                requires(trusted held(&owner))
            {{
            }}
            func caller(pos mode: u8)
            {{
                match consume create(mode)
                {{
                    case Ok(optional)
                    {{
                        match consume optional
                        {{
                            case ?owner {{ {body} }}
                            case none {{}}
                        }}
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
fn returned_shared_views_do_not_freeze_mutable_referents() {
    for (input, subject) in [("Owner", "borrowed"), ("Holder", "&borrowed.current")] {
        let observed = subject.replace("borrowed", "result");

        let source = format!(
            r#"
            trusted module app;
            using core.atomic;
            struct Owner
            {{
                value: core.atomic.Atomic<u32>;
            }}
            struct Holder
            {{
                current: Owner;
                label: u32;
            }}
            trusted predicate observed(owner: &Owner);
            extern trusted func selected(pos owner: &{input}) -> &{input}
                ensures(trusted observed({observed})) executes(pure, total);
            extern func change();
            func observe(pos owner: &Owner)
                requires(trusted observed(owner)) executes(pure, total) {{}}
            func caller(pos owner: &{input})
            {{
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
fn opaque_calls_preserve_whole_unexposed_values_only_without_indirect_storage() {
    for (field, exposure, parameter, argument, valid) in [
        ("u64", "", "", "", true),
        ("(u64, bool)", "", "", "", true),
        ("u64", "", "pos owner: &mut Owner", "&mut owner", false),
        (
            "u64",
            "let escaped = core.memory.address_of_mut<Owner>(&mut owner); publish(escaped);",
            "",
            "",
            false,
        ),
        ("DataPointer<u8>", "", "", "", false),
        ("&u64", "", "", "", false),
        ("core.memory.Uninit<u64>", "", "", "", false),
        ("core.atomic.Atomic<u64>", "", "", "", false),
    ] {
        let source = format!(
            r#"
            trusted module app;

            using core.atomic;

            using core.memory;
            struct Owner
            {{
                mut value: {field};
            }}
            trusted predicate acquired(owner: &Owner);
            trusted extern func acquire() -> Owner
                ensures(trusted acquired(&result));
            extern func opaque({parameter});
            extern func publish(pos pointer: DataPointer<Owner>);
            func inspect(pos owner: &Owner)
                requires(trusted acquired(owner))
                executes(pure, total)
            {{
            }}
            trusted func run()
            {{
                let mut owner = trusted acquire();
                {exposure}
                opaque({argument});
                inspect(&owner);
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

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}
