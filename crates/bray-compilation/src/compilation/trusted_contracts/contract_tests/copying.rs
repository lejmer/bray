use bray_diagnostics::{DiagnosticKind};
use crate::test_support::{compilation, source_function_body_key};

#[test]
fn copied_pointer_components_keep_storage_facts_without_owning_the_tuple() {
    let application = compilation(
        r#"
        trusted module app;
        using core.memory;
        struct BorrowedHolder
        {
            items: &[u8; 1];
        }
        func read_borrowed(pos value: BorrowedHolder) -> u8
        {
            return value.items[0];
        }
        func read_holder(pos items: &[u8; 1]) -> u8
        {
            return read_borrowed(
                BorrowedHolder
                {
                    items = items
                }
            );
        }
        struct Marker
        {
        }
        struct Access
        {
            marker: Marker;
            context: usize;
            pointer: DataPointer<u8>;
        }
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
        {
            return (left, right);
        }
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
                @copy struct Owner
                {{
                    epoch: u64;
                }}
                trusted predicate live(value: &Owner);
                trusted func owner(pos flag: bool) -> Owner {guarantee}
                {{
                    return
                    {{
                        epoch = 1
                    }};
                }}
                func caller(pos flag: bool)
                {{
                    let value = owner({argument});
                    {copy}
                }}
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
            @copy struct Owner
            {{
                epoch: u64;
            }}
            trusted predicate live(value: &mut Owner);
            trusted func initialize(pos flag: bool, pos value: &mut Owner) {guarantee}
            {{
            }}
            func caller(pos flag: bool)
            {{
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
        @copy struct Owner
        {
            epoch: u64;
        }
        trusted predicate live(value: &mut Owner);
        trusted func initialize(pos value: &mut Owner) -> u32
            ensures(result != 0 || trusted live(value))
        {
            return 0;
        }
        func caller()
        {
            let mut value: Owner = {
                epoch = 0
            };
            let status = initialize(&mut value);
            if status != 0 {
                let copy = value;
            }
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
fn copying_a_structural_owner_cannot_duplicate_nested_authority() {
    let compilation = compilation(
        r#"
        trusted module app;
        @copy struct Owner
        {
            epoch: u64;
        }
        @copy struct Holder
        {
            owner: Owner;
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
        func caller()
        {
            let holder: Holder =
            {
                owner = owner()
            };
            let copy = holder;
        }
    "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(),
        DiagnosticKind::CheckingTrustedWitnessTransferNotProven,
    );
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
            @copy struct Owner
            {{
                epoch: u64;
            }}
            trusted predicate live(value: &Owner);
            trusted func owner() -> Owner
                ensures(trusted live(&result))
            {{
                return
                {{
                    epoch = 1
                }};
            }}
            func caller()
            {{
                {body}
            }}
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
fn negative_entry_evidence_retains_witness_copy_obligations() {
    let compilation = compilation(
        r#"
        trusted module app;
        @copy struct Owner
        {
            epoch: u64;
        }
        trusted predicate dangerous(value: &Owner);
        func duplicate(pos value: Owner)
            requires(!(trusted dangerous(&value)))
        {
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
            @copy struct Owner
            {{
                epoch: u64;
            }}
            trusted predicate dangerous(value: &Owner);
            trusted func direct_owner() -> Owner
                ensures(!(trusted dangerous(&result)))
            {{
                return
                {{
                    epoch = 1
                }};
            }}
            trusted func owner(pos flag: bool) -> Owner
                ensures(flag || !(trusted dangerous(&result)))
            {{
                return
                {{
                    epoch = 1
                }};
            }}
            func caller()
            {{
                {body}
            }}
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
fn a_completion_status_can_be_copied_without_copying_the_output_authority() {
    let compilation = compilation(
        r#"
        trusted module app;
        struct Owner
        {
            epoch: u64;
        }
        trusted predicate live(value: &mut Owner);
        trusted func initialize(pos value: &mut Owner) -> u32
            ensures(result != 0 || trusted live(value))
        {
            return 0;
        }
        func observe(pos value: &mut Owner)
            requires(trusted live(value))
        {
        }
        func caller()
        {
            let mut value: Owner = {
                epoch = 1
            };
            let status = initialize(&mut value);
            let copied_status = status;
            if copied_status == 0 {
                observe(&mut value);
            }
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
fn ordinary_field_copy_does_not_outlive_its_witness() {
    let compilation = compilation(
        r#"
        trusted module app;
        struct Owner
        {
            pointer: u64;
        }
        trusted predicate live(value: u64);
        trusted func owner() -> Owner
            ensures(trusted live(result.pointer))
        {
            return
            {
                pointer = 1
            };
        }
        trusted func observe(pos value: u64)
            requires(trusted live(value))
        {
        }
        func caller()
        {
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
fn implicit_copy_cannot_duplicate_a_live_witness() {
    let compilation = compilation(
        r#"
        trusted module app;
        @copy struct Owner
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
        func caller()
        {
            let value = owner();
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
fn enclosing_owner_authority_cannot_be_duplicated_by_copying() {
    let compilation = compilation(
        r#"
        trusted module app;
        @copy struct Owner
        {
            epoch: u64;
        }
        trusted predicate live(value: &Owner);
        func observe(pos value: &Owner)
            requires(trusted live(value))
        {
        }
        func caller(pos value: Owner)
            requires(trusted live(&value))
        {
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
fn repeated_array_witness_copies_use_the_checked_extent() {
    for (count, extent, valid) in [
        ("ONE", 1, true),
        ("one()", 1, true),
        ("TWO", 2, false),
        ("two()", 2, false),
    ] {
        let compilation = compilation(&format!(
            r#"
            trusted module app;

            @copy
            struct Owner
            {{
                epoch: u64;
            }}

            trusted predicate live(value: &Owner);

            trusted func owner() -> Owner
                ensures(trusted live(&result))
            {{
                return Owner
                {{
                    epoch = 1,
                }};
            }}

            const ONE: usize = 1;

            const TWO: usize = 2;

            const func one() -> usize
            {{
                return ONE;
            }}

            const func two() -> usize
            {{
                return TWO;
            }}

            func caller()
            {{
                let values: [Owner; {extent}] = [owner(); {count}];
            }}
            "#
        ));

        assert_eq!(
            !compilation.check_diagnostics().has_errors(),
            valid,
            "{count}: {:#?}",
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
