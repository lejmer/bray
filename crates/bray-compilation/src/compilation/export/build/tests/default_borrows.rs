use bray_symbols::CallableParameterDefaultValue;

use crate::test_support::source_function_body_key;

use super::fixtures::{compilation, execution_consumer};

#[test]
fn runtime_defaults_reject_provider_owned_borrows_in_every_declaration_kind() {
    let declarations = r#"
        module app;

        struct Flag
        {
            value: bool;
        }

        struct Holder
        {
            value: &Flag;
        }

        func make_flag() -> Flag
        {
            return Flag
            {
                value = true,
            };
        }

        func holder(pos value: &Flag) -> Holder
        {
            return Holder
            {
                value = value,
            };
        }
    "#;

    for declaration in [
        r#"
            func bad(value: &Flag = &make_flag())
            {
            }
        "#,
        r#"
            struct Bad
            {
                value: &Flag = &make_flag();
            }
        "#,
        r#"
            union Bad
            {
                Value(value: &Flag = &make_flag());
            }
        "#,
        r#"
            struct Bad
            {
                value: Holder = holder(&make_flag());
            }
        "#,
    ] {
        let compilation = compilation(&format!("{declarations}\n{declaration}"));
        let diagnostics = compilation.check_diagnostics();

        bray_testing::assert_goal_state_diagnostic_kind(
            &diagnostics,
            bray_diagnostics::DiagnosticKind::CheckingEscapingStorageDependency,
        );
    }
}

#[test]
fn runtime_default_storage_cannot_escape_source_or_imported_calls() {
    let provider_source = r#"
        module api;

        @copy
        struct Holder
        {
            value: &bool;
        }

        func choose(pos first: bool, second: &bool = &first) -> &bool
        {
            return second;
        }

        func choose_generic<T>(pos first: T, second: &T = &first) -> &T
        {
            return second;
        }

        func holder(pos value: &bool) -> Holder
        {
            return Holder
            {
                value = value,
            };
        }

        func wrap(pos first: bool, second: Holder = holder(&first)) -> Holder
        {
            return second;
        }

        func chain(pos first: bool, second: &bool = &first, third: &bool = second) -> &bool
        {
            return third;
        }

        func forward(pos first: &bool, second: &bool = first, third: &bool = second) -> &bool
        {
            return third;
        }

        func forward_holder(pos first: Holder, second: Holder = first) -> Holder
        {
            return second;
        }

        struct Flag
        {
            value: bool;
        }

        @copy
        struct Wrapper
        {
            flag: &Flag;
        }

        impl Wrapper
        {
            consume func selected(second: &bool = &self.flag.value) -> &bool
            {
                return second;
            }

            consume func slot(second: &&Flag = &self.flag) -> &&Flag
            {
                return second;
            }
        }

        func borrow_wrapped(pos first: Wrapper, second: &bool = &first.flag.value) -> &bool
        {
            return second;
        }

        func borrow_wrapped_slot(pos first: Wrapper, second: &&Flag = &first.flag) -> &&Flag
        {
            return second;
        }

        func read_default(pos first: Flag, second: &Flag = &first) -> bool
        {
            return second.value;
        }

        func borrow_field(pos first: &Flag, second: &bool = &first.value) -> &bool
        {
            return second;
        }

        func borrow_slot(pos first: &bool, second: &&bool = &first) -> &&bool
        {
            return second;
        }

        func borrow_index(pos first: &[bool; 1], second: &bool = &first[0]) -> &bool
        {
            return second;
        }

        func borrow_slice(pos first: &[bool; 1], second: &[bool] = &first[..]) -> &[bool]
        {
            return second;
        }

        func borrow_owned_index(pos first: [bool; 1], second: &bool = &first[0]) -> &bool
        {
            return second;
        }

        func borrow_array_slot(
            pos first: &[bool; 1],
            second: &&[bool; 1] = &first,
        ) -> &&[bool; 1]
        {
            return second;
        }

        func observe(pos first: bool, second: &bool = &first)
        {
        }

        func ignore_borrow(pos first: &bool) -> bool
        {
            return true;
        }

        func evaluate(pos first: bool, second: bool = ignore_borrow(&first)) -> bool
        {
            return second;
        }
    "#;

    let provider = compilation(provider_source);

    assert!(
        !provider.check_diagnostics().has_errors(),
        "{:?}",
        provider.check_diagnostics()
    );

    for (result, invocation, valid) in [
        ("&bool", "choose(first)", false),
        ("&bool", "choose_generic<bool>(first)", false),
        ("Holder", "wrap(first)", false),
        ("&bool", "chain(first)", false),
        ("&bool", "forward(caller)", true),
        (
            "Holder",
            r#"forward_holder(Holder
                    {
                        value = caller,
                    })"#,
            true,
        ),
        ("&bool", "borrow_field(owner)", true),
        ("&bool", "wrapped.selected()", true),
        ("&&Flag", "wrapped.slot()", false),
        (
            "&bool",
            r#"borrow_wrapped(Wrapper
                    {
                        flag = owner,
                    })"#,
            true,
        ),
        (
            "&&Flag",
            r#"borrow_wrapped_slot(Wrapper
                    {
                        flag = owner,
                    })"#,
            false,
        ),
        ("&&bool", "borrow_slot(caller)", false),
        ("&bool", "borrow_index(array)", true),
        ("&[bool]", "borrow_slice(array)", true),
        ("&bool", "borrow_owned_index(owned_array)", false),
        ("&&[bool; 1]", "borrow_array_slot(array)", false),
        ("&bool", "choose(first, second = caller)", true),
        ("bool", "evaluate(first)", true),
        (
            "bool",
            r#"read_default(Flag
                    {
                        value = first,
                    })"#,
            true,
        ),
    ] {
        let body = format!(
            r#"
                func check(
                    pos caller: &bool,
                    pos owner: &Flag,
                    pos array: &[bool; 1],
                    pos owned_array: [bool; 1],
                ) -> {result}
                {{
                    let first: bool = true;
                    let wrapped = Wrapper
                    {{
                        flag = owner,
                    }};

                    observe(first);

                    return {invocation};
                }}
            "#
        );

        let source = compilation(&format!("{provider_source}\n{body}"));

        let imported_invocation = if invocation.starts_with("wrapped.") {
            invocation.to_owned()
        } else {
            format!("example.package.api.{invocation}")
        };

        let imported_body = body
            .replace(
                &format!("return {invocation};"),
                &format!("return {imported_invocation};"),
            )
            .replace("Flag", "example.package.api.Flag")
            .replace("Holder", "example.package.api.Holder")
            .replace("Wrapper", "example.package.api.Wrapper")
            .replace("observe(first)", "example.package.api.observe(first)");

        let imported = execution_consumer(
            &provider,
            &format!(
                r#"
                    module app;

                    using example.package.api;

                    {imported_body}
                "#
            ),
        );

        for consumer in [&source, &imported] {
            let diagnostics = consumer.check_diagnostics();

            if valid {
                assert!(!diagnostics.has_errors(), "{invocation}: {diagnostics:?}");
            } else {
                bray_testing::assert_goal_state_diagnostic_kind(
                    &diagnostics,
                    bray_diagnostics::DiagnosticKind::CheckingEscapingStorageDependency,
                );
            }
        }
    }
}

