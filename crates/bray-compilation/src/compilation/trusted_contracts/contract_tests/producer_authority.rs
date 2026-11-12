use bray_diagnostics::{DiagnosticKind};
use crate::test_support::{compilation};

#[test]
fn bootstrap_host_access_requires_owner_bound_acquisition_evidence() {
    for (body, valid) in [
        ("observe(&guard);", true),
        ("let moved = guard; observe(&moved);", true),
        (
            "let fake: HostAccess = { owner = guard.owner, lock = guard.lock, active = guard.active }; observe(&fake);",
            false,
        ),
        (
            "let fake: HostAccess = { owner = &host, lock = &lock, active = true }; observe(&fake);",
            false,
        ),
        ("guard.owner = &other; observe(&guard);", false),
        ("guard.lock = &other_lock; observe(&guard);", false),
        ("guard.active = true; observe(&guard);", false),
    ] {
        let source = format!(
            r#"
            trusted module app;

            using core.atomic;
            struct HostOwner
            {{
                mut value: u64;
            }}
            trusted predicate acquired(owner: &HostAccess);
            struct HostAccess
            {{
                owner: &HostOwner;
                lock: &core.atomic.Atomic<u32>;
                active: bool;
                trusted destruct() requires(trusted acquired(&self)) executes(total)
                {{
                    if self.active
                    {{
                        let decrement: u32 = ~0;
                        _ = core.atomic.fetch_add<u32, 2>(self.lock, decrement);
                    }}
                }}
            }}
            trusted func acquire(pos host: &HostOwner, pos lock: &core.atomic.Atomic<u32>) -> HostAccess
                ensures(trusted acquired(&result))
            {{
                let (_, selected) = core.atomic.compare_exchange<u32, 3, 1>(lock, expected = 0, desired = 1);
                return
                {{
                    owner = host, lock = lock, active = selected
                }};
            }}
            func observe(pos value: &HostAccess)
                requires(trusted acquired(value)) ensures(trusted acquired(value)) executes(pure, total) {{}}
            func caller()
            {{
                let host: HostOwner = {{ value = 0 }};
                let other: HostOwner = {{ value = 0 }};
                let lock = core.atomic.initialize<u32>(0);
                let other_lock = core.atomic.initialize<u32>(0);
                let mut guard = acquire(&host, &lock);
                {body}
            }}
            "#,
        );

        let compilation = compilation(&source);

        assert_eq!(
            !compilation.check_diagnostics().has_errors(),
            valid,
            "{body}: {:#?}",
            compilation.check_diagnostics(),
        );
    }
}

#[test]
fn input_guarantees_do_not_make_unrelated_results_authority_witnesses() {
    for preservation in [
        "trusted valid(owner = owner), trusted core.memory.valid_read<u8>(pointer = result, count = 1)",
        "trusted valid(owner = owner) && trusted core.memory.valid_read<u8>(pointer = result, count = 1)",
    ] {
        let source = r#"
        trusted module app;

        using core.memory;
        struct Owner
        {
            value: u8;
        }
        trusted predicate valid(owner: &Owner);

        extern trusted func address(pos owner: &Owner) -> DataPointer<u8>
            requires(trusted valid(owner = owner))
            ensures(
                PRESERVATION,
                trusted core.memory.aligned_for<u8>(pointer = result),
                trusted core.memory.initialized_as<u8>(pointer = result),
            );

        trusted func caller(pos owner: &Owner) -> u8
            requires(trusted valid(owner = owner))
            uses(raw_memory)
        {
            let pointer: DataPointer<u8> = trusted address(owner);
            let copied: DataPointer<u8> = pointer;

            return trusted core.memory.read<u8>(copied);
        }
    "#
        .replace("PRESERVATION", preservation);

        let compilation = compilation(&source);

        assert!(
            !compilation.check_diagnostics().has_errors(),
            "{:?}",
            compilation.check_diagnostics()
        );
    }
}

#[test]
fn whole_owner_guarantees_retain_the_result_dependencies() {
    for (arguments, valid) in [("&local, &outer", true), ("&outer, &local", false)] {
        let compilation = compilation(&format!(
            r#"
            trusted module app;
            struct Value
            {{
                byte: u8;
            }}
            struct View
            {{
                value: &Value;
            }}
            trusted predicate live(owner: &View);
            trusted func make_view(pos observed: &Value, pos retained: &Value) -> View
                ensures(trusted live(owner = &result)) executes(pure, total)
            {{
                return
                {{
                    value = retained
                }};
            }}
            func observe(pos owner: &View) -> u8
                requires(trusted live(owner = owner)) executes(pure, total)
            {{
                return owner.value.byte;
            }}
            func caller() -> u8
            {{
                let outer: Value = {{ byte = 1 }};
                let returned: View =
                {{
                    let local: Value = {{ byte = 2 }};
                    yield trusted make_view({arguments});
                }};
                return observe(&returned);
            }}
        "#
        ));

        assert!(
            !compilation.syntax_tree_result().diagnostics().has_errors(),
            "{:?}",
            compilation.syntax_tree_result().diagnostics()
        );

        let checked = compilation
            .lowered_unit(crate::test_support::source_function_body_key(
                &compilation,
                "caller",
            ))
            .expect("fixture callable must lower successfully");

        assert_eq!(
            !checked.diagnostics().has_errors(),
            valid,
            "{arguments}: {checked:#?}"
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                checked.diagnostics(),
                DiagnosticKind::CheckingEscapingStorageDependency,
            );
        }
    }
}

