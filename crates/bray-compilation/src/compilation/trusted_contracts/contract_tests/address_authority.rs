use bray_diagnostics::{DiagnosticKind};
use crate::test_support::{compilation};

#[test]
fn native_pointer_types_do_not_bound_unknown_effects() {
    for body in [
        "trusted touch_word(core.memory.address_of<u32>(&owner.value));",
        "trusted touch_marker(core.memory.address_of<Marker>(&owner.marker));",
        "let address = core.memory.address_of<Marker>(&owner.marker); trusted touch_marker(address); trusted opaque();",
    ] {
        let source = format!(
            r#"
            trusted module app;

            using core.memory;
            struct Marker
            {{
            }}
            struct Owner
            {{
                marker: Marker;
                value: u32;
            }}
            trusted predicate live(marker: &Marker);
            trusted func create() -> Owner
                ensures(trusted live(marker = &result.marker))
            {{
                return
                {{
                    marker = Marker {{}}, value = 1
                }};
            }}
            func accept(pos owner: Owner)
                requires(trusted live(marker = &owner.marker)) {{}}
            @abi(c)
            @link(name = "native")
            @symbol(name = "touch_word")
            extern trusted func touch_word(pos pointer: DataPointer<u32>)
                uses(foreign_call);
            @abi(c)
            @link(name = "native")
            @symbol(name = "touch_marker")
            extern trusted func touch_marker(pos pointer: DataPointer<Marker>)
                uses(foreign_call);
            @abi(c)
            @link(name = "native")
            @symbol(name = "opaque")
            extern trusted func opaque()
                uses(foreign_call);
            trusted func caller()
                uses(foreign_call)
            {{
                let owner = create();
                {body}
                accept(owner);
            }}
        "#
        );

        let options = crate::CompilationOptions::default().with_native_link_inputs([
            bray_symbols::NativeLinkRequirement::new(
                bray_base::NonEmptySharedStr::try_new("native")
                    .expect("fixture non empty shared str must be valid"),
                bray_symbols::NativeLinkKind::Dynamic,
            ),
        ]);

        let compilation = crate::test_support::compilation_with_options(&source, options);
        let diagnostics = compilation.check_diagnostics();

        bray_testing::assert_goal_state_diagnostic_kind(
            diagnostics,
            DiagnosticKind::CheckingTrustedObligationNotProven,
        );
    }
}

#[test]
fn borrowed_addresses_establish_live_access_conditions() {
    for nested in ["", "{ let scratch: u8 = 0; _ = scratch; };"] {
        let compilation = compilation(&format!(
            r#"
            trusted module app;

            using core.memory;
            trusted func caller() -> u8
                uses(raw_memory, unchecked_init)
            {{
                let mut value: u8 = 1;
                let pointer = core.memory.address_of_mut<u8>(&mut value);
                {nested}
                trusted core.memory.write<u8>(pointer, 2);
                return trusted core.memory.read<u8>(pointer);
            }}
            "#,
        ));

        assert!(
            !compilation.check_diagnostics().has_errors(),
            "{nested}: {:?}",
            compilation.check_diagnostics()
        );
    }
}

