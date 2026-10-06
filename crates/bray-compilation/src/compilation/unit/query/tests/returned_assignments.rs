use crate::test_support::{compilation, source_function_body_key};
use bray_diagnostics::DiagnosticKind;

#[test]
fn returned_assignments_preserve_disjoint_fields() {
    for returned in [
        "pair.first",
        "first(pair)",
        "tuple_first((pair.first, pair.second))",
    ] {
        let source = r#"
                module app;

                struct Pair
                {
                    first: &bool;
                    mut second: &bool;
                }

                func select(pos caller: &bool, pos temporary: &bool) -> &bool
                {
                    let mut pair = Pair { first = caller, second = caller };
                    pair.second = temporary;

                    return RETURNED;
                }

                func first(pos pair: Pair) -> &bool
                {
                    return pair.first;
                }

                func tuple_first(pos values: (&bool, &bool)) -> &bool
                {
                    let (value, _) = values;

                    return value;
                }

                func good(pos caller: &bool) -> &bool
                {
                    let local: bool = true;

                    return select(caller, &local);
                }
            "#;

        let compilation = compilation(&source.replace("RETURNED", returned));

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:?}",
            compilation.check_diagnostics()
        );
    }
}

#[test]
fn returned_assignments_through_calls_reject_local_storage() {
    let compilation = compilation(
        r#"
                module app;

                struct Holder
                {
                    mut value: &bool;
                }

                func read(pos holder: Holder) -> &bool
                {
                    return holder.value;
                }

                func bad(pos caller: &bool) -> &bool
                {
                    let local: bool = true;
                    let mut holder = Holder { value = caller };
                    holder.value = &local;

                    return read(holder);
                }
            "#,
    );

    let flow = compilation
        .storage_flow(source_function_body_key(&compilation, "bad"))
        .unwrap();

    bray_testing::assert_goal_state_diagnostic_kind(
        flow.diagnostics(),
        DiagnosticKind::CheckingEscapingStorageDependency,
    );
}

#[test]
fn returned_assignments_follow_parameter_array_and_alias_storage() {
    for body in [
        r#"
                func replace(pos mut result: Holder, pos second: &bool) -> Holder
                {
                    let owner =
                    {
                        yield &mut result;
                    };
                    owner.value = second;

                    return result;
                }
            "#,
        r#"
                func replace(pos mut result: Holder, pos second: &bool) -> Holder
                {
                    let mut other = Holder { value = second };
                    let owner = if false
                    {
                        yield &mut other;
                    }
                    else
                    {
                        yield &mut result;
                    };
                    owner.value = second;

                    return result;
                }
            "#,
        r#"
                func first(pos values: (&mut Holder,)) -> &mut Holder
                {
                    return values.0;
                }

                func replace(pos mut result: Holder, pos second: &bool) -> Holder
                {
                    let owner = first((&mut result,));
                    owner.value = second;

                    return result;
                }
            "#,
        r#"
                impl Holder
                {
                    mut func same() -> &mut Holder
                    {
                        return &mut self;
                    }
                }

                func replace(pos mut result: Holder, pos second: &bool) -> Holder
                {
                    let owner = result.same();
                    owner.value = second;

                    return result;
                }
            "#,
        r#"
                func same<T>(pos value: &mut T) -> &mut T
                {
                    return value;
                }

                func replace(pos mut result: Holder, pos second: &bool) -> Holder
                {
                    let owner = same(&mut result);
                    owner.value = second;

                    return result;
                }
            "#,
        r#"
                func same(pos value: &mut Holder) -> &mut Holder
                {
                    return value;
                }

                func replace(pos mut result: Holder, pos second: &bool) -> Holder
                {
                    same(&mut result).value = second;

                    return result;
                }
            "#,
        r#"
                func choose(pos first: &mut Holder, pos second: &mut Holder, pos condition: bool) -> &mut Holder
                {
                    if condition
                    {
                        return first;
                    }

                    return second;
                }

                func replace(pos mut result: Holder, pos second: &bool) -> Holder
                {
                    let mut other = Holder { value = second };
                    let owner = choose(&mut other, &mut result, false);
                    owner.value = second;

                    return result;
                }
            "#,
        r#"
                func replace(pos mut result: Holder, pos second: &bool) -> Holder
                {
                    result.value = second;

                    return result;
                }
            "#,
        r#"
                func replace(pos result: Holder, pos second: &bool) -> Holder
                {
                    let mut values = [result];

                    values[0] = Holder
                    {
                        value = second
                    };

                    return values[0];
                }
            "#,
        r#"
                func replace(pos mut result: Holder, pos second: &bool) -> Holder
                {
                    let owner = &mut result;
                    let alias = owner;

                    alias.value = second;

                    return result;
                }
            "#,
        r#"
                func replace(pos mut result: Holder, pos second: &bool) -> Holder
                {
                    result.value = second;

                    return Holder { value = read(result) };
                }

                func read(pos holder: Holder) -> &bool
                {
                    return holder.value;
                }
            "#,
        r#"
                func replace(pos mut result: Holder, pos second: &bool) -> Holder
                {
                    let mut owner = &mut result;
                    let mut alias = owner;
                    alias.value = second;

                    return result;
                }
            "#,
        r#"
                func replace(pos mut result: Holder, pos second: &bool) -> Holder
                {
                    let (owner,) = (&mut result,);
                    owner.value = second;

                    return result;
                }
            "#,
        r#"
                func replace(pos mut result: Holder, pos second: &bool) -> Holder
                {
                    let mut owner = &mut result;
                    owner = Holder { value = second };

                    return result;
                }
            "#,
        r#"
                func replace(pos mut result: Holder, pos second: &bool) -> Holder
                {
                    let [owner] = [&mut result];
                    owner.value = second;

                    return result;
                }
            "#,
        r#"
                func replace(pos mut result: Holder, pos second: &bool) -> Holder
                {
                    if let (owner,) = (&mut result,)
                    {
                        owner.value = second;
                    }

                    return result;
                }
            "#,
    ] {
        let source = format!(
            r#"
                    module app;

                    struct Holder
                    {{
                        mut value: &bool;
                    }}

                    {body}

                    func bad(pos caller: &bool) -> Holder
                    {{
                        let local: bool = true;

                        return replace(Holder {{ value = caller }}, &local);
                    }}
                "#
        );

        let caller_owned = compilation(&source.replace("&local", "caller"));

        assert!(
            caller_owned.check_diagnostics().is_empty(),
            "{source}\n{:?}",
            caller_owned.check_diagnostics()
        );

        let compilation = compilation(&source);

        let flow = compilation
            .storage_flow(source_function_body_key(&compilation, "bad"))
            .unwrap();

        assert!(
            flow.diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.kind()
                    == DiagnosticKind::CheckingEscapingStorageDependency),
            "{source}\n{:?}",
            flow.diagnostics()
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            flow.diagnostics(),
            DiagnosticKind::CheckingEscapingStorageDependency,
        );
    }
}

