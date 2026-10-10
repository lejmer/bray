use bray_diagnostics::DiagnosticKind;

use crate::test_support::compilation;

#[test]
fn ordinary_observers_supply_extents_to_trusted_memory_operations() {
    for (postcondition, valid) in [("ensures(result == count)", true), ("", false)] {
        let source = format!(
            r#"
                trusted module app;

                func observe(pos count: usize) -> usize
                    {postcondition}
                    executes(pure, total)
                {{
                    return count;
                }}

                trusted func copy(pos source: RawPointer<u8>, pos destination: RawPointer<u8>, count: usize)
                    requires(
                        trusted core.memory.valid_read<u8>(pointer = source, count = count),
                        trusted core.memory.valid_write<u8>(pointer = destination, count = count),
                        trusted core.memory.initialized_range_as<u8>(pointer = source, count = count),
                    )
                    uses(raw_memory, unchecked_init)
                {{
                    let observed: usize = observe(count);

                    core.memory.copy_overlapping<u8>(source, destination, count = observed);
                }}
            "#,
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
fn closed_writes_preserve_storage_validity_but_opaque_calls_revoke_it() {
    for (intervening, valid) in [("", true), ("opaque();", false)] {
        let source = format!(
            r#"
            trusted module app;
            func opaque() {{}}
            trusted func roundtrip(pos pointer: RawPointer<u8>) -> u8
                requires(
                    trusted core.memory.valid_read<u8>(pointer = pointer, count = 1),
                    trusted core.memory.valid_write<u8>(pointer = pointer, count = 1),
                    trusted core.memory.aligned_for<u8>(pointer = pointer),
                )
                uses(raw_memory, unchecked_init)
            {{
                core.memory.write<u8>(pointer, 1);
                {intervening}
                return core.memory.read<u8>(pointer);
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
fn returned_terminal_storage_keeps_initialization_with_its_owner() {
    for (producer, intervening, valid) in [
        ("produce()", "", true),
        ("(1, core.memory.uninit<u32>())", "", false),
        (
            "produce()",
            "terminal.1 = core.memory.uninit<u32>();",
            false,
        ),
    ] {
        let source = format!(
            r#"
            trusted module app;
            trusted func produce() -> (u32, Uninit<u32>)
                ensures(result.0 != 1 || trusted core.memory.uninit_initialized<u32>(storage = &result.1))
            {{
                let mut storage: Uninit<u32> = core.memory.uninit<u32>();
                let _: &mut u32 = core.memory.uninit_write<u32>(&mut storage, 7);
                return (1, storage);
            }}
            trusted func consume_terminal() -> u32
                uses(unchecked_init)
            {{
                let mut terminal: (u32, Uninit<u32>) = {producer};
                if terminal.0 == 1
                {{
                    {intervening}
                    return core.memory.move_initialized<u32>(&mut terminal.1);
                }}
                return 0;
            }}
        "#
        );

        let compilation = compilation(&source);

        assert_eq!(
            !compilation.check_diagnostics().has_errors(),
            valid,
            "{source}: {:#?}",
            compilation.check_diagnostics()
        );
    }
}

#[test]
fn conditional_initialization_guarantees_from_field_observers() {
    for (status, branch, valid) in [
        (
            "1",
            "if state == 1 { let _ = trusted core.memory.move_initialized<T>(&mut storage.value); }",
            true,
        ),
        (
            "COMPLETE",
            "if state == COMPLETE { let _ = trusted core.memory.move_initialized<T>(&mut storage.value); }",
            true,
        ),
        (
            "1",
            "match state { case 1 { let _ = trusted core.memory.move_initialized<T>(&mut storage.value); } case _ {} }",
            true,
        ),
        (
            "COMPLETE",
            "match state { case COMPLETE { let _ = trusted core.memory.move_initialized<T>(&mut storage.value); } case _ {} }",
            true,
        ),
        (
            "COMPLETE",
            "if state == OTHER { let _ = trusted core.memory.move_initialized<T>(&mut storage.value); }",
            false,
        ),
        (
            "COMPLETE",
            "match state { case OTHER { let _ = trusted core.memory.move_initialized<T>(&mut storage.value); } case _ {} }",
            false,
        ),
        (
            "COMPLETE",
            "if state == COMPLETE { storage.value = core.memory.uninit<T>(); let _ = trusted core.memory.move_initialized<T>(&mut storage.value); }",
            false,
        ),
        (
            "COMPLETE",
            "match state { case COMPLETE { storage.value = core.memory.uninit<T>(); let _ = trusted core.memory.move_initialized<T>(&mut storage.value); } case _ {} }",
            false,
        ),
    ] {
        let source = format!(
            r#"
            trusted module app;
            const COMPLETE: u32 = 1;
            const OTHER: u32 = 2;
            struct Slots<T> {{ mut value: Uninit<T>; state: u32; }}
            trusted func observe<T>(pos storage: &mut Slots<T>) -> u32
                ensures(result != {status} || trusted core.memory.uninit_initialized<T>(storage = &storage.value))
            {{
                return storage.state;
            }}
            trusted func consume_initialized_field<T>(pos storage: &mut Slots<T>)
                uses(unchecked_init)
            {{
                let state: u32 = trusted observe<T>(storage);
                {branch}
            }}
        "#
        );

        let compilation = compilation(&source);

        assert_eq!(
            !compilation.check_diagnostics().has_errors(),
            valid,
            "{source}: {:#?}",
            compilation.check_diagnostics()
        );
    }
}

#[test]
fn conditional_initialization_guarantees_reach_borrowed_fields() {
    for (guard, mutation, valid) in [
        ("state == 1", "", true),
        ("state == 0", "", false),
        (
            "state == 1",
            "storage.value = core.memory.uninit<u8>();",
            false,
        ),
    ] {
        let source = format!(
            r#"
            trusted module app;
            struct Slots {{ mut value: Uninit<u8>; }}

            trusted func initialize(pos storage: &mut Slots, fail: bool) -> u32
                ensures(result != 1 || trusted core.memory.uninit_initialized<u8>(storage = &storage.value))
            {{
                if fail {{ return 0; }}
                let _ = core.memory.uninit_write<u8>(&mut storage.value, 42);
                return 1;
            }}

            trusted func consume_initialized_field(pos storage: &mut Slots, fail: bool)
                uses(unchecked_init)
            {{
                let state: u32 = trusted initialize(storage, fail = fail);
                {mutation}
                if {guard} {{ let _ = trusted core.memory.move_initialized<u8>(&mut storage.value); }}
            }}
        "#
        );

        let compilation = compilation(&source);

        assert_eq!(
            !compilation.check_diagnostics().has_errors(),
            valid,
            "{source}: {:#?}",
            compilation.check_diagnostics()
        );
    }
}

#[test]
fn initialized_storage_requirements_supply_consuming_operations() {
    for (signature, operation) in [
        (
            "pos storage: &mut Uninit<u8>",
            "core.memory.move_initialized<u8>(storage)",
        ),
        (
            "pos mut storage: Uninit<u8>",
            "core.memory.assume_initialized<u8>(storage)",
        ),
    ] {
        let argument = "&storage";

        let source = format!(
            r#"
            trusted module app;
            trusted func consume_initialized({signature}) -> u8
                requires(trusted core.memory.uninit_initialized<u8>(storage = {argument}))
                uses(unchecked_init)
            {{
                return trusted {operation};
            }}
        "#
        );

        let compilation = compilation(&source);
        let diagnostics = compilation.check_diagnostics();

        assert!(!diagnostics.has_errors(), "{source}: {diagnostics:?}");
    }
}

#[test]
fn conditional_memory_obligations_accept_smaller_proven_ranges() {
    for (condition, valid) in [
        (
            "count == 0 || trusted core.memory.valid_write<u8>(pointer = pointer, count = count)",
            true,
        ),
        (
            "count == 0 && trusted core.memory.valid_write<u8>(pointer = pointer, count = count)",
            false,
        ),
        (
            "count == 0 || trusted core.memory.valid_write<u8>(pointer = pointer, count = count + 1)",
            false,
        ),
    ] {
        let source = format!(
            r#"
            trusted module app;
            extern trusted func consume_range(pos pointer: RawPointer<u8>, count: usize)
                requires({condition});

            trusted func caller(pos pointer: RawPointer<u8>, extent: usize, count: usize)
                requires(
                    count > 0,
                    count <= extent,
                    trusted core.memory.valid_write<u8>(pointer = pointer, count = extent),
                )
            {{
                trusted consume_range(pointer, count = count);
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

    let compilation = compilation(
        r#"
        trusted module app;
        extern trusted func consume_range(pos pointer: RawPointer<u8>, count: usize)
            requires(count == 0 || trusted core.memory.valid_write<u8>(pointer = pointer, count = count));

        trusted func caller(pos pointer: RawPointer<u8>, count: usize)
            requires(count == 0 || trusted core.memory.valid_write<u8>(pointer = pointer, count = count))
        {
            trusted consume_range(pointer, count = count);
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
fn closed_memory_ranges_can_supply_smaller_checked_ranges() {
    for (range, valid) in [(0, false), (1, true), (2, true)] {
        let source = format!(
            r#"
            trusted module app;
            trusted func write_byte(pos pointer: RawPointer<u8>)
                requires(
                    trusted core.memory.valid_write<u8>(pointer = pointer, count = {range}),
                    trusted core.memory.aligned_for<u8>(pointer = pointer),
                )
                uses(raw_memory, unchecked_init)
            {{
                core.memory.write<u8>(pointer, 1);
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
fn checked_nonempty_ranges_supply_one_element_obligations() {
    for (guard, valid) in [("count > 0", true), ("count == 0", false)] {
        let source = format!(
            r#"
            trusted module app;
            trusted func write_byte(pos pointer: RawPointer<u8>, count: usize)
                requires(
                    trusted core.memory.valid_write<u8>(pointer = pointer, count = count),
                    trusted core.memory.aligned_for<u8>(pointer = pointer),
                )
                uses(raw_memory, unchecked_init)
            {{
                if {guard} {{ core.memory.write<u8>(pointer, 1); }}
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
fn borrowed_addresses_establish_live_access_conditions() {
    let compilation = compilation(
        r#"
        trusted module app;
        trusted func caller() -> u8 uses(raw_memory, unchecked_init) {
            let mut value: u8 = 1;
            let pointer = core.memory.address_of_mut<u8>(&mut value);
            trusted core.memory.write<u8>(pointer, 2);
            return trusted core.memory.read<u8>(pointer);
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
fn writing_through_an_address_does_not_extend_its_owner_lifetime() {
    let compilation = compilation(
        r#"
        trusted module app;
        trusted func caller() -> u8 uses(raw_memory, unchecked_init) {
            let unrelated: u8 = 1;
            let mut pointer = core.memory.null<u8>();
            {
                let mut value: u8 = 1;
                pointer = core.memory.address_of_mut<u8>(&mut value);
                core.memory.write<u8>(pointer, 2);
            };
            return core.memory.read<u8>(pointer);
        }
    "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(),
        DiagnosticKind::CheckingTrustedObligationNotProven,
    );
}

#[test]
fn copied_range_extents_describe_storage_without_owning_it() {
    let compilation = compilation(
        r#"
        trusted module app;

        trusted func extent(pos value: &u8) -> usize
            ensures(
                trusted core.memory.valid_read<u8>(
                    pointer = core.memory.address_of<u8>(value),
                    count = result,
                ),
            )
        {
            return 1;
        }

        func caller() {
            let value: u8 = 1;
            let count: usize = extent(&value);
            let copied: usize = count;
            let _ = copied;
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
fn borrowed_invariants_survive_loops_only_when_every_back_edge_preserves_them() {
    for (body, valid) in [
        ("trusted advance(&mut owner);", true),
        ("if choose { trusted advance(&mut owner); }", true),
        (
            "if choose { trusted advance(&mut owner); } else { owner.value += 1; }",
            false,
        ),
        ("owner.value += 1;", false),
        ("trusted advance(&mut owner); owner.value += 1;", false),
        (
            "if choose { trusted advance(&mut owner); } else { trusted advance(&mut owner); } owner.value += 1;",
            false,
        ),
    ] {
        let compilation = compilation(&format!(
            r#"
            trusted module app;
            struct Owner {{ mut value: usize; }}
            trusted predicate valid(owner: &Owner);
            extern trusted func observe(pos owner: &Owner)
                requires(trusted valid(owner = owner))
                ensures(trusted valid(owner = owner));
            extern trusted func advance(pos owner: &mut Owner)
                requires(trusted valid(owner = &owner))
                ensures(trusted valid(owner = &owner));
            trusted func caller(pos mut owner: Owner, pos choose: bool)
                requires(trusted valid(owner = &owner))
            {{
                loop {{
                    trusted observe(&owner);
                    {body}
                }}
            }}
        "#
        ));

        assert_eq!(
            !compilation.check_diagnostics().has_errors(),
            valid,
            "{body}: {:?}",
            compilation.check_diagnostics()
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
        struct Owner { value: u8; }
        trusted predicate valid(owner: &Owner);

        extern trusted func address(pos owner: &Owner) -> RawPointer<u8>
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
            let pointer: RawPointer<u8> = trusted address(owner);
            let copied: RawPointer<u8> = pointer;

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
fn copied_callable_addresses_preserve_validity_without_creating_it() {
    for (producer, valid) in [
        ("core.memory.pointer_from_callable<Callback>(entry)", true),
        ("core.memory.null<Callback>()", false),
    ] {
        let compilation = compilation(&format!(
            r#"
            trusted module app;
            callable Callback = @abi(c) func();

            @abi(c)
            func entry() {{}}

            trusted func caller()
                uses(layout_reinterpret)
            {{
                let pointer: RawPointer<Callback> = {producer};
                let copied: RawPointer<Callback> = pointer;
                let callback: Callback = core.memory.callable_from_pointer<Callback>(copied);
                let _ = callback;
            }}
            "#
        ));

        if valid {
            assert!(
                !compilation.check_diagnostics().has_errors(),
                "{:?}",
                compilation.check_diagnostics()
            );
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                compilation.check_diagnostics(),
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn raw_addresses_cannot_bypass_custom_witness_copy_contracts() {
    let compilation = compilation(
        r#"
        trusted module app;
        trusted predicate live(pointer: RawPointer<u8>);
        trusted func owner() -> RawPointer<u8> ensures(trusted live(result))
        {
            return core.memory.null<u8>();
        }
        func caller() { let pointer = owner(); let copy = pointer; }
    "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(),
        DiagnosticKind::CheckingTrustedWitnessTransferNotProven,
    );
}

#[test]
fn conditional_storage_guarantees_preserve_copyable_addresses() {
    for (ty, predicate) in [
        ("RawPointer<u8>", "core.memory.valid_read<u8>"),
        ("DevicePointer<u8>", "core.target.device_valid_read<u8>"),
    ] {
        for guard in ["flag", "enabled(flag)"] {
            let compilation = compilation(&format!(
                r#"
                trusted module app;
                predicate enabled(flag: bool) = flag;
                extern trusted func address(pos flag: bool) -> {ty}
                    ensures(!{guard} || trusted {predicate}(pointer = result, count = 1));
                func caller() {{ let pointer = address(true); let copy = pointer; }}
            "#
            ));

            assert!(
                !compilation.check_diagnostics().has_errors(),
                "{ty}, {guard}: {:?}",
                compilation.check_diagnostics()
            );
        }
    }
}

#[test]
fn device_addresses_cannot_bypass_custom_witness_copy_contracts() {
    let compilation = compilation(
        r#"
        trusted module app;
        trusted predicate live(pointer: DevicePointer<u8>);
        extern trusted func owner() -> DevicePointer<u8> ensures(trusted live(result));
        func caller() { let pointer = owner(); let copy = pointer; }
    "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(),
        DiagnosticKind::CheckingTrustedWitnessTransferNotProven,
    );
}

#[test]
fn address_guarantees_expire_with_the_borrowed_storage_owner() {
    for (invocation, valid) in [("observe(pointer);", true), ("", false)] {
        let later = if valid { "" } else { "observe(pointer);" };

        let compilation = compilation(&format!(
            r#"
            trusted module app;
            struct Owner {{ byte: Uninit<u8>; }}
            trusted func address_view(pos owner: &Owner) -> RawPointer<u8>
                executes(pure, total)
                ensures(trusted core.memory.valid_read<u8>(pointer = result, count = 1))
            {{ return core.memory.uninit_pointer<u8>(&owner.byte); }}
            func observe(pos pointer: RawPointer<u8>)
                requires(trusted core.memory.valid_read<u8>(pointer = pointer, count = 1)) {{}}
            func caller() {{
                let mut pointer = core.memory.null<u8>();
                {{
                    let owner: Owner = {{ byte = core.memory.uninit<u8>() }};
                    pointer = address_view(&owner);
                    {invocation}
                }};
                {later}
            }}
        "#
        ));

        let diagnostics = compilation.check_diagnostics();

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
fn conditional_trusted_guarantees_require_the_selected_entry_guard() {
    for (flag, valid) in [("true", true), ("false", false)] {
        let compilation = compilation(&format!(
            r#"
            trusted module app;
            struct Owner {{ byte: u8; }}
            trusted predicate live(value: &Owner);
            trusted func owner(pos flag: bool) -> Owner
                when(flag) {{ ensures(trusted live(&result)) }}
            {{ return {{ byte = 1 }}; }}
            func observe(pos value: &Owner) requires(trusted live(value)) {{}}
            func caller() {{ let value = owner({flag}); observe(&value); }}
        "#
        ));

        let diagnostics = compilation.check_diagnostics();

        assert_eq!(!diagnostics.has_errors(), valid, "{diagnostics:?}");
    }
}

#[test]
fn scoped_guarantees_and_requirements_use_the_actual_capability() {
    for (enter_mode, exit_mode, caller_mode) in [
        ("", "", ""),
        ("async ", "", "async "),
        ("", "async ", "async "),
        ("async ", "async ", "async "),
    ] {
        for (enter_contract, exit_contract, body, valid) in [
            (
                "ensures(trusted live(&result))",
                "",
                "observe(&lease);",
                true,
            ),
            ("", "", "observe(&lease);", false),
            (
                "ensures(trusted live(&result))",
                "requires(trusted live(&lease))",
                "",
                true,
            ),
            ("", "requires(trusted live(&lease))", "", false),
        ] {
            let compilation = compilation(&format!(
                r#"
            trusted module app;
            struct Resource {{}}
            struct Lease {{ tag: bool; }}
            trusted predicate live(value: &Lease);
            impl Resource
            {{
                trusted {enter_mode}enter() -> Lease {enter_contract} {{ return {{ tag = true }}; }}
                {exit_mode}exit(pos lease: Lease) {exit_contract} {{}}
            }}
            func observe(pos value: &Lease) requires(trusted live(value)) {{}}
            {caller_mode}func caller(pos resource: Resource)
            {{
                with lease = resource {{ {body} }};
            }}
            "#,
            ));

            let diagnostics = compilation.check_diagnostics();

            assert_eq!(
                !diagnostics.has_errors(),
                valid,
                "enter {enter_mode:?}, exit {exit_mode:?}: {diagnostics:?}"
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
fn scoped_exit_must_explicitly_transfer_trusted_authority() {
    for (exit_contract, borrowed, valid) in [
        ("", false, false),
        ("ensures(trusted live(lease.tag))", false, true),
        ("ensures(trusted live(&lease.tag))", true, false),
    ] {
        let observation = if borrowed { "&" } else { "" };

        let compilation = compilation(&format!(
            r#"
            trusted module app;
            struct Resource {{ tag: bool; }}
            struct Lease {{ tag: bool; }}
            trusted predicate live(value: {observation}bool);
            impl Resource
            {{
                trusted enter() -> Lease
                    ensures(result.tag == self.tag, trusted live({observation}result.tag))
                    executes(pure)
                {{ return {{ tag = self.tag }}; }}
                trusted exit(pos lease: Lease) {exit_contract} executes(pure) {{}}
            }}
            func observe(pos value: {observation}bool)
                requires(trusted live(value)) executes(pure) {{}}
            func caller(pos resource: &Resource)
            {{
                with lease = resource {{ let copied = lease.tag; let _ = copied; }};
                observe({observation}resource.tag);
            }}
            "#,
        ));

        let diagnostics = compilation.check_diagnostics();

        assert_eq!(
            !diagnostics.has_errors(),
            valid,
            "{exit_contract}: {diagnostics:?}"
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
fn vacuous_conditional_guarantees_do_not_restrict_ordinary_copies() {
    for guarantee in [
        "when(flag) { ensures(trusted live(&result)) }",
        "ensures(!flag || trusted live(&result))",
    ] {
        for (argument, copy, valid) in [
            ("false", "let copy = value;", true),
            ("true", "let copy = value;", false),
            ("flag", "let copy = value;", false),
            ("flag", "if !flag { let copy = value; }", true),
        ] {
            let compilation = compilation(&format!(
                r#"
                trusted module app;
                @copy struct Owner {{ epoch: u64; }}
                trusted predicate live(value: &Owner);
                trusted func owner(pos flag: bool) -> Owner {guarantee} {{ return {{ epoch = 1 }}; }}
                func caller(pos flag: bool) {{ let value = owner({argument}); {copy} }}
            "#
            ));

            assert_eq!(
                !compilation.check_diagnostics().has_errors(),
                valid,
                "{guarantee}, {argument}, {copy}: {:?}",
                compilation.check_diagnostics()
            );
        }
    }
}

#[test]
fn output_authority_copy_checks_follow_current_completion_guards() {
    for guarantee in [
        "when(flag) { ensures(trusted live(value)) }",
        "ensures(!flag || trusted live(value))",
    ] {
        let compilation = compilation(&format!(
            r#"
            trusted module app;
            @copy struct Owner {{ epoch: u64; }}
            trusted predicate live(value: &mut Owner);
            trusted func initialize(pos flag: bool, pos value: &mut Owner) {guarantee} {{}}
            func caller(pos flag: bool) {{
                let mut value: Owner = {{ epoch = 0 }};
                initialize(flag, &mut value);
                if !flag {{ let copy = value; }}
            }}
        "#
        ));

        assert!(
            !compilation.check_diagnostics().has_errors(),
            "{guarantee}: {:?}",
            compilation.check_diagnostics()
        );
    }

    let compilation = compilation(
        r#"
        trusted module app;
        @copy struct Owner { epoch: u64; }
        trusted predicate live(value: &mut Owner);
        trusted func initialize(pos value: &mut Owner) -> u32
            ensures(result != 0 || trusted live(value)) { return 0; }
        func caller() {
            let mut value: Owner = { epoch = 0 };
            let status = initialize(&mut value);
            if status != 0 { let copy = value; }
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
fn structural_storage_preserves_whole_moved_and_fresh_owner_guarantees() {
    for body in [
        "let value = owner(); let holder: Holder = { owner = value }; observe(&holder.owner);",
        "let holder: Holder = { owner = owner() }; observe(&holder.owner);",
        "let holder: Holder = { owner = { let value = owner(); yield value; } }; observe(&holder.owner);",
        "let nested: Nested = { holder = { owner = owner() } }; observe(&nested.holder.owner);",
        "let holder: Holder = { owner = owner() }; let moved = holder; observe(&moved.owner);",
        "let holder: Holder = { owner = owner() }; let moved = holder.owner; observe(&moved);",
        "let mut holder: Holder = { owner = { epoch = 0 } }; holder.owner = owner(); observe(&holder.owner);",
    ] {
        let compilation = compilation(&format!(
            r#"
            trusted module app;
            struct Owner {{ epoch: u64; }}
            struct Holder {{ mut owner: Owner; }}
            struct Nested {{ holder: Holder; }}
            trusted predicate live(value: &Owner);
            trusted func owner() -> Owner ensures(trusted live(&result)) {{ return {{ epoch = 1 }}; }}
            func observe(pos value: &Owner) requires(trusted live(value)) {{}}
            func caller() {{ {body} }}
        "#
        ));

        assert!(
            !compilation.check_diagnostics().has_errors(),
            "{body}: {:?}",
            compilation.check_diagnostics()
        );
    }
}

#[test]
fn copying_a_structural_owner_cannot_duplicate_nested_authority() {
    let compilation = compilation(
        r#"
        trusted module app;
        @copy struct Owner { epoch: u64; }
        @copy struct Holder { owner: Owner; }
        trusted predicate live(value: &Owner);
        trusted func owner() -> Owner ensures(trusted live(&result)) { return { epoch = 1 }; }
        func caller() { let holder: Holder = { owner = owner() }; let copy = holder; }
    "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(),
        DiagnosticKind::CheckingTrustedWitnessTransferNotProven,
    );
}

#[test]
fn destructured_storage_preserves_whole_owner_guarantees() {
    for body in [
        "let number: u8 = 0; let (value, copied) = (owner(), number); observe(&value);",
        "let [value]: [Owner; 1] = [owner()]; observe(&value);",
        "let holder = Holder.Owned(owner()); match consume holder { case .Owned(value) { observe(&value); } }",
        "let value: Owner = owner(); let holder: Owner? = value; match consume holder { case ?present { observe(&present); } case none {} }",
        "let value = (owner(), owner()); observe(&value.0);",
        "let value = (owner(), owner()); observe(&value.1);",
        "let value: [Owner; 1] = [owner()]; observe(&value[0]);",
        "let number: u8 = 0; let (_, value) = (number, owner()); observe(&value);",
        "let [first, .., last]: [Owner; 2] = [owner(), owner()]; observe(&first);",
        "let [first, .., last]: [Owner; 2] = [owner(), owner()]; observe(&last);",
        "let value = { let number: u8 = 0; let pair = (owner(), number); let (value, copied) = pair; yield value; }; observe(&value);",
        "let value = { let values: [Owner; 1] = [owner()]; let [value] = values; yield value; }; observe(&value);",
    ] {
        let compilation = compilation(&format!(
            r#"
            trusted module app;
            struct Owner {{ epoch: u64; }}
            union Holder {{ Owned(pos value: Owner); }}
            trusted predicate live(value: &Owner);
            trusted func owner() -> Owner executes(pure, total)
                ensures(trusted live(&result)) {{ return {{ epoch = 1 }}; }}
            func observe(pos value: &Owner) executes(pure, total) requires(trusted live(value)) {{}}
            func caller() {{ {body} }}
        "#
        ));

        assert!(
            !compilation.check_diagnostics().has_errors(),
            "{body}: {:?}",
            compilation.check_diagnostics()
        );
    }
}

#[test]
fn aggregate_copy_checks_apply_to_the_transferred_component() {
    for (body, valid) in [
        (
            "let number: u8 = 0; let pair = (owner(), number); let (value, copied) = pair;",
            false,
        ),
        (
            "let number: u8 = 0; let (value, copied) = (owner(), number);",
            true,
        ),
        (
            "let values: [Owner; 1] = [owner()]; let copy = values;",
            false,
        ),
        (
            "let number: u8 = 0; let pair = (owner(), number); let copied = pair.1;",
            true,
        ),
        (
            "let number: u8 = 0; let pair = (owner(), number); let copy = pair.0;",
            false,
        ),
        (
            "let values: [Owner; 1] = [owner()]; let copy = values[0];",
            false,
        ),
        ("let values: [Owner; 2] = [owner(); 2];", false),
        ("let values: [Owner; 1] = [owner(); 1];", true),
    ] {
        let compilation = compilation(&format!(
            r#"
            trusted module app;
            @copy struct Owner {{ epoch: u64; }}
            trusted predicate live(value: &Owner);
            trusted func owner() -> Owner ensures(trusted live(&result)) {{ return {{ epoch = 1 }}; }}
            func caller() {{ {body} }}
        "#
        ));

        assert_eq!(
            !compilation.check_diagnostics().has_errors(),
            valid,
            "{body}: {:?}",
            compilation.check_diagnostics()
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                compilation.check_diagnostics(),
                DiagnosticKind::CheckingTrustedWitnessTransferNotProven,
            );
        }
    }
}

#[test]
fn equal_aggregate_elements_cannot_share_borrowed_authority() {
    for body in [
        "let first: [u8; 1] = [0]; let second: [u8; 1] = [0]; establish(&first[0]); observe(&second[0]);",
        "let value = (false, false); establish_bool(&value.0); observe_bool(&value.1);",
    ] {
        let compilation = compilation(&format!(
            r#"
            trusted module app;
            trusted predicate live(value: &u8);
            trusted predicate live_bool(value: &bool);
            trusted func establish(pos value: &u8) executes(pure, total) ensures(trusted live(value)) {{}}
            trusted func establish_bool(pos value: &bool) executes(pure, total) ensures(trusted live_bool(value)) {{}}
            func observe(pos value: &u8) requires(trusted live(value)) {{}}
            func observe_bool(pos value: &bool) requires(trusted live_bool(value)) {{}}
            func caller() {{ {body} }}
        "#
        ));

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            DiagnosticKind::CheckingTrustedObligationNotProven,
        );
    }
}

#[test]
fn repeated_borrows_of_one_scalar_component_preserve_authority() {
    for body in [
        "let first: [u8; 1] = [0]; establish(&first[0]); observe(&first[0]);",
        "let value = (false, false); establish_bool(&value.0); observe_bool(&value.0);",
        "let values: [[u8; 1]; 1] = [[0]]; establish(&values[0][0]); observe(&values[0][0]);",
        "let value = ((false, false), false); establish_bool(&(value.0).0); observe_bool(&(value.0).0);",
    ] {
        let compilation = compilation(&format!(
            r#"
            trusted module app;
            trusted predicate live(value: &u8);
            trusted predicate live_bool(value: &bool);
            trusted func establish(pos value: &u8) executes(pure, total) ensures(trusted live(value)) {{}}
            trusted func establish_bool(pos value: &bool) executes(pure, total) ensures(trusted live_bool(value)) {{}}
            func observe(pos value: &u8) requires(trusted live(value)) {{}}
            func observe_bool(pos value: &bool) requires(trusted live_bool(value)) {{}}
            func caller() {{ {body} }}
        "#
        ));

        assert!(
            !compilation.check_diagnostics().has_errors(),
            "{body}: {:?}",
            compilation.check_diagnostics()
        );
    }
}

#[test]
fn unknown_array_selectors_cannot_borrow_another_components_authority() {
    let compilation = compilation(
        r#"
        trusted module app;
        trusted predicate live(value: &u8);
        trusted func establish(pos value: &u8) executes(pure, total) ensures(trusted live(value)) {}
        func observe(pos value: &u8) requires(trusted live(value)) {}
        func caller(pos index: usize) {
            let values: [u8; 2] = [0, 0];
            establish(&values[0]);
            observe(&values[index]);
        }
    "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(),
        DiagnosticKind::CheckingTrustedObligationNotProven,
    );
}

#[test]
fn ordinary_predicate_assumptions_cannot_discharge_trusted_disjunctions() {
    let compilation = compilation(
        r#"
        trusted module app;
        trusted predicate live(value: u64);
        func observe(pos value: u64, pos flag: bool) requires((trusted live(value)) || flag) {}
        func caller(pos value: u64, pos flag: bool) requires(live(value)) { observe(value, flag); }
    "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(),
        DiagnosticKind::CheckingTrustedObligationNotProven,
    );
}

#[test]
fn mixed_boolean_requirements_preserve_trusted_occurrences_and_ordinary_guards() {
    for (requirement, precondition, valid) in [
        ("(trusted live(value)) || flag", "live(value)", false),
        ("flag || trusted live(value)", "live(value)", false),
        ("(trusted live(value)) && flag", "live(value), flag", false),
        ("!(trusted live(value)) || flag", "!live(value)", false),
        ("flag || trusted live(value)", "flag", true),
        ("(trusted live(value)) || flag", "flag", true),
        ("ready(value) || trusted live(value)", "ready(value)", true),
        (
            "!(trusted live(value)) || ready(value)",
            "ready(value)",
            true,
        ),
        ("(trusted live(value)) || flag", "trusted live(value)", true),
    ] {
        let source = format!(
            r#"
            trusted module app;
            trusted predicate live(value: u64);
            trusted predicate ready(value: u64);
            func observe(pos value: u64, pos flag: bool) requires({requirement}) {{}}
            func caller(pos value: u64, pos flag: bool) requires({precondition}) {{ observe(value, flag); }}
        "#
        );

        let compilation = compilation(&source);

        if valid {
            assert!(
                !compilation.check_diagnostics().has_errors(),
                "{source}: {:?}",
                compilation.check_diagnostics()
            );
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                compilation.check_diagnostics(),
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn selecting_an_ordinary_disjunct_does_not_grant_its_trusted_qualification() {
    for requirement in [
        "live(value) || trusted ready(value)",
        "(trusted ready(value)) || live(value)",
    ] {
        let source = format!(
            r#"
            trusted module app;
            trusted predicate live(value: u64);
            trusted predicate ready(value: u64);
            func observe(pos value: u64) requires(trusted live(value)) {{}}
            func caller(pos value: u64) requires({requirement}, !(trusted ready(value))) {{ observe(value); }}
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
fn trusted_disjunctions_retain_evidence_on_either_side() {
    for requirement in [
        "(trusted live(value)) || flag",
        "flag || trusted live(value)",
    ] {
        let source = format!(
            r#"
            trusted module app;
            trusted predicate live(value: u64);
            func observe(pos value: u64) requires(trusted live(value)) {{}}
            func caller(pos value: u64, pos flag: bool) requires({requirement}, !flag) {{ observe(value); }}
        "#
        );

        let compilation = compilation(&source);

        assert!(
            !compilation.check_diagnostics().has_errors(),
            "{source}: {:?}",
            compilation.check_diagnostics()
        );
    }
}

#[test]
fn negative_entry_evidence_retains_witness_copy_obligations() {
    let compilation = compilation(
        r#"
        trusted module app;
        @copy struct Owner { epoch: u64; }
        trusted predicate dangerous(value: &Owner);
        func duplicate(pos value: Owner) requires(!(trusted dangerous(&value))) {
            let copied = value;
        }
    "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(),
        DiagnosticKind::CheckingTrustedWitnessTransferNotProven,
    );
}

#[test]
fn negative_trusted_guarantees_retain_witness_copy_obligations() {
    for (body, valid) in [
        ("let value = direct_owner(); let copied = value;", false),
        ("let value = owner(false); let copied = value;", false),
        ("let value = owner(true); let copied = value;", true),
    ] {
        let source = format!(
            r#"
            trusted module app;
            @copy struct Owner {{ epoch: u64; }}
            trusted predicate dangerous(value: &Owner);
            trusted func direct_owner() -> Owner
                ensures(!(trusted dangerous(&result))) {{ return {{ epoch = 1 }}; }}
            trusted func owner(pos flag: bool) -> Owner
                ensures(flag || !(trusted dangerous(&result))) {{ return {{ epoch = 1 }}; }}
            func caller() {{ {body} }}
        "#
        );

        let compilation = compilation(&source);

        if valid {
            assert!(
                !compilation.check_diagnostics().has_errors(),
                "{source}: {:?}",
                compilation.check_diagnostics()
            );
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                compilation.check_diagnostics(),
                DiagnosticKind::CheckingTrustedWitnessTransferNotProven,
            );
        }
    }
}

#[test]
fn ordinary_predicate_equalities_cannot_rename_trusted_authority() {
    let compilation = compilation(
        r#"
        trusted module app;
        trusted predicate live(value: u64);
        trusted predicate other(value: u64);
        func observe(pos value: u64) requires(trusted other(value)) {}
        func caller(pos value: u64) requires(trusted live(value), other(value) == live(value)) {
            observe(value);
        }
    "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(),
        DiagnosticKind::CheckingTrustedObligationNotProven,
    );
}

#[test]
fn equal_scalar_contents_do_not_identify_borrowed_predicate_subjects() {
    for requirement in ["trusted live(&first)", "ready(&first)"] {
        let source = format!(
            r#"
            trusted module app;
            trusted predicate live(value: &u8);
            trusted predicate ready(value: &u8);
            func observe(pos value: &u8) requires(trusted live(value)) {{}}
            trusted func make(pos value: &u8) -> u64 executes(pure, total)
                when(ready(value)) {{ ensures(trusted result_live(result)) }} {{ return 0; }}
            trusted predicate result_live(value: u64);
            func observe_result(pos value: u64) requires(trusted result_live(value)) {{}}
            func caller(pos first: u8, pos second: u8) requires(first == second, {requirement}) {{
                {body}
            }}
        "#,
            body = if requirement.starts_with("trusted") {
                "observe(&second);"
            } else {
                "observe_result(make(&second));"
            }
        );

        let compilation = compilation(&source);

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            DiagnosticKind::CheckingTrustedObligationNotProven,
        );
    }
}

#[test]
fn mutation_cannot_reuse_an_ordinary_borrowed_guard_to_establish_authority() {
    let compilation = compilation(
        r#"
        trusted module app;
        trusted predicate ready(value: &u8);
        trusted predicate live(value: u64);
        trusted func make(pos value: &u8) -> u64 executes(pure, total)
            when(ready(value)) { ensures(trusted live(result)) } { return 0; }
        func observe(pos value: u64) requires(trusted live(value)) {}
        func caller(pos mut value: u8) requires(ready(&value)) {
            value = 1;
            observe(make(&value));
        }
    "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(),
        DiagnosticKind::CheckingTrustedObligationNotProven,
    );
}

#[test]
fn ordinary_borrowed_guards_select_the_producers_entry_domain() {
    let compilation = compilation(
        r#"
        trusted module app;
        trusted predicate ready(value: &u8);
        trusted predicate live(value: u64);
        trusted func make(pos value: &u8) -> u64
            when(ready(value)) { ensures(trusted live(result)) } { return 0; }
        func observe(pos value: u64) requires(trusted live(value)) {}
        func caller(pos value: u8) requires(ready(&value)) { observe(make(&value)); }
    "#,
    );

    assert!(
        !compilation.check_diagnostics().has_errors(),
        "{:?}",
        compilation.check_diagnostics()
    );
}

#[test]
fn changing_a_guard_does_not_erase_a_safe_producers_entry_obligation() {
    let compilation = compilation(
        r#"
        trusted module app;
        trusted predicate live(value: u64);
        func make(pos mut flag: bool) -> u64
            when(flag) { ensures(trusted live(result)) } { flag = false; return 0; }
    "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(),
        DiagnosticKind::CheckingTrustedObligationNotProven,
    );
}

#[test]
fn ordinary_predicate_guards_select_trusted_conditional_guarantees() {
    for (precondition, valid) in [("requires(ready(value))", true), ("", false)] {
        let source = format!(
            r#"
            trusted module app;
            trusted predicate live(value: u64);
            trusted predicate ready(value: u64);
            trusted func make(pos value: u64) -> u64
                when(ready(value)) {{ ensures(trusted live(result)) }} {{ return value; }}
            func observe(pos value: u64) requires(trusted live(value)) {{}}
            func caller(pos value: u64) {precondition} {{ observe(make(value)); }}
        "#
        );

        let compilation = compilation(&source);

        if valid {
            assert!(
                !compilation.check_diagnostics().has_errors(),
                "{source}: {:?}",
                compilation.check_diagnostics()
            );
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                compilation.check_diagnostics(),
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn scalar_component_mutation_revokes_only_overlapping_authority() {
    for (body, valid) in [
        (
            "let mut values: [u8; 2] = [0, 0]; establish(&values[0]); values[1] = 1; observe(&values[0]);",
            true,
        ),
        (
            "let mut values: [u8; 2] = [0, 0]; establish(&values[0]); values[0] = 1; observe(&values[0]);",
            false,
        ),
        (
            "let mut value = (false, false); establish_bool(&value.0); value.1 = true; observe_bool(&value.0);",
            true,
        ),
        (
            "let mut value = (false, false); establish_bool(&value.0); value.0 = true; observe_bool(&value.0);",
            false,
        ),
        (
            "let mut value = ((false, false), false); establish_bool(&(value.0).0); (value.0).1 = true; observe_bool(&(value.0).0);",
            true,
        ),
        (
            "let mut value = ((false, false), false); establish_bool(&(value.0).0); value.0 = (false, false); observe_bool(&(value.0).0);",
            false,
        ),
    ] {
        let source = format!(
            r#"
            trusted module app;
            trusted predicate live(value: &u8);
            trusted predicate live_bool(value: &bool);
            trusted func establish(pos value: &u8) executes(pure, total) ensures(trusted live(value)) {{}}
            trusted func establish_bool(pos value: &bool) executes(pure, total) ensures(trusted live_bool(value)) {{}}
            func observe(pos value: &u8) requires(trusted live(value)) {{}}
            func observe_bool(pos value: &bool) requires(trusted live_bool(value)) {{}}
            func caller() {{ {body} }}
        "#
        );

        let compilation = compilation(&source);

        if valid {
            assert!(
                !compilation.check_diagnostics().has_errors(),
                "{source}: {:?}",
                compilation.check_diagnostics()
            );
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                compilation.check_diagnostics(),
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn a_completion_status_can_be_copied_without_copying_the_output_authority() {
    let compilation = compilation(
        r#"
        trusted module app;
        struct Owner { epoch: u64; }
        trusted predicate live(value: &mut Owner);
        trusted func initialize(pos value: &mut Owner) -> u32
            ensures(result != 0 || trusted live(value)) { return 0; }
        func observe(pos value: &mut Owner) requires(trusted live(value)) {}
        func caller() {
            let mut value: Owner = { epoch = 1 };
            let status = initialize(&mut value);
            let copied_status = status;
            if copied_status == 0 { observe(&mut value); }
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
fn a_vacuous_entry_obligation_does_not_make_the_owner_a_witness() {
    let compilation = compilation(
        r#"
        trusted module app;
        @copy struct Owner { epoch: u64; }
        trusted predicate live(value: &Owner);
        func caller(pos value: Owner, flag: bool)
            requires(flag || trusted live(&value)) {
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
fn conditional_callable_types_preserve_trusted_guarantees() {
    let compilation = compilation(
        r#"
        trusted module app;
        struct Owner { byte: u8; }
        trusted predicate live(value: &Owner);
        callable Producer = trusted func(pos ready: bool) -> Owner
            when(ready) { ensures(trusted live(&result)) };
        trusted func owner(pos ready: bool) -> Owner
            when(ready) { ensures(trusted live(&result)) } { return { byte = 1 }; }
        func observe(pos value: &Owner) requires(trusted live(value)) {}
        func caller() { let produce: Producer = owner; let value = produce(true); observe(&value); }
    "#,
    );

    assert!(
        !compilation.check_diagnostics().has_errors(),
        "{:?}",
        compilation.check_diagnostics()
    );
}

#[test]
fn generic_trait_fulfillments_preserve_exact_trusted_predicates() {
    for (predicate, valid) in [("live", true), ("other", false)] {
        let compilation = compilation(&format!(
            r#"
            trusted module app;
            trusted predicate live<T>(value: &T);
            trusted predicate other<T>(value: &T);
            trait Observer<T> {{
                static func observe(pos value: &T) requires(trusted live<T>(value));
            }}
            struct Watcher {{}}
            impl Watcher(Observer<u64>) {{
                static func observe(pos value: &u64) requires(trusted {predicate}<u64>(value)) {{}}
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

#[test]
fn trusted_caller_obligations_require_matching_evidence() {
    for (requirement, invocation, valid) in [
        ("", "use_owner(value);", false),
        ("", "trusted use_owner(value);", false),
        ("requires(trusted live(value))", "use_owner(value);", true),
        ("requires(trusted other(value))", "use_owner(value);", false),
    ] {
        let source = format!(
            r#"
            trusted module app;

            trusted predicate live(value: u64);
            trusted predicate other(value: u64);

            trusted func use_owner(pos value: u64)
                requires(trusted live(value)) {{}}

            func caller(pos value: u64) {requirement}
            {{
                {invocation}
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
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn trusted_producers_establish_live_result_witnesses() {
    let source = r#"
        trusted module app;

        struct Owner { mut epoch: u64; }

        trusted predicate live(owner: &Owner);

        trusted func owner() -> Owner
            ensures(trusted live(&result))
        {
            return { epoch = 1 };
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
fn mutation_invalidates_owner_witnesses() {
    let source = r#"
        trusted module app;

        struct Owner { mut epoch: u64; }
        trusted predicate live(owner: &Owner);

        trusted func owner() -> Owner ensures(trusted live(&result))
        {
            return { epoch = 1 };
        }

        trusted func observe(pos value: &Owner) requires(trusted live(value)) {}

        func caller()
        {
            let mut value = owner();
            value.epoch = 2;
            observe(&value);
        }
    "#;

    let compilation = compilation(source);

    bray_testing::assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(),
        DiagnosticKind::CheckingTrustedObligationNotProven,
    );
}
#[test]
fn safe_guarantees_cannot_fabricate_trusted_authority() {
    let compilation = compilation(
        r#"
        trusted module app;
        trusted predicate live(value: u64);
        func fabricated() -> u64 ensures(trusted live(result)) { return 1; }
        trusted func use_owner(pos value: u64) requires(trusted live(value)) {}
        func caller() { use_owner(fabricated()); }
    "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(),
        DiagnosticKind::CheckingTrustedObligationNotProven,
    );
}

#[test]
fn callable_conversion_cannot_erase_predicate_requirements() {
    let compilation = compilation(
        r#"
        trusted module app;
        trusted predicate live(value: u64);
        func use_owner(pos value: u64) requires(trusted live(value)) {}
        func caller() { let erased: func(pos value: u64) = use_owner; erased(1); }
    "#,
    );

    assert!(compilation.check_diagnostics().has_errors());
}

#[test]
fn callable_conversion_preserves_trusted_predicate_authority() {
    let compilation = compilation(
        r#"
        trusted module app;
        trusted predicate live(value: u64);
        callable Ordinary = func(pos value: u64) requires(live(value));
        func obligated(pos value: u64) requires(trusted live(value)) {}
        func caller() { let erased: Ordinary = obligated; }
    "#,
    );

    assert!(compilation.check_diagnostics().has_errors());
}

#[test]
fn raw_read_uses_declared_storage_obligations() {
    let compilation = compilation(
        r#"
        trusted module app;
        trusted func read_byte(pos pointer: RawPointer<u8>) -> u8
            requires(
                trusted core.memory.valid_read<u8>(pointer = pointer, count = 1),
                trusted core.memory.aligned_for<u8>(pointer = pointer),
                trusted core.memory.initialized_as<u8>(pointer = pointer),
            )
            uses(raw_memory)
        { return trusted core.memory.read<u8>(pointer); }
    "#,
    );

    assert!(
        !compilation.check_diagnostics().has_errors(),
        "{:?}",
        compilation.check_diagnostics()
    );
}

#[test]
fn ordinary_field_copy_does_not_outlive_its_witness() {
    let compilation = compilation(
        r#"
        trusted module app;
        struct Owner { pointer: u64; }
        trusted predicate live(value: u64);
        trusted func owner() -> Owner ensures(trusted live(result.pointer)) { return { pointer = 1 }; }
        trusted func observe(pos value: u64) requires(trusted live(value)) {}
        func caller() {
            let mut pointer: u64 = 0;
            {
                let value = owner();
                pointer = value.pointer;
            };
            observe(pointer);
        }
    "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(),
        DiagnosticKind::CheckingTrustedObligationNotProven,
    );
}

#[test]
fn moving_a_witness_preserves_the_new_owner() {
    let compilation = compilation(
        r#"
        trusted module app;
        struct Owner { epoch: u64; }
        trusted predicate live(value: &Owner);
        trusted func owner() -> Owner ensures(trusted live(&result)) { return { epoch = 1 }; }
        trusted func observe(pos value: &Owner) requires(trusted live(value)) {}
        func caller() { let value = owner(); let moved = value; observe(&moved); }
    "#,
    );

    assert!(
        !compilation.check_diagnostics().has_errors(),
        "{:?}",
        compilation.check_diagnostics()
    );
}

#[test]
fn named_callable_contracts_preserve_and_enforce_obligations() {
    for (enclosing, valid) in [("", false), ("requires(trusted live(value))", true)] {
        let source = format!(
            r#"
            trusted module app;
            trusted predicate live(value: u64);
            callable Action = func(pos value: u64) requires(trusted live(value));
            func observe(pos value: u64) requires(trusted live(value)) {{}}
            func caller(pos action: Action, pos value: u64) {enclosing} {{ action(value); }}
            func convert() -> Action {{ return observe; }}
        "#
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
fn implicit_copy_cannot_duplicate_a_live_witness() {
    let compilation = compilation(
        r#"
        trusted module app;
        @copy struct Owner { epoch: u64; }
        trusted predicate live(value: &Owner);
        trusted func owner() -> Owner ensures(trusted live(&result)) { return { epoch = 1 }; }
        func caller() { let value = owner(); let copied = value; }
    "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(),
        DiagnosticKind::CheckingTrustedWitnessTransferNotProven,
    );
}

#[test]
fn enclosing_owner_authority_cannot_be_duplicated_by_copying() {
    let compilation = compilation(
        r#"
        trusted module app;
        @copy struct Owner { epoch: u64; }
        trusted predicate live(value: &Owner);
        func observe(pos value: &Owner) requires(trusted live(value)) {}
        func caller(pos value: Owner) requires(trusted live(&value)) {
            let copied = value;
            observe(&copied);
        }
    "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(),
        DiagnosticKind::CheckingTrustedWitnessTransferNotProven,
    );
}

#[test]
fn a_trust_boundary_does_not_hide_wrapper_requirements() {
    for tail in ["", "observe(&value);"] {
        let compilation = compilation(&format!(
            r#"
            trusted module app;
            struct Owner {{ epoch: u64; }}
            trusted predicate live(value: &Owner);
            func observe(pos value: &Owner) requires(trusted live(value)) {{}}
            func caller() {{
                let value: Owner = {{ epoch = 1 }};
                trusted {{ observe(&value); observe(&value); }};
                {tail}
            }}
        "#
        ));

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            DiagnosticKind::CheckingTrustedObligationNotProven,
        );
    }
}

#[test]
fn an_acknowledged_producer_establishes_guarantees_when_its_obligation_is_exposed() {
    let compilation = compilation(
        r#"
        trusted module app;
        struct Owner { epoch: u64; }
        trusted predicate admitted(epoch: u64);
        trusted predicate live(value: &Owner);
        trusted func owner(pos epoch: u64) -> Owner
            requires(trusted admitted(epoch)) ensures(trusted live(&result))
            { return { epoch = epoch }; }
        func observe(pos value: &Owner) requires(trusted live(value)) {}
        func caller(pos epoch: u64) requires(trusted admitted(epoch)) {
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
fn unknown_observations_do_not_merge_unrelated_value_authority() {
    let compilation = compilation(
        r#"
        trusted module app;
        @copy struct Owner { epoch: u64; }
        trusted predicate live(value: &Owner);
        trusted func establish(pos value: &Owner) executes(pure, total)
            ensures(trusted live(value)) {}
        func caller() {
            let value: Owner = { epoch = 1 };
            let unrelated: Owner = { epoch = 2 };
            establish(&value);
            let copied = unrelated;
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
fn equal_scalar_values_do_not_share_borrowed_authority() {
    for (observed, valid) in [("value", true), ("unrelated", false)] {
        let compilation = compilation(&format!(
            r#"
        trusted module app;
        trusted predicate live(value: &u8);
        trusted func establish(pos value: &u8) executes(pure, total)
            ensures(trusted live(value)) {{}}
        func observe(pos value: &u8) requires(trusted live(value)) {{}}
        func caller() {{
            let value: u8 = 1;
            let unrelated: u8 = 1;
            establish(&value);
            observe(&{observed});
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

#[test]
fn safe_forwarders_preserve_existing_witnesses() {
    let compilation = compilation(
        r#"
        trusted module app;
        struct Owner { epoch: u64; }
        trusted predicate live(value: &Owner);
        trusted func owner() -> Owner ensures(trusted live(&result)) { return { epoch = 1 }; }
        func forward(pos value: Owner) -> Owner
            requires(trusted live(&value)) ensures(trusted live(&result)) { return value; }
        trusted func observe(pos value: &Owner) requires(trusted live(value)) {}
        func caller() { let value = forward(owner()); observe(&value); }
    "#,
    );

    assert!(
        !compilation.check_diagnostics().has_errors(),
        "{:?}",
        compilation.check_diagnostics()
    );
}

#[test]
fn ordinary_conditions_do_not_create_trusted_authority() {
    let compilation = compilation(
        r#"
        trusted module app;
        trusted predicate live(value: u64);
        func ordinary(pos value: u64) requires(live(value)) {}
        trusted func observe(pos value: u64) requires(trusted live(value)) {}
        func caller(pos value: u64) requires(live(value)) { observe(value); }
    "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(),
        DiagnosticKind::CheckingTrustedObligationNotProven,
    );
}

#[test]
fn ordinary_producer_requirements_guard_trusted_guarantees() {
    for (value, valid) in [("1", true), ("0", false), ("epoch", true)] {
        let compilation = compilation(&format!(
            r#"
            trusted module app;
            struct Owner {{ epoch: u64; }}
            trusted predicate live(value: &Owner);
            trusted func owner(pos epoch: u64) -> Owner requires(epoch == 1)
                ensures(trusted live(&result)) {{ return {{ epoch = epoch }}; }}
            trusted func observe(pos value: &Owner) requires(trusted live(value)) {{}}
            func caller(pos epoch: u64) {{ let value = owner({value}); observe(&value); }}
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

#[test]
fn runtime_checked_ordinary_requirements_establish_completion_guards() {
    let compilation = compilation(r#"
        trusted module app;
        struct Owner { epoch: u64; }
        trusted predicate live(value: &Owner);
        trusted func owner(pos epoch: u64) -> Owner
            requires(epoch == 1)
            when(epoch == 1) { ensures(trusted live(&result)) }
        { return { epoch = epoch }; }
        func observe(pos value: &Owner) requires(trusted live(value)) {}
        func caller(pos epoch: u64) { let value = owner(epoch); observe(&value); }
    "#);

    assert!(!compilation.check_diagnostics().has_errors(), "{:?}", compilation.check_diagnostics());
}

#[test]
fn ordinary_equalities_match_existing_trusted_conditions() {
    let compilation = compilation(
        r#"
        trusted module app;
        trusted predicate live(value: u64);
        trusted func observe(pos value: u64) requires(trusted live(value)) {}
        func caller(pos left: u64, pos right: u64)
            requires(left == right, trusted live(left)) { observe(right); }
    "#,
    );

    assert!(
        !compilation.check_diagnostics().has_errors(),
        "{:?}",
        compilation.check_diagnostics()
    );
}

#[test]
fn deferred_producers_require_live_entry_evidence_when_their_body_begins() {
    for (intervening, valid) in [
        ("", true),
        ("opaque();", false),
        ("if flag { opaque(); }", false),
        ("epoch = 2;", false),
    ] {
        let compilation = compilation(&format!(
            r#"
            trusted module app;
            struct Owner {{ epoch: u64; }}
            trusted predicate admitted(epoch: u64);
            trusted predicate live(value: &Owner);
            trusted async func owner(pos epoch: u64) -> Owner
                requires(trusted admitted(epoch)) ensures(trusted live(&result))
                {{ return {{ epoch = epoch }}; }}
            func observe(pos value: &Owner) requires(trusted live(value)) {{}}
            func opaque() {{}}
            async func caller(pos mut epoch: u64, flag: bool) requires(trusted admitted(epoch)) {{
                let future = owner(epoch);
                {intervening}
                let value = await future;
                observe(&value);
            }}
        "#
        ));

        assert_eq!(
            !compilation.check_diagnostics().has_errors(),
            valid,
            "{intervening}: {:?}",
            compilation.check_diagnostics()
        );
    }
}

#[test]
fn deferred_producers_publish_witnesses_after_await() {
    let compilation = compilation(
        r#"
        trusted module app;
        struct Owner { epoch: u64; }
        trusted predicate live(value: &Owner);
        trusted async func owner() -> Owner ensures(trusted live(&result)) { return { epoch = 1 }; }
        trusted func observe(pos value: &Owner) requires(trusted live(value)) {}
        async func caller() { let future = owner(); let value = await future; observe(&value); }
    "#,
    );

    assert!(
        !compilation.check_diagnostics().has_errors(),
        "{:?}",
        compilation.check_diagnostics()
    );
}

#[test]
fn nested_trust_in_requirements_cannot_hide_obligations() {
    let compilation = compilation(
        r#"
        trusted module app;
        trusted predicate live(value: u64);
        func observe(pos value: u64) requires((trusted live(value)) && true) {}
        func caller() { observe(1); }
    "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(),
        DiagnosticKind::CheckingTrustedObligationNotProven,
    );
}

#[test]
fn spare_capacity_bounds_prove_unsigned_sums_without_wrapping_subtraction() {
    for (ty, bounds, valid) in [
        ("u8", "initialized <= capacity", true),
        ("u8", "true", false),
        ("r64", "initialized <= capacity", false),
    ] {
        let source = format!(
            r#"
            module app;
            func append_length(initialized: {ty}, count: {ty}, capacity: {ty}) -> {ty}
                requires({bounds})
                when(true) {{ ensures(result <= capacity) }}
            {{
                let available = capacity - initialized;
                if count > available {{ panic("exceeds spare capacity"); }}
                return initialized + count;
            }}
        "#
        );

        let compilation = compilation(&source);

        assert_eq!(
            !compilation.check_diagnostics().has_errors(),
            valid,
            "{source}: {:?}",
            compilation.check_diagnostics()
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                compilation.check_diagnostics(),
                DiagnosticKind::CheckingExecutionGuaranteeNotProven,
            );
        }
    }

    let compilation = compilation(
        r#"
        trusted module app;
        struct Header { capacity: usize; }
        trusted func write_unknown_storage(pos header: &mut Header, pos destination: RawPointer<u8>) -> usize
            requires(trusted core.memory.valid_write<u8>(pointer = destination, count = 1))
            when(true) { ensures(result <= header.capacity) }
            uses(raw_memory, unchecked_init)
        {
            let capacity = header.capacity;
            trusted core.memory.write<u8>(destination, 0);
            return capacity;
        }
    "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(),
        DiagnosticKind::CheckingExecutionGuaranteeNotProven,
    );
}

#[test]
fn machine_arithmetic_totality_distinguishes_wrapping_from_trapping_operations() {
    for (operator, valid) in [
        ("+", true),
        ("-", true),
        ("*", true),
        ("/", false),
        ("%", false),
        ("<<", false),
    ] {
        for body in [
            format!("return left {operator} right;"),
            format!("let mut value: u64 = left; value {operator}= right; return value;"),
        ] {
            let source = format!(
                r#"
            module app;
            func arithmetic(left: u64, right: u64) -> u64 executes(total)
            {{ {body} }}
        "#
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
}

#[test]
fn checked_raw_primitives_complete_only_on_their_required_storage_domains() {
    for (requirements, valid) in [
        (
            "trusted core.memory.valid_read<u8>(pointer = pointer, count = 1), trusted core.memory.aligned_for<u8>(pointer = pointer), trusted core.memory.initialized_as<u8>(pointer = pointer)",
            true,
        ),
        (
            "trusted core.memory.valid_read<u8>(pointer = pointer, count = 1)",
            false,
        ),
    ] {
        let source = format!(
            r#"
            trusted module app;
            trusted func read_byte(pos pointer: RawPointer<u8>) -> u8
                requires({requirements}) executes(pure, total) uses(raw_memory)
            {{ return trusted core.memory.read<u8>(pointer); }}
        "#
        );

        let compilation = compilation(&source);

        assert_eq!(
            !compilation.check_diagnostics().has_errors(),
            valid,
            "{source}: {:?}",
            compilation.check_diagnostics()
        );
    }

    for (pointee, result, access, body) in [
        (
            "u8",
            "u8",
            "write",
            "trusted core.memory.write<u8>(pointer, 1); return 0;",
        ),
        (
            "Owner",
            "Owner",
            "read",
            "return trusted core.memory.read<Owner>(pointer);",
        ),
    ] {
        let source = format!(
            r#"
            trusted module app;
            struct Owner {{ value: u8; }}
            trusted func mutate(pos pointer: RawPointer<{pointee}>) -> {result}
                requires(
                    trusted core.memory.valid_{access}<{pointee}>(pointer = pointer, count = 1),
                    trusted core.memory.aligned_for<{pointee}>(pointer = pointer),
                    trusted core.memory.initialized_as<{pointee}>(pointer = pointer),
                )
                executes(pure, total) uses(raw_memory, unchecked_init)
            {{ {body} }}
        "#
        );

        let compilation = compilation(&source);

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            DiagnosticKind::CheckingExecutionGuaranteeNotProven,
        );
    }

    let compilation = compilation(
        r#"
        trusted module app;
        trusted func allocate() -> RawPointer<u8> executes(total) uses(manual_alloc)
        { return trusted core.memory.allocate(bytes = 1, align = 1); }
    "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(),
        DiagnosticKind::CheckingExecutionGuaranteeNotProven,
    );
}

#[test]
fn borrowed_completion_preserves_the_reached_owner_for_cleanup() {
    for (body, valid) in [
        ("observe(&owner);", true),
        ("let alias = &owner; observe(alias);", true),
        ("observe(&owner); observe(&owner);", true),
        ("let alias = &owner; observe(alias); observe(&owner);", true),
        ("observe_mut(&mut owner);", true),
        ("panic(\"stopped\");", true),
        ("if owner.epoch == 1 { panic(\"stopped\"); }", true),
        ("while owner.epoch == 1 { panic(\"stopped\"); }", true),
        ("{ let message = \"stopped\"; panic(message); }", true),
        (
            "{ let mut message = \"first\"; message = \"stopped\"; panic(message); }",
            true,
        ),
        (
            "let pending: Cleanup = { tag = 0 }; let mut index: usize = 0; index = 1; index += 1; { let released: Owner = owner; };",
            true,
        ),
        ("observe(&owner); owner.epoch = 2;", false),
        (
            "let alias = &owner; observe(alias); owner.epoch = 2;",
            false,
        ),
        ("forget(&owner);", false),
        ("may_fail(&owner);", false),
    ] {
        let source = format!(
            r#"
            trusted module app;
            struct Owner {{
                epoch: u64;
                trusted destruct() requires(trusted live(&self)) {{}}
            }}
            trusted predicate live(value: &Owner);
            struct Cleanup {{ tag: u8; destruct() {{}} }}
            trusted func make() -> Owner ensures(trusted live(&result)) {{ return {{ epoch = 1 }}; }}
            trusted func observe(pos value: &Owner)
                requires(trusted live(value)) ensures(trusted live(value)) executes(total) {{}}
            trusted func may_fail(pos value: &Owner)
                requires(trusted live(value)) ensures(trusted live(value)) {{}}
            trusted func observe_mut(pos value: &mut Owner)
                requires(trusted live(&value)) ensures(trusted live(&value)) executes(total) {{}}
            func forget(pos value: &Owner) {{}}
            func caller() {{ let mut owner = make(); {body} }}
        "#
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
fn cleanup_checks_destructor_against_finalizer_completion() {
    for (guarantee, execution, valid) in [
        ("ensures(trusted finished(&self))", "executes(total)", true),
        ("ensures(trusted finished(&self))", "", false),
        ("", "executes(total)", false),
    ] {
        let source = format!(
            r#"
            trusted module app;

            trusted predicate finished(value: &Owner);

            struct Owner {{
                epoch: u64;

                trusted finalize() {execution} {guarantee} {{}}

                trusted destruct() requires(trusted finished(&self)) {{}}
            }}

            func caller() {{
                let value: Owner = {{ epoch = 1 }};
            }}
        "#
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
fn represented_child_cleanup_checks_its_trusted_requirements() {
    let compilation = compilation(
        r#"
        trusted module app;
        trusted predicate live(value: &Child);
        struct Child {
            byte: u8;
            trusted destruct() requires(trusted live(&self)) {}
        }
        struct Parent { child: Child; }
        func caller() { let parent: Parent = { child = { byte = 1 } }; }
    "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(),
        DiagnosticKind::CheckingTrustedObligationNotProven,
    );
}

#[test]
fn recursive_owned_cleanup_does_not_require_infinite_type_expansion() {
    let compilation = compilation(
        r#"
        module app;
        struct Node { next: box[Heap] Node; }
        func dispose(pos node: Node) {}
    "#,
    );

    assert!(
        !compilation.check_diagnostics().has_errors(),
        "{:?}",
        compilation.check_diagnostics()
    );
}

#[test]
fn finalization_revokes_prior_destructor_evidence() {
    let compilation = compilation(
        r#"
        trusted module app;

        trusted predicate live(value: &Owner);

        struct Owner {
            epoch: u64;

            finalize() {}

            trusted destruct() requires(trusted live(&self)) {}
        }

        trusted func owner() -> Owner ensures(trusted live(&result)) {
            return { epoch = 1 };
        }

        func caller() {
            let value = owner();
        }
    "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(),
        DiagnosticKind::CheckingTrustedObligationNotProven,
    );
}

#[test]
fn anonymous_calls_preserve_trusted_requirements() {
    for (requirement, valid) in [("", false), ("requires(trusted live(value))", true)] {
        let source = format!(
            r#"
            trusted module app;

            trusted predicate live(value: u64);

            func caller(pos value: u64) {requirement} {{
                let action = lambda (pos value: u64)
                    requires(trusted live(value))
                {{}};

                action(value);
            }}
        "#
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
fn anonymous_bodies_receive_their_declared_trusted_requirements() {
    let compilation = compilation(
        r#"
        trusted module app;

        trusted predicate live(value: u64);

        trusted func observe(pos value: u64) requires(trusted live(value)) {}

        func caller(pos value: u64) requires(trusted live(value)) {
            let action = lambda (pos value: u64) requires(trusted live(value)) {
                observe(value);
            };

            action(value);
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
fn trusted_predicate_guarantees_preserve_execution_property_proofs() {
    for (body, valid) in [("return { epoch = 1 };", true), ("loop {}", false)] {
        let source = format!(
            r#"
            trusted module app;

            struct Owner {{ epoch: u64; }}

            trusted predicate live(value: &Owner);

            trusted func owner() -> Owner
                executes(pure, total)
                ensures(trusted live(&result))
            {{
                {body}
            }}
        "#
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