#[test]
fn writing_through_an_address_does_not_extend_its_owner_lifetime() {
    let compilation = compilation(
        r#"
        trusted module app;

        using core.memory;
        trusted func caller() -> u8
            uses(raw_memory, unchecked_init)
        {
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
fn copied_callable_addresses_preserve_validity_without_creating_it() {
    for (producer, valid) in [
        ("core.memory.function_address<Callback>(entry)", true),
        (
            "trusted core.memory.function.from_data<Callback>(trusted core.memory.function.to_data<Callback>(core.memory.function_address<Callback>(entry)))",
            true,
        ),
        (
            "trusted core.memory.function.reinterpret<Callback, Callback>(core.memory.function_address<Callback>(entry))",
            true,
        ),
        (
            "trusted core.memory.function.from_data<Callback>(core.memory.null<u8>())",
            false,
        ),
    ] {
        let compilation = compilation(&format!(
            r#"
            trusted module app;

            using core.memory;
            using core.memory.function;

            callable Callback = @abi(c) func();

            @abi(c)
            func entry()
            {{
            }}

            trusted func caller()
                uses(layout_reinterpret)
            {{
                let pointer: FunctionPointer<Callback> = {producer};
                let copied: FunctionPointer<Callback> = pointer;
                let callback: Callback = core.memory.callable_from_pointer<Callback>(copied);
                _ = callback;
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

        using core.memory;
        trusted predicate live(pointer: DataPointer<u8>);
        trusted func owner() -> DataPointer<u8>
            ensures(trusted live(result))
        {
            return core.memory.null<u8>();
        }
        func caller()
        {
            let pointer = owner();
            let copy = pointer;
        }
    "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(),
        DiagnosticKind::CheckingTrustedWitnessTransferNotProven,
    );

    let diagnostics = compilation.check_diagnostics();

    let failure = diagnostics.iter().flat_map(|diagnostic| diagnostic.args())
        .find_map(|argument| match argument.value() {
            bray_diagnostics::DiagnosticArgValue::WitnessTransfer(failure) => Some(failure),
            _ => None,
        }).expect("rejected copy retains its transfer cause");

    assert_eq!(failure.kind, bray_diagnostics::DiagnosticWitnessTransferKind::Copy);

    assert!(matches!(&failure.guarantee,
        bray_diagnostics::DiagnosticCondition::Application { path, arguments, .. }
            if path.last().is_some_and(|name| name == "live") && arguments.len() == 1),
        "{:?}", failure.guarantee);
}

#[test]
fn conditional_storage_guarantees_preserve_copyable_addresses() {
    for (ty, predicate) in [
        ("DataPointer<u8>", "core.memory.valid_read<u8>"),
        ("DevicePointer<u8>", "core.target.device_valid_read<u8>"),
    ] {
        for guard in ["flag", "enabled(flag)"] {
            let compilation = compilation(&format!(
                r#"
                trusted module app;

                using core.memory;
                using core.target;
                predicate enabled(flag: bool) = flag;
                extern trusted func address(pos flag: bool) -> {ty}
                    ensures(!{guard} || trusted {predicate}(pointer = result, count = 1));
                func caller()
                {{
                    let pointer = address(true);
                    let copy = pointer;
                }}
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
        extern trusted func owner() -> DevicePointer<u8>
            ensures(trusted live(result));
        func caller()
        {
            let pointer = owner();
            let copy = pointer;
        }
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

            using core.memory;
            struct Owner
            {{
                byte: Uninit<u8>;
            }}
            trusted func address_view(pos owner: &Owner) -> DataPointer<u8>
                executes(pure, total)
                ensures(trusted core.memory.valid_read<u8>(pointer = result, count = 1))
            {{
                return core.memory.uninit_pointer<u8>(&owner.byte);
            }}
            func observe(pos pointer: DataPointer<u8>)
                requires(trusted core.memory.valid_read<u8>(pointer = pointer, count = 1)) {{}}
            func caller()
            {{
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
fn returned_addresses_preserve_only_the_checked_input_authority() {
    for (returned, valid) in [
        ("pointer", true),
        ("other", false),
        ("core.memory.null<u8>()", false),
    ] {
        let source = format!(
            r#"
            trusted module app;

            using core.memory;
            func forward(pos pointer: DataPointer<u8>, pos other: DataPointer<u8>) -> DataPointer<u8>
                executes(pure, total)
            {{
                return {returned};
            }}
            trusted func caller() -> u8
                uses(raw_memory)
            {{
                let value: u8 = 1;
                let pointer = core.memory.address_of<u8>(&value);
                let forwarded = forward(pointer, core.memory.null<u8>());
                return core.memory.read<u8>(forwarded);
            }}
            "#,
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
fn returned_address_authority_expires_with_its_original_owner() {
    let compilation = compilation(
        r#"
        trusted module app;

        using core.memory;
        func forward(pos pointer: DataPointer<u8>) -> DataPointer<u8>
            executes(pure, total)
        {
            return pointer;
        }
        trusted func caller() -> u8
            uses(raw_memory)
        {
            let mut forwarded = core.memory.null<u8>();
            {
                let value: u8 = 1;
                forwarded = forward(core.memory.address_of<u8>(&value));
            };
            return core.memory.read<u8>(forwarded);
        }
    "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(),
        DiagnosticKind::CheckingTrustedObligationNotProven,
    );
}

#[test]
fn address_formation_keeps_the_value_owner_distinct_from_the_address_witness() {
    let compilation = compilation(
        r#"
        trusted module app;

        using core.memory;
        struct Bytes
        {
            first: u8;
        }
        func observe(pos pointer: DataPointer<Bytes>)
            requires(trusted core.memory.valid_read<Bytes>(pointer = pointer, count = 1)) {}
        func caller() -> u8
        {
            let owner = Bytes { first = 1 };
            let pointer = core.memory.address_of<Bytes>(&owner);
            let copied = owner.first;
            observe(pointer);
            return copied;
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
fn returned_borrow_alias_keeps_address_authority_tied_to_physical_storage() {
    for (body, valid) in [
        (
            "let value: u8 = 1; let pointer = locate(&value); return core.memory.read<u8>(pointer);",
            true,
        ),
        (
            "let mut pointer = core.memory.null<u8>(); { let value: u8 = 1; pointer = locate(&value); }; return core.memory.read<u8>(pointer);",
            false,
        ),
        (
            "let original: u8 = 1; let mut pointer = core.memory.null<u8>(); { let copy: u8 = original; pointer = locate(&copy); }; return core.memory.read<u8>(pointer);",
            false,
        ),
    ] {
        let source = format!(
            r#"
            trusted module app;

            using core.memory;
            func identity(pos value: &u8) -> &u8
            {{
                return value;
            }}
            func locate(pos value: &u8) -> DataPointer<u8>
                ensures(
                    trusted core.memory.valid_read<u8>(pointer = result, count = 1),
                    trusted core.memory.initialized_as<u8>(pointer = result),
                    trusted core.memory.aligned_for<u8>(pointer = result),
                )
            {{
                let alias: &u8 = identity(value);
                return core.memory.address_of<u8>(alias);
            }}
            trusted func caller() -> u8
                uses(raw_memory)
            {{
                {body}
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
fn opaque_effects_preserve_unexposed_pointer_identity_without_preserving_authority() {
    for (exposure, preserve, valid) in [
        ("", true, true),
        ("", false, false),
        (
            "let escaped = core.memory.address_of_mut<DataPointer<u8>>(&mut pointer); publish(escaped);",
            true,
            false,
        ),
    ] {
        let postcondition = if preserve {
            "ensures(trusted core.memory.valid_read<u8>(pointer = value, count = 1), trusted core.memory.valid_write<u8>(pointer = value, count = 1), trusted core.memory.aligned_for<u8>(pointer = value), trusted core.memory.initialized_as<u8>(pointer = value))"
        } else {
            ""
        };

        let source = format!(
            r#"
            trusted module app;

            using core.memory;
            trusted extern func renew(pos value: DataPointer<u8>) {postcondition};
            extern func publish(pos pointer: DataPointer<DataPointer<u8>>);
            trusted func run(pos input: DataPointer<u8>)
                uses(raw_memory, unchecked_init)
            {{
                let mut pointer = input;
                {exposure}
                trusted renew(pointer);
                let contents = core.memory.read<u8>(pointer);
                core.memory.write<u8>(pointer, contents);
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
fn content_writes_keep_spatial_facts_about_copied_pointer_values() {
    for (write, valid) in [
        (
            "core.memory.write<DataPointer<u8>>(slot, replacement);",
            true,
        ),
        ("opaque();", false),
    ] {
        let source = format!(
            r#"
            trusted module app;

            using core.memory;
            extern func opaque();
            struct Pointer
            {{
                value: DataPointer<u8>;
            }}
            trusted func run(pos owner: &Pointer, pos slot: DataPointer<DataPointer<u8>>, pos replacement: DataPointer<u8>)
                requires(
                    trusted core.memory.valid_write<u8>(pointer = owner.value, count = 1),
                    trusted core.memory.aligned_for<u8>(pointer = owner.value),
                    trusted core.memory.valid_write<DataPointer<u8>>(pointer = slot, count = 1),
                    trusted core.memory.aligned_for<DataPointer<u8>>(pointer = slot),
                ) uses(raw_memory, unchecked_init)
            {{
                let captured = owner.value;
                {write}
                core.memory.write<u8>(captured, 1);
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
fn conditional_calls_rejoin_preserved_borrowed_pointer_records() {
    for (guarantee, observation, valid) in [
        (
            "ensures(trusted selected(cursor))",
            "observe(cursor);",
            true,
        ),
        ("", "observe(cursor);", false),
        (
            "ensures(trusted selected(cursor))",
            "observe(other);",
            false,
        ),
        (
            "ensures(trusted selected(cursor))",
            "let rebuilt: Cursor = { root = cursor.root, member = cursor.member }; observe(&rebuilt);",
            false,
        ),
    ] {
        let compilation = compilation(&format!(
            r#"
            trusted module app;
            struct Cursor
            {{
                root: DataPointer<u8>;
                member: DataPointer<u8>;
            }}
            trusted predicate selected(cursor: &Cursor);
            extern trusted func update(pos cursor: &Cursor)
                requires(trusted selected(cursor)) {guarantee};
            func observe(pos cursor: &Cursor)
                requires(trusted selected(cursor))
            {{
            }}
            func caller(pos cursor: &Cursor, pos other: &Cursor, pos change: bool)
                requires(trusted selected(cursor))
            {{
                if change {{ trusted update(cursor); }}
                {observation}
            }}
            "#
        ));

        assert_eq!(
            !compilation.check_diagnostics().has_errors(),
            valid,
            "{guarantee} / {observation}: {:?}",
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