#[test]
fn changing_a_guard_does_not_erase_a_safe_producers_entry_obligation() {
    let compilation = compilation(
        r#"
        trusted module app;
        trusted predicate live(value: u64);
        func make(pos mut flag: bool) -> u64
            when(flag)
            {
                ensures(trusted live(result))
            }
            {
                flag = false;
                return 0;
            }
    "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(),
        DiagnosticKind::CheckingTrustedObligationNotProven,
    );
}

#[test]
fn a_vacuous_entry_obligation_does_not_make_the_owner_a_witness() {
    let compilation = compilation(
        r#"
        trusted module app;
        @copy struct Owner
        {
            epoch: u64;
        }
        trusted predicate live(value: &Owner);
        func caller(pos value: Owner, flag: bool)
            requires(flag || trusted live(&value))
            {
            if flag { let copied = value; }
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
fn trusted_producers_establish_live_result_witnesses() {
    let source = r#"
        trusted module app;

        struct Owner
        {
            mut epoch: u64;
        }

        trusted predicate live(owner: &Owner);

        trusted func owner() -> Owner
            ensures(trusted live(&result))
        {
            return
            {
                epoch = 1
            };
        }

        trusted func observe(pos value: &Owner)
            requires(trusted live(value)) {}

        func caller()
        {
            let value = owner();
            observe(&value);
        }
    "#;

    let compilation = compilation(source);

    assert!(
        !compilation.check_diagnostics().has_errors(),
        "{:?}",
        compilation.check_diagnostics()
    );
}

#[test]
fn moving_raw_contents_preserves_allocation_ownership_until_deallocation() {
    for (completion, valid) in [
        ("return value;", true),
        (
            "trusted core.memory.deallocate(address, bytes = 1, align = 1); return value;",
            false,
        ),
    ] {
        let source = format!(
            r#"
            trusted module app;

            using core.memory;
            struct Payload
            {{
                value: u8;
            }}
            trusted func take(pos pointer: DataPointer<Payload>) -> Payload
                requires(
                    trusted core.memory.owned_allocation(pointer = core.memory.byte_address<Payload>(pointer), bytes = 1, align = 1),
                    trusted core.memory.valid_read<Payload>(pointer = pointer, count = 1),
                    trusted core.memory.aligned_for<Payload>(pointer = pointer),
                    trusted core.memory.initialized_as<Payload>(pointer = pointer),
                )
                uses(raw_memory, manual_alloc)
            {{
                let value = trusted core.memory.read<Payload>(pointer);
                let address = core.memory.byte_address<Payload>(pointer);
                trusted core.memory.deallocate(address, bytes = 1, align = 1);
                {completion}
            }}
            "#,
        );

        let compilation = compilation(&source);

        assert_eq!(
            !compilation.check_diagnostics().has_errors(),
            valid,
            "{source}: {:?}",
            compilation.check_diagnostics()
        );
    }
}

#[test]
fn moving_a_witness_preserves_the_new_owner() {
    let compilation = compilation(
        r#"
        trusted module app;
        struct Owner
        {
            epoch: u64;
        }
        trusted predicate live(value: &Owner);
        trusted func owner() -> Owner
            ensures(trusted live(&result))
        {
            return
            {
                epoch = 1
            };
        }
        trusted func observe(pos value: &Owner)
            requires(trusted live(value))
        {
        }
        func caller()
        {
            let value = owner();
            let moved = value;
            observe(&moved);
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
fn an_acknowledged_producer_establishes_guarantees_when_its_obligation_is_exposed() {
    let compilation = compilation(
        r#"
        trusted module app;
        struct Owner
        {
            epoch: u64;
        }
        trusted predicate admitted(epoch: u64);
        trusted predicate live(value: &Owner);
        trusted func owner(pos epoch: u64) -> Owner
            requires(trusted admitted(epoch)) ensures(trusted live(&result))
            {
                return
                {
                    epoch = epoch
                };
            }
        func observe(pos value: &Owner)
            requires(trusted live(value))
        {
        }
        func caller(pos epoch: u64)
            requires(trusted admitted(epoch))
        {
            let value = trusted owner(epoch); observe(&value);
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
fn safe_forwarders_preserve_existing_witnesses() {
    let compilation = compilation(
        r#"
        trusted module app;
        struct Owner
        {
            epoch: u64;
        }
        trusted predicate live(value: &Owner);
        trusted func owner() -> Owner
            ensures(trusted live(&result))
        {
            return
            {
                epoch = 1
            };
        }
        func forward(pos value: Owner) -> Owner
            requires(trusted live(&value)) ensures(trusted live(&result))
            {
                return value;
            }
        trusted func observe(pos value: &Owner)
            requires(trusted live(value))
        {
        }
        func caller()
        {
            let value = forward(owner());
            observe(&value);
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
fn ordinary_producer_requirements_guard_trusted_guarantees() {
    for (value, valid) in [("1", true), ("0", false), ("epoch", true)] {
        let compilation = compilation(&format!(
            r#"
            trusted module app;
            struct Owner
            {{
                epoch: u64;
            }}
            trusted predicate live(value: &Owner);
            trusted func owner(pos epoch: u64) -> Owner
                requires(epoch == 1)
                ensures(trusted live(&result))
                {{
                    return
                    {{
                        epoch = epoch
                    }};
                }}
            trusted func observe(pos value: &Owner)
                requires(trusted live(value))
            {{
            }}
            func caller(pos epoch: u64)
            {{
                let value = owner({value});
                observe(&value);
            }}
        "#
        ));

        assert_eq!(
            !compilation.check_diagnostics().has_errors(),
            valid,
            "{:?}",
            compilation.check_diagnostics()
        );
    }
}
