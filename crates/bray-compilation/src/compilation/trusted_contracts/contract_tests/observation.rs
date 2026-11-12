use bray_diagnostics::{DiagnosticKind};
use crate::test_support::{compilation};

#[test]
fn completed_value_requirements_preserve_snapshots_without_restoring_mutated_borrows() {
    for (parameters, requirement, preparation, count, valid) in [
        (
            "pos start: usize, pos end: usize, pos pointer: DataPointer<u8>",
            "count = end",
            "validate(start, end = end);",
            "end - start",
            true,
        ),
        (
            "pos record: &mut Record, pos pointer: DataPointer<u8>",
            "count = record.number",
            "change(record);",
            "record.number - 1",
            false,
        ),
    ] {
        let source = format!(
            r#"
            trusted module app;

            using core.memory;

            struct Record
            {{
                mut number: usize;
            }}

            func validate(pos start: usize, end: usize)
                requires(start <= end)
            {{
            }}

            func change(pos record: &mut Record)
                requires(record.number > 0)
            {{
                record.number = 0;
            }}

            func inspect(pos pointer: DataPointer<u8>, count: usize)
                requires(trusted core.memory.valid_read<u8>(pointer = pointer, count = count))
            {{
            }}

            func caller({parameters})
                requires(trusted core.memory.valid_read<u8>(pointer = pointer, {requirement}))
            {{
                {preparation}
                inspect(pointer, count = {count});
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
            assert!(
                diagnostics.iter().all(|diagnostic| diagnostic.kind()
                    == DiagnosticKind::CheckingTrustedObligationNotProven),
                "{source}: {diagnostics:#?}"
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
            "let mut values: [u8; 2] = [0, 0]; establish(&values[0]); values[index] = 1; observe(&values[0]);",
            false,
        ),
        (
            "let mut values = Indexed { value = 0 }; establish(&values.value); values[index] = 1; observe(&values.value);",
            false,
        ),
        (
            "let mut values = Indexed { value = 0 }; establish(&values.value); let other = &mut values; other.value = 1; observe(&values.value);",
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
            struct Indexed {{ mut value: u8; }}
            impl Indexed(MutableElementIndex<usize>) {{
                type Output = u8;
                mut func index(pos selector: &usize) -> &mut u8 {{ return &mut self.value; }}
            }}
            trusted func establish(pos value: &u8) executes(pure, total) ensures(trusted live(value)) {{}}
            trusted func establish_bool(pos value: &bool) executes(pure, total) ensures(trusted live_bool(value)) {{}}
            func observe(pos value: &u8) requires(trusted live(value)) {{}}
            func observe_bool(pos value: &bool) requires(trusted live_bool(value)) {{}}
            func caller(pos index: usize) {{ {body} }}
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
fn borrowed_results_do_not_invalidate_their_input_observations() {
    // A readonly borrow retains an empty marker's historical identity. An opaque
    // implementation can still change observable mutable contents.
    for marker_fields in ["", "mut active: bool;"] {
        for body in [
            "return obtain<T>(&lazy.cell, lazy.initializer);",
            "let output = obtain<T>(&lazy.cell, lazy.initializer); return output;",
            "_ = obtain<T>(&lazy.cell, lazy.initializer); return &lazy.cell.value;",
        ] {
            for opaque in [false, true] {
                for guarantee in [false, true] {
                    let declaration = if opaque {
                        "extern trusted func"
                    } else {
                        "func"
                    };

                    let implementation = if opaque {
                        ";"
                    } else {
                        "{ _ = initializer(); return &cell.value; }"
                    };

                    let completion = if guarantee {
                        "ensures(trusted valid(&cell.marker))"
                    } else {
                        ""
                    };

                    let source = format!(
                        r#"
                    trusted module app;
                    struct Marker {{ {marker_fields} }}
                    trusted predicate valid(owner: &Marker);
                    struct Cell<T> {{ marker: Marker; value: T; }}
                    struct Lazy<T> {{ cell: Cell<T>; initializer: func() -> T; }}
                    {declaration} obtain<T>(pos cell: &Cell<T>, pos initializer: func() -> T) -> &T
                        requires(trusted valid(&cell.marker)) {completion} {implementation}
                    func caller<T>(pos lazy: &Lazy<T>) -> &T
                        requires(trusted valid(&lazy.cell.marker))
                        ensures(trusted valid(&lazy.cell.marker))
                    {{ {body} }}
                    "#
                    );

                    let compilation = compilation(&source);

                    let valid = marker_fields.is_empty() || (opaque && guarantee);

                    assert_eq!(
                        !compilation.check_diagnostics().has_errors(),
                        valid,
                        "{source}: {:?}",
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
        }
    }
}

#[test]
fn consuming_reference_carriers_preserves_referent_evidence() {
    for (parameters, requirement, initialization) in [
        (
            "pos guard: Guard<T>",
            "requires(trusted acquired(&guard.acquisition))",
            "",
        ),
        ("pos input: &Cell<T>", "", "let guard = make<T>(input);"),
    ] {
        for dispose in ["guard.unlock();", "dispose<T>(guard);"] {
            let source = format!(
                r#"
                trusted module app;
                struct Marker {{}}
                trusted predicate valid(owner: &Marker);
                trusted predicate acquired(owner: &Marker);
                struct Cell<T> {{ marker: Marker; value: T; }}
                struct Guard<T> {{
                    cell: &Cell<T>;
                    acquisition: Marker;
                    trusted destruct()
                        requires(trusted acquired(&self.acquisition))
                        ensures(trusted valid(&self.cell.marker)) executes(total) {{}}
                    trusted func owner() -> &Cell<T>
                        requires(trusted acquired(&self.acquisition))
                        ensures(trusted valid(&result.marker)) executes(pure, total)
                    {{ return self.cell; }}
                    consume func unlock()
                        requires(trusted acquired(&self.acquisition)) executes(total) {{}}
                }}
                trusted func make<T>(pos cell: &Cell<T>) -> Guard<T>
                    ensures(trusted acquired(&result.acquisition))
                {{ return {{ cell = cell, acquisition = Marker {{}} }}; }}
                func dispose<T>(pos guard: Guard<T>)
                    requires(trusted acquired(&guard.acquisition)) {{}}
                func observe<T>(pos cell: &Cell<T>) requires(trusted valid(&cell.marker)) {{}}
                func caller<T>({parameters}) {requirement} {{
                    {initialization}
                    let cell = guard.owner();
                    {dispose}
                    observe(cell);
                }}
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
}

#[test]
fn nullable_borrow_descriptor_survives_unrelated_opaque_write() {
    let compilation = compilation(
        r#"
        trusted module app;

        using core.memory;
        trusted predicate destination_owned();
        trusted func destination_pointer(pos pointer: DataPointer<u8>) -> DataPointer<u8>
            requires(trusted destination_owned())
            ensures(
                trusted core.memory.valid_write<u8>(pointer = result, count = 1),
                trusted core.memory.aligned_for<u8>(pointer = result)
            )
            executes(pure, total)
        { return pointer; }
        struct Payload { mut value: u32; }
        func selected(pos value: &mut Payload) -> (&mut Payload)?
            executes(total)
        {
            let mut result: (&mut Payload)? = none;
            result = value;
            return result;
        }
        trusted func update(pos value: &mut Payload, pos destination: DataPointer<u8>)
            requires(trusted destination_owned())
            uses(raw_memory, unchecked_init)
        {
            let found = selected(value);
            if let ?payload = found
            {
                trusted core.memory.write<u8>(trusted destination_pointer(destination), 0);
                payload.value = 17;
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