#[test]
fn returned_iteration_values_reject_local_storage() {
    let source = r#"
            module app;

            struct Items
            {
                value: &bool;
            }

            struct Cursor
            {
                value: &bool;
                mut available: bool;
            }

            impl &Items(Iterable)
            {
                type Element = &bool;
                type Cursor = Cursor;

                consume func iterate() -> Cursor
                {
                    return Cursor
                    {
                        value = self.value,
                        available = true
                    };
                }
            }

            impl Cursor(Iterator)
            {
                type Element = &bool;

                mut func next() -> (&bool)?
                {
                    if !self.available
                    {
                        return none;
                    }

                    self.available = false;
                    return self.value;
                }
            }

            func first(pos values: &Items) -> (&bool)?
            {
                for value in values
                {
                    return value;
                }

                return none;
            }

            func bad() -> (&bool)?
            {
                let local: bool = true;

                let items = Items
                {
                    value = &local
                };

                return first(&items);
            }
        "#;

    let compilation = compilation(source);
    let key = source_function_body_key(&compilation, "bad");
    let flow = compilation.storage_flow(key).unwrap();

    bray_testing::assert_goal_state_diagnostic_kind(
        flow.diagnostics(),
        DiagnosticKind::CheckingEscapingStorageDependency,
    );
}

#[test]
fn returned_propagation_respects_yield_regions_and_independent_errors() {
    for (body, result, input, argument, valid) in [
        (
            r#"
                    func forward(pos input: Result<bool, &bool>) -> Result<bool, &bool>
                    {
                        let result: Result<bool, &bool> =
                        {
                            let value = try input;

                            yield Ok(value);
                        };

                        return result;
                    }
                "#,
            "Result<bool, &bool>",
            "Result<bool, &bool>",
            "Error(&local)",
            false,
        ),
        (
            r#"
                    func forward(pos input: Result<(&bool), bool>) -> Result<bool, bool>
                    {
                        let value = try input;

                        let output: Result<bool, bool> = Ok(true);

                        return output;
                    }
                "#,
            "Result<bool, bool>",
            "Result<(&bool), bool>",
            "Ok(&local)",
            true,
        ),
    ] {
        let source = format!(
            r#"
                    module app;

                    {body}

                    func caller() -> {result}
                    {{
                        let local: bool = true;

                        let input: {input} = {argument};

                        return forward(input);
                    }}
                "#
        );

        let compilation = compilation(&source);

        if valid {
            assert!(
                compilation.check_diagnostics().is_empty(),
                "{:?}",
                compilation.check_diagnostics()
            );
        } else {
            let flow = compilation
                .storage_flow(source_function_body_key(&compilation, "caller"))
                .unwrap();

            bray_testing::assert_goal_state_diagnostic_kind(
                flow.diagnostics(),
                DiagnosticKind::CheckingEscapingStorageDependency,
            );
        }
    }
}

#[test]
fn returned_propagated_errors_reject_local_storage() {
    let source = r#"
            module app;

            func forward(pos input: Result<bool, &bool>) -> Result<bool, &bool>
            {
                let value = try input;

                return Ok(value);
            }

            func bad() -> Result<bool, &bool>
            {
                let local: bool = true;

                return forward(Error(&local));
            }
        "#;

    let compilation = compilation(source);
    let key = source_function_body_key(&compilation, "bad");
    let flow = compilation.storage_flow(key).unwrap();

    bray_testing::assert_goal_state_diagnostic_kind(
        flow.diagnostics(),
        DiagnosticKind::CheckingEscapingStorageDependency,
    );
}