#[test]
fn runtime_defaults_reborrow_array_targets() {
    for (input, result, expression) in [
        ("&[bool; 1]", "&bool", "&first[0]"),
        ("&[bool; 1]", "&[bool]", "&first[..]"),
        ("&[bool; 1]", "&[bool]", "&first[0..]"),
        ("&[bool; 1]", "&[bool]", "&first[..1]"),
        ("&[bool; 1]", "&[bool]", "&first[0..1]"),
        ("&[bool]", "&bool", "&first[0]"),
        ("&[bool]", "&[bool]", "&first[0..1]"),
        ("&mut [bool; 1]", "&mut bool", "&mut first[0]"),
        ("&mut [bool; 1]", "&mut [bool]", "&mut first[..]"),
    ] {
        let declaration = format!(
            r#"
                func choose(pos first: {input}, second: {result} = {expression}) -> {result}
                {{
                    return second;
                }}
            "#
        );

        let body = format!(
            r#"
                func check(pos caller: {input}) -> {result}
                {{
                    return choose(caller);
                }}
            "#
        );

        let provider = compilation(&format!("module api;\n{declaration}"));
        let source = compilation(&format!("module app;\n{declaration}\n{body}"));

        assert!(
            !provider.check_diagnostics().has_errors(),
            "{expression}: {:?}",
            provider.check_diagnostics()
        );

        let imported = execution_consumer(
            &provider,
            &format!(
                "module app;\nusing example.package.api;\n{}",
                body.replace("choose(caller)", "example.package.api.choose(caller)")
            ),
        );

        for consumer in [&source, &imported] {
            let diagnostics = consumer.check_diagnostics();

            assert!(!diagnostics.has_errors(), "{expression}: {diagnostics:?}");
        }
    }
}

#[test]
fn runtime_defaults_preserve_permanent_literal_borrows() {
    let provider = compilation(
        r#"
        module api;

        func text(value: &string = &"default text") -> &string
        {
            return value;
        }

        struct Text
        {
            value: &string = &"field text";
        }

        union Choice
        {
            Text(value: &string = &"payload text");
        }
    "#,
    );

    assert!(
        !provider.check_diagnostics().has_errors(),
        "{:?}",
        provider.check_diagnostics()
    );

    let consumer = execution_consumer(
        &provider,
        r#"
        module app;

        using example.package.api;

        func read() -> &string
        {
            return example.package.api.text();
        }
    "#,
    );

    assert!(
        !consumer.check_diagnostics().has_errors(),
        "{:?}",
        consumer.check_diagnostics()
    );
}

#[test]
fn imported_runtime_default_keeps_its_borrowed_result_type() {
    let provider = compilation(
        r#"
        module api;

        func observe(first: bool, second: &bool = &first)
        {
        }
    "#,
    );

    let consumer = execution_consumer(
        &provider,
        r#"
        module app;

        using example.package.api;

        func check()
        {
            example.package.api.observe(first = true);
        }
    "#,
    );

    let semantics = consumer
        .expression_semantics_with_cancellation(
            source_function_body_key(&consumer, "check"),
            &consumer.state.cancellation,
        )
        .unwrap_or_else(|error| panic!("consumer selections must publish: {error:?}"));

    let parameter = semantics
        .result()
        .value()
        .selections()
        .entries()
        .iter()
        .find_map(|entry| {
            let bray_bound_tree::SemanticSelection::Call(call) = entry.selection() else {
                return None;
            };

            call.arguments().iter().find_map(|argument| match argument {
                bray_bound_tree::SelectedArgument::Default { parameter, .. } => Some(*parameter),
                _ => None,
            })
        })
        .unwrap_or_else(|| panic!("imported default must be selected"));

    let default = consumer
        .callable_parameter_default(parameter)
        .unwrap_or_else(|error| panic!("imported default must resolve: {error:?}"));

    let CallableParameterDefaultValue::Valid(surface) = default.value().value() else {
        panic!("imported default must be valid");
    };

    let store = consumer
        .semantic_value_store()
        .unwrap_or_else(|error| panic!("semantic values must exist: {error:?}"));

    let result = store.type_data(surface.result());

    assert!(matches!(
        result.as_ref(),
        bray_symbols::TypeData::Borrow { .. }
    ));
}

#[test]
fn mutable_default_results_allow_field_writes() {
    let declarations = r#"
        struct Item { mut value: bool; mut func clear() { self.value = false; } }
        struct Readonly { value: bool; }
        struct Values { mut item: Item; }
        impl Values(MutableElementIndex<i32>) {
            type Output = Item;
            mut func index(pos selector: &i32) -> &mut Item { return &mut self.item; }
        }
        func direct(pos value: &mut Item) -> &mut Item { return value; }
        func defaulted(pos value: &mut Item, selected: &mut Item = value) -> &mut Item { return selected; }
        func indexed(pos values: &mut Values, pos selector: i32, selected: &mut Item = &mut values[selector]) -> &mut Item { return selected; }
    "#;

    let provider = compilation(&format!("module api;\n{declarations}"));
    let mut failures = Vec::new();

    for (body, mutable) in [
        (
            "func check(pos value: &mut Item) { value.value = false; }",
            true,
        ),
        (
            "func check(pos value: &mut Item) { direct(value).value = false; }",
            true,
        ),
        (
            "func check(pos value: &mut Item) { defaulted(value).value = false; }",
            true,
        ),
        (
            "func check(pos values: &mut Values) { indexed(values, 0).value = false; }",
            true,
        ),
        (
            "func check(pos values: &mut Values) { values.item.clear(); }",
            true,
        ),
        (
            "func check(pos value: &mut Readonly) { value.value = false; }",
            false,
        ),
    ] {
        let source = compilation(&format!("module api;\n{declarations}\n{body}"));

        let imported_body = body
            .replace("Item", "example.package.api.Item")
            .replace("Values", "example.package.api.Values")
            .replace("Readonly", "example.package.api.Readonly")
            .replace("direct(", "example.package.api.direct(")
            .replace("defaulted(", "example.package.api.defaulted(")
            .replace("indexed(", "example.package.api.indexed(");

        let imported = execution_consumer(
            &provider,
            &format!("module app;\nusing example.package.api;\n{imported_body}"),
        );

        for (kind, consumer) in [("source", &source), ("import", &imported)] {
            let diagnostics = consumer.check_diagnostics();

            if mutable {
                if diagnostics.has_errors() {
                    failures.push(format!("{kind} {body}: {diagnostics:?}"));
                }
            } else {
                bray_testing::assert_goal_state_diagnostic_kind(
                    &diagnostics,
                    bray_diagnostics::DiagnosticKind::CheckingMissingMutationAuthority,
                );
            }
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
