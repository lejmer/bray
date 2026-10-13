use crate::test_support::{compilation, source_function_body_key};
use bray_diagnostics::DiagnosticKind;
use bray_testing::assert_goal_state_diagnostic_kind;

#[test]
fn returned_nullable_sequence_borrows_preserve_the_input_storage() {
    for extent in ["", "; 1"] {
        for borrow in ["", "mut "] {
            let source = format!(
                r#"
                module app;

                func element<T>(pos items: &{borrow}[(T?){extent}], index: usize) -> &{borrow}T
                {{
                    return match items[index]
                    {{
                        case ?value {{ yield &{borrow}value; }}
                        case none {{ panic("empty slot"); }}
                    }};
                }}

                func caller<T>(pos items: &{borrow}[(T?){extent}]) -> &{borrow}T
                {{
                    return element<T>(items, index = 0);
                }}
                "#
            );

            let compilation = compilation(&source);
            let key = source_function_body_key(&compilation, "caller");

            let flow = compilation
                .storage_flow(key)
                .expect("returned nullable element retains its caller-owned sequence");

            assert!(flow.diagnostics().is_empty(), "{source}\n{flow:#?}");
        }
    }
}

#[test]
fn returned_nullable_sequence_borrows_reject_local_storage() {
    let compilation = compilation(
        r#"
        module app;

        func element<T>(pos items: &[(T?); 1], index: usize) -> &T
        {
            return match items[index]
            {
                case ?value { yield &value; }
                case none { panic("empty slot"); }
            };
        }

        func bad(pos item: i32?) -> &i32
        {
            let items: [(i32?); 1] = [item];

            return element<i32>(&items, index = 0);
        }
        "#,
    );

    let key = source_function_body_key(&compilation, "bad");

    let types = compilation
        .expression_types(key.clone())
        .expect("local nullable sequence escape has valid expression types");

    assert!(types.diagnostics().is_empty(), "{types:#?}");

    let flow = compilation
        .storage_flow(key)
        .expect("local nullable sequence escape publishes its diagnostic");

    assert_goal_state_diagnostic_kind(
        flow.diagnostics(),
        DiagnosticKind::CheckingEscapingStorageDependency,
    );
}

#[test]
fn returned_values_reject_local_storage_dependencies() {
    for source in [
        r#"
            module app;
            struct Holder { mut value: &bool; }
            func set(pos target: &mut Holder, pos value: &bool) { target.value = value; }
            func bad(pos caller: &bool, pos condition: bool) -> Holder {
                let local = true;
                let mut result: Holder = { value = caller };
                if condition { set(&mut result, &local); }
                return result;
            }
        "#,
        r#"
            module app;
            struct Holder { mut value: &bool; }
            func set(pos target: &mut Holder, pos value: &bool) { target.value = value; }
            func replace(pos first: &bool, pos second: &bool) -> Holder {
                let mut result: Holder = { value = first };
                set(&mut result, second);
                return result;
            }
            func bad(pos caller: &bool) -> Holder {
                let local = true;
                return replace(caller, &local);
            }
        "#,
        r#"
                module app;

                struct Holder
                {
                    mut value: &bool;
                }

                func replace(pos first: &bool, pos second: &bool) -> Holder
                {
                    let mut result = Holder { value = first };
                    result.value = second;

                    return result;
                }

                func bad(pos caller: &bool) -> Holder
                {
                    let local: bool = true;

                    return replace(caller, &local);
                }
            "#,
        r#"
                module app;

                func bad() -> &bool
                {
                    let value: bool = true;

                    return &value;
                }
            "#,
        r#"
                module app;

                func bad() -> &mut bool
                {
                    let mut value: bool = true;

                    return &mut value;
                }
            "#,
        r#"
                module app;

                func bad() -> &bool
                {
                    let value: bool = true;
                    let alias = &value;

                    return alias;
                }
            "#,
        r#"
                module app;

                struct Holder
                {
                    value: &bool;
                }

                func bad() -> Holder
                {
                    let value: bool = true;

                    return Holder
                    {
                        value = &value
                    };
                }
            "#,
        r#"
                module app;

                func same<T>(pos value: &T) -> &T
                {
                    return value;
                }

                func bad() -> &bool
                {
                    let value: bool = true;

                    return same(&value);
                }
            "#,
        r#"
                module app;

                func same<T>(pos value: &mut T) -> &mut T
                {
                    return value;
                }

                func bad() -> &mut bool
                {
                    let mut value: bool = true;

                    return same(&mut value);
                }
            "#,
        r#"
                module app;

                func same<T>(pos value: T) -> T
                {
                    return value;
                }

                struct Holder
                {
                    value: &bool;
                }

                func bad() -> Holder
                {
                    let value: bool = true;

                    return same(
                        Holder
                        {
                            value = &value
                        }
                    );
                }
            "#,
        r#"
                module app;

                func same<T>(pos value: &T) -> &T
                {
                    return
                    {
                        yield value;
                    };
                }

                func bad() -> &bool
                {
                    let value: bool = true;

                    return same(&value);
                }
            "#,
        r#"
                module app;

                func project<T>(pos value: &box T) -> &T
                {
                    return match value
                    {
                        case box(inner)
                        {
                            yield &inner;
                        }
                    };
                }

                func bad() -> &bool
                {
                    let value = box(true);

                    return project(&value);
                }
            "#,
        r#"
                module app;

                struct Holder
                {
                    value: bool;
                }

                func same(pos value: &Holder) -> &Holder
                {
                    return value;
                }

                func bad() -> &bool
                {
                    let value = Holder
                    {
                        value = true
                    };

                    let alias = same(&value);

                    return &alias.value;
                }
            "#,
        r#"
                module app;

                struct Holder
                {
                    value: bool;
                }

                func project(pos value: &Holder) -> &bool
                {
                    return &value.value;
                }

                func bad() -> &bool
                {
                    let value = Holder
                    {
                        value = true
                    };

                    let alias = &value;

                    return project(alias);
                }
            "#,
        r#"
                module app;

                func bad() -> (&bool)?
                {
                    let value: bool = true;
                    let mut result: (&bool)? = none;

                    result = &value;
                    return result;
                }
            "#,
        r#"
                module app;

                struct Holder
                {
                    value: bool;

                    func borrow() -> &bool
                    {
                        return &self.value;
                    }
                }

                func bad() -> &bool
                {
                    let value = Holder
                    {
                        value = true
                    };

                    return value.borrow();
                }
            "#,
        r#"
                module app;

                struct Holder
                {
                    value: &bool;

                    consume func take() -> &bool
                    {
                        return self.value;
                    }
                }

                func bad() -> &bool
                {
                    let value: bool = true;

                    let holder = Holder
                    {
                        value = &value
                    };

                    return holder.take();
                }
            "#,
        r#"
                module app;

                func choose(pos value: &bool, other: &bool = value) -> &bool
                {
                    return other;
                }

                func bad() -> &bool
                {
                    let value: bool = true;

                    return choose(&value);
                }
            "#,
        r#"
                module app;

                struct Holder
                {
                    value: bool;
                }

                func same(pos value: &Holder) -> &Holder
                {
                    return value;
                }

                func project(pos value: &Holder) -> &bool
                {
                    return &(same(value)).value;
                }

                func bad() -> &bool
                {
                    let value = Holder
                    {
                        value = true
                    };

                    return project(&value);
                }
            "#,
        r#"
                module app;

                struct Holder
                {
                    value: &bool;
                }

                func wrap(pos transient: &bool, pos anchor: &bool) -> Holder
                {
                    return Holder
                    {
                        value = anchor
                    };
                }

                func choose(pos transient: &bool, pos anchor: &bool, value: Holder = wrap(transient, anchor)) -> Holder
                {
                    return value;
                }

                func bad() -> Holder
                {
                    let value: bool = true;

                    return choose(&value, &value);
                }
            "#,
        r#"
                module app;

                struct Holder
                {
                    value: bool;
                }

                func project(pos value: &Holder) -> &bool
                {
                    return &value.value;
                }

                struct Owner
                {
                    source: &Holder;

                    consume func take() -> &bool
                    {
                        return project(self.source);
                    }
                }

                func bad() -> &bool
                {
                    let value = Holder
                    {
                        value = true
                    };

                    let owner = Owner
                    {
                        source = &value
                    };

                    return owner.take();
                }
            "#,
        r#"
                module app;

                func first(pos values: &[bool; 2]) -> &bool
                {
                    return &values[0];
                }

                func bad() -> &bool
                {
                    let values = [true, false];

                    return first(&values);
                }
            "#,
        r#"
                module app;

                func bad(pos value: bool) -> &bool
                {
                    return &value;
                }
            "#,
        r#"
                module app;

                func call(pos function: func(pos value: &bool) -> &bool, pos value: &bool) -> &bool
                {
                    return function(value);
                }

                func bad(pos function: func(pos value: &bool) -> &bool) -> &bool
                {
                    let local: bool = true;

                    return call(function, &local);
                }
            "#,
        r#"
                module app;

                struct Pair
                {
                    first: &bool;
                    second: &bool;
                }

                func first(pos value: Pair) -> &bool
                {
                    return value.first;
                }

                func wrap(pos first_value: &bool, pos second_value: &bool) -> &bool
                {
                    return first(
                        Pair
                        {
                            first = first_value,
                            second = second_value
                        }
                    );
                }

                func bad(pos anchor: &bool) -> &bool
                {
                    let local: bool = true;

                    return wrap(&local, anchor);
                }
            "#,
    ] {
        let compilation = compilation(source);
        let key = source_function_body_key(&compilation, "bad");

        let flow = compilation
            .storage_flow(key)
            .expect("storage flow must publish");

        assert!(
            flow.diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.kind()
                    == DiagnosticKind::CheckingEscapingStorageDependency),
            "{source}: {:?}",
            flow.diagnostics()
        );

        assert_goal_state_diagnostic_kind(
            flow.diagnostics(),
            DiagnosticKind::CheckingEscapingStorageDependency,
        );
    }
}

#[test]
fn returned_values_preserve_caller_storage_and_owned_results() {
    for source in [
        r#"
                module app;

                struct Value<T>
                {
                    values: [T; 2];
                }

                union Failure
                {
                    Unavailable;
                }

                func good(pos produce: func(pos input: &bool) -> Result<Value<bool>, Failure>)
                    -> Result<Value<bool>, Failure>
                {
                    let local: bool = true;

                    return produce(&local);
                }
            "#,
        r#"
                module app;

                func good() -> &string
                {
                    return &"product lifetime";
                }
            "#,
        r#"
                module app;

                func same(pos value: &string) -> &string
                {
                    return value;
                }

                func good() -> &string
                {
                    return same(&"product lifetime");
                }
            "#,
        r#"
                module app;

                func good(pos value: &bool) -> &bool
                {
                    return value;
                }
            "#,
        r#"
                module app;

                struct Holder
                {
                    value: bool;
                }

                func same(pos value: &Holder) -> &Holder
                {
                    return value;
                }

                func good(pos value: &Holder) -> &bool
                {
                    let alias = same(value);

                    return &alias.value;
                }
            "#,
        r#"
                module app;

                func project<T>(pos value: &box T) -> &T
                {
                    return match value
                    {
                        case box(inner)
                        {
                            yield &inner;
                        }
                    };
                }

                func good(pos value: &box bool) -> &bool
                {
                    return project(value);
                }
            "#,
        r#"
                module app;

                func good(pos value: &bool) -> (&bool)?
                {
                    let mut result: (&bool)? = none;

                    result = value;
                    return result;
                }
            "#,
        r#"
                module app;

                struct Holder
                {
                    value: &bool;

                    consume func take() -> &bool
                    {
                        return self.value;
                    }
                }

                func good(pos value: &bool) -> &bool
                {
                    let holder = Holder
                    {
                        value = value
                    };

                    return holder.take();
                }
            "#,
        r#"
                module app;

                func same<T>(pos value: &T, pos stop: bool) -> &T
                {
                    if stop
                    {
                        return value;
                    }

                    return same<T>(value, true);
                }

                func good(pos value: &bool) -> &bool
                {
                    return same(value, false);
                }
            "#,
        r#"
                module app;

                struct Holder
                {
                    value: bool;
                }

                func good(pos value: &Holder) -> &bool
                {
                    let alias = value;

                    return &alias.value;
                }
            "#,
        r#"
                module app;

                func good() -> bool
                {
                    let value: bool = true;
                    let alias = &value;

                    return true;
                }
            "#,
        r#"
                module app;

                func good() -> bool
                {
                    let value: bool = true;

                    return read(&value);
                }

                func read(pos value: &bool) -> bool
                {
                    return true;
                }
            "#,
        r#"
                module app;

                struct Holder
                {
                    value: bool;
                }

                func good() -> Holder
                {
                    let value = Holder
                    {
                        value = true
                    };

                    return value;
                }
            "#,
        r#"
                module app;

                func same<T>(pos value: &T) -> &T
                {
                    return value;
                }

                func good(pos value: &bool) -> &bool
                {
                    return same(value);
                }
            "#,
        r#"
                module app;

                func same<T>(pos value: T) -> T
                {
                    return value;
                }

                struct Holder
                {
                    value: bool;
                }

                func good() -> Holder
                {
                    let value = Holder
                    {
                        value = true
                    };

                    return same(value);
                }
            "#,
        r#"
                module app;

                func use_value(pos value: &mut bool)
                {
                }

                func good() -> bool
                {
                    let mut count: i32 = 0;

                    while count < 2
                    {
                        let mut value: bool = true;

                        use_value(&mut value);
                        count += 1;
                    }

                    return true;
                }
            "#,
        r#"
                module app;

                func choose(pos value: &bool, other: &bool = value) -> &bool
                {
                    return other;
                }

                func good(pos value: &bool) -> &bool
                {
                    return choose(value);
                }
            "#,
        r#"
                module app;

                struct Holder
                {
                    value: &bool;
                }

                func wrap(pos transient: &bool, pos anchor: &bool) -> Holder
                {
                    return Holder
                    {
                        value = anchor
                    };
                }

                func choose(pos transient: &bool, pos anchor: &bool, value: Holder = wrap(transient, anchor)) -> Holder
                {
                    return value;
                }

                func good(pos anchor: &bool) -> Holder
                {
                    let transient: bool = true;

                    return choose(&transient, anchor);
                }
            "#,
        r#"
                module app;

                struct Holder
                {
                    value: bool;
                }

                func project(pos value: &Holder) -> &bool
                {
                    return &value.value;
                }

                struct Owner
                {
                    source: &Holder;

                    consume func take() -> &bool
                    {
                        return project(self.source);
                    }
                }

                func good(pos value: &Holder) -> &bool
                {
                    let owner = Owner
                    {
                        source = value
                    };

                    return owner.take();
                }
            "#,
        r#"
                module app;

                func first(pos values: &[bool; 2]) -> &bool
                {
                    return &values[0];
                }

                func good(pos values: &[bool; 2]) -> &bool
                {
                    return first(values);
                }
            "#,
        r#"
                module app;

                struct Pair
                {
                    first: &bool;
                    second: &bool;
                }

                func first(pos value: Pair) -> &bool
                {
                    return value.first;
                }

                func good(pos anchor: &bool) -> &bool
                {
                    let local: bool = true;

                    return first(
                        Pair
                        {
                            first = anchor,
                            second = &local
                        }
                    );
                }
            "#,
        r#"
                module app;

                struct Pair
                {
                    first: &bool;
                    second: &bool;
                }

                func first(pos value: Pair) -> &bool
                {
                    return value.first;
                }

                func wrap(pos first_value: &bool, pos second_value: &bool) -> &bool
                {
                    let pair = Pair
                    {
                        first = first_value,
                        second = second_value
                    };

                    return first(pair);
                }

                func good(pos anchor: &bool) -> &bool
                {
                    let local: bool = true;

                    return wrap(anchor, &local);
                }
            "#,
    ] {
        let compilation = compilation(source);

        assert!(
            !compilation.check_diagnostics().has_errors(),
            "{source}: {:?}",
            compilation.check_diagnostics()
        );

        let key = source_function_body_key(&compilation, "good");

        let flow = compilation
            .storage_flow(key)
            .expect("storage flow must publish");

        assert!(
            !flow.diagnostics().has_errors(),
            "{source}: {:?}",
            flow.diagnostics()
        );
    }
}

#[test]
fn returned_static_borrows_preserve_their_owner_through_calls() {
    for (directive, exact_thread) in [("", false), ("@thread_local", true)] {
        let source = format!(
            r#"
            module app;
            struct Holder {{
                value: u32;
                func borrow() -> &Self {{ return &self; }}
            }}
            {directive} static STATE: Holder = {{ value = 7 }};
            func projected() -> &u32 {{ return &STATE.borrow().value; }}
            func forwarded() -> &u32 {{ return projected(); }}
            "#,
        );

        let application = compilation(&source);
        let diagnostics = application.check_diagnostics();

        assert!(!diagnostics.has_errors(), "{source}\n{diagnostics:#?}");

        let callable = crate::test_support::source_function(&application, "forwarded");

        let context = application
            .binding_context(&application.state.cancellation)
            .expect("static borrow binding context must build");

        let dependencies = bray_binder::SymbolQueryProvider::resolve_symbol_query(
            &context,
            bray_symbols::SymbolQueryRequest::<bray_symbols::CallableResultDependenciesQuery>::new(
                callable.into(),
            ),
        )
        .expect("static borrow must publish its retained owner");

        let template = application
            .semantic_value_store()
            .expect("semantic values must exist")
            .dependency_contract_template_data(*dependencies.value());

        assert!(
            template.requirements().iter().any(|requirement| {
                let bray_symbols::DependencyRequirement::Direct { subject, .. } = requirement
                else {
                    return false;
                };
                matches!(
                    subject.subject_root(),
                    bray_symbols::DependencySubjectRoot::ExactThreadStatic(_)
                ) == exact_thread
                    && matches!(
                        subject.subject_root(),
                        bray_symbols::DependencySubjectRoot::ProductStatic(_)
                            | bray_symbols::DependencySubjectRoot::ExactThreadStatic(_)
                    )
            }),
            "{template:#?}"
        );
    }
}

#[test]
fn exact_returned_borrow_origins_follow_replacements_and_require_path_agreement() {
    for (body, expected) in [
        ("return first;", Some(0)),
        ("if condition { return first; } return first;", Some(0)),
        ("if condition { return first; } return second;", None),
        ("return forward(first);", Some(0)),
        (
            "let result: Holder = { value = first }; return result;",
            Some(0),
        ),
        (
            "let mut result: Holder = { value = first }; result.value = second; return result;",
            Some(1),
        ),
        (
            "let mut result: Holder = { value = first }; replace(&mut result, second); return result;",
            Some(1),
        ),
        (
            "let mut result: Holder = { value = first }; let alias = &mut result; replace(alias, second); return result;",
            Some(1),
        ),
        (
            "let mut result: Holder = { value = first }; if condition { replace(&mut result, second); } return result;",
            None,
        ),
        (
            "let mut result: Holder = { value = first }; result.value = result.value; return result;",
            Some(0),
        ),
    ] {
        let result_type = if body.contains("Holder") {
            "Holder"
        } else {
            "&bool"
        };

        let source = format!(
            r#"
            module app;
            struct Holder {{ mut value: &bool; }}
            func forward(pos value: &bool) -> &bool {{ return value; }}
            func replace(pos target: &mut Holder, pos value: &bool) {{ target.value = value; }}
            func test(pos first: &bool, pos second: &bool, pos condition: bool) -> {result_type} {{ {body} }}
        "#
        );

        let application = compilation(&source);
        let diagnostics = application.check_diagnostics();

        assert!(!diagnostics.has_errors(), "{source}\n{diagnostics:#?}");

        if body.contains("replace(&mut") {
            let replaced = crate::compilation::result_dependencies::source_function_dependencies(
                &application,
                "replace",
            );

            assert!(
                !replaced.borrows().is_empty(),
                "replacement must publish its changed field: {replaced:#?}"
            );
        }

        let template = crate::compilation::result_dependencies::source_function_dependencies(
            &application,
            "test",
        );

        let returns = template
            .borrows()
            .iter()
            .filter(|relation| {
                relation.exit().subject_root() == bray_symbols::DependencySubjectRoot::Result
            })
            .collect::<Vec<_>>();

        assert_eq!(
            !returns.is_empty(),
            expected.is_some(),
            "{source}\n{template:#?}"
        );

        for relation in returns {
            assert_eq!(
                relation.entry().subject_root(),
                bray_symbols::DependencySubjectRoot::Parameter(bray_symbols::SymbolOrdinal::new(
                    expected.expect("an exact returned reference has an input")
                ))
            );
        }
    }
}

#[test]
fn stored_borrow_preservation_excludes_replacement_and_carrier_escape() {
    for (lifecycle, body, preserved) in [
        ("", "observe(value.reference);", true),
        (
            "destruct() { self.reference = self.other; }",
            "observe(value.reference);",
            false,
        ),
        (
            "",
            "value.reference = other; observe(value.reference);",
            false,
        ),
        (
            "",
            "replace(&mut value, other); observe(value.reference);",
            false,
        ),
        (
            "",
            "let alias = &mut value; replace(alias, other); observe(value.reference);",
            false,
        ),
    ] {
        let source = format!(
            r#"
            module app;
            struct Holder {{ mut reference: &bool; other: &bool; {lifecycle} }}
            func observe(pos value: &bool) {{}}
            func replace(pos value: &mut Holder, pos other: &bool) {{ value.reference = other; }}
            func test(pos mut value: Holder, pos other: &bool) {{ {body} }}
        "#
        );

        let application = compilation(&source);
        let diagnostics = application.check_diagnostics();

        assert!(!diagnostics.has_errors(), "{source}\n{diagnostics:#?}");

        let template = crate::compilation::result_dependencies::source_function_dependencies(
            &application,
            "test",
        );

        assert_eq!(
            template
                .borrows()
                .iter()
                .any(|relation| relation.entry() == relation.exit()
                    && !relation.entry().projections().is_empty()),
            preserved,
            "{source}\n{template:#?}"
        );
    }
}

#[test]
fn nullable_borrow_origins_preserve_payload_paths() {
    for (result_type, body, expected, nullable_depth) in [
        (
            "(&bool)?",
            "if condition { return none; } return first;",
            true,
            1,
        ),
        (
            "(&bool)?",
            "if condition { return first; } return second;",
            false,
            1,
        ),
        (
            "(&bool)?",
            "let value: (&bool)? = first; return value;",
            true,
            1,
        ),
        ("(&bool)?", "return optional(first, condition);", true, 1),
        (
            "Holder",
            "let wrapped: (&bool)? = first; return { value = wrapped };",
            true,
            1,
        ),
        (
            "Holder?",
            "if condition { return none; } let wrapped: (&bool)? = first; let value: Holder = { value = wrapped }; return value;",
            true,
            2,
        ),
        (
            "&bool",
            "if condition { return first; } return test(first, second, condition);",
            false,
            0,
        ),
    ] {
        let source = format!(
            r#"
            module app;
            struct Holder {{ value: (&bool)?; }}
            func optional(pos value: &bool, pos flag: bool) -> (&bool)? {{ if flag {{ return none; }} return value; }}
            func test(pos first: &bool, pos second: &bool, pos condition: bool) -> {result_type} {{ {body} }}
        "#
        );

        let application = compilation(&source);
        let diagnostics = application.check_diagnostics();

        assert!(!diagnostics.has_errors(), "{source}\n{diagnostics:#?}");

        let template = crate::compilation::result_dependencies::source_function_dependencies(
            &application,
            "test",
        );

        let returned = template
            .borrows()
            .iter()
            .filter(|relation| {
                relation.exit().subject_root() == bray_symbols::DependencySubjectRoot::Result
            })
            .collect::<Vec<_>>();

        assert_eq!(!returned.is_empty(), expected, "{source}\n{template:#?}");

        for relation in returned {
            assert_eq!(
                relation.entry().subject_root(),
                bray_symbols::DependencySubjectRoot::Parameter(bray_symbols::SymbolOrdinal::new(0))
            );

            assert_eq!(
                relation
                    .exit()
                    .projections()
                    .iter()
                    .filter(|projection| **projection
                        == bray_symbols::DependencyProjection::NullableValue)
                    .count(),
                nullable_depth,
                "{relation:?}"
            );
        }
    }
}

#[test]
fn copied_stored_borrow_outlives_its_consumed_carrier() {
    let application = compilation(
        r#"
        module app;
        struct Holder {
            reference: &bool;
            func get() -> &bool { return self.reference; }
            consume func finish() {}
        }
        func test(pos holder: Holder) -> &bool {
            let reference = holder.get();
            holder.finish();
            return reference;
        }
    "#,
    );

    let diagnostics = application.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:#?}");
}

#[test]
fn returned_borrow_storage_uses_exact_relations_instead_of_lifetime_anchors() {
    for (body, exact) in [
        ("return &mut pair.second;", true),
        (
            "if condition { return &mut pair.first; } return &mut pair.second;",
            false,
        ),
    ] {
        let source = format!(
            r#"
            module app;
            struct Cell {{ mut value: bool; }}
            struct Pair {{ mut first: Cell; mut second: Cell; }}
            func select_value(pos pair: &mut Pair, pos condition: bool) -> &mut Cell {{ {body} }}
            func caller(pos pair: &mut Pair, pos condition: bool) {{
                let selected = select_value(pair, condition);
                selected.value = false;
            }}
        "#
        );

        let application = compilation(&source);
        let callable = crate::test_support::source_function(&application, "select_value");

        let context = application
            .binding_context(&application.state.cancellation)
            .unwrap();

        let dependencies = bray_binder::SymbolQueryProvider::resolve_symbol_query(
            &context,
            bray_symbols::SymbolQueryRequest::<bray_symbols::CallableResultDependenciesQuery>::new(
                callable.into(),
            ),
        )
        .unwrap();

        let template = application
            .semantic_value_store()
            .unwrap()
            .dependency_contract_template_data(*dependencies.value());

        assert_eq!(
            !template.borrows().is_empty(),
            exact,
            "{source}\n{template:#?}"
        );

        let producer = application
            .storage_flow(source_function_body_key(&application, "select_value"))
            .unwrap();

        assert_eq!(
            !producer.value().dependencies().borrows().is_empty(),
            exact,
            "{source}\nnormal-exit references: {:?}",
            producer.value().dependencies().borrows()
        );

        let key = source_function_body_key(&application, "caller");
        let selections = application.semantic_selections(key.clone()).unwrap();

        let call = selections
            .value()
            .entries()
            .iter()
            .find(|entry| {
                matches!(
                    entry.selection(),
                    bray_bound_tree::SemanticSelection::Call(_)
                )
            })
            .unwrap()
            .expression();

        let flow = application.storage_flow(key.clone()).unwrap();
        let refinements = application.refinements(key.clone()).unwrap();
        let storage = application.storage_plan(key).unwrap();

        assert!(
            !flow.diagnostics().has_errors(),
            "{source}\n{:?}",
            flow.diagnostics()
        );

        assert!(
            !storage.diagnostics().has_errors(),
            "{source}\n{storage:#?}"
        );

        let storage = storage.value();

        let returned = storage
            .borrow_capability_entries()
            .filter(|(_, capability)| capability.expression() == Some(call))
            .collect::<Vec<_>>();

        assert!(
            returned.is_empty(),
            "storage planning leaves call results symbolic"
        );

        let observed = refinements
            .value()
            .invalidations()
            .values()
            .filter_map(|invalidation| {
                let bray_bound_tree::StorageInvalidation::Observed { accesses, .. } = invalidation
                else {
                    return None;
                };

                accesses.values().flatten().find(|path| {
                    matches!(
                        storage.identity(path.root),
                        Some(bray_bound_tree::StorageIdentity::Parameter(_))
                    ) && path.projections.len() == 2
                })
            })
            .collect::<Vec<_>>();

        assert_eq!(
            !observed.is_empty(),
            exact,
            "{source}\n{:?}",
            refinements.value().invalidations()
        );

        if exact {
            let bray_symbols::DependencyProjection::ProductField(expected) =
                template.borrows()[0].target_projections()[0]
            else {
                panic!("the exact result refers to the selected field");
            };

            assert!(observed.iter().all(|path| path.projections[0]
                == bray_bound_tree::StorageProjection::ProductField(expected)));
        }
    }
}

#[test]
fn unknown_returned_borrow_keeps_every_possible_input_protected() {
    for mutated in ["first", "second"] {
        let source = format!(
            r#"
            module app;
            struct Cell {{ mut value: bool; }}
            func select_value(pos first: &Cell, pos second: &Cell, pos condition: bool) -> &Cell {{
                if condition {{ return first; }}
                return second;
            }}
            func caller(pos condition: bool) -> bool {{
                let mut first: Cell = {{ value = true }};
                let mut second: Cell = {{ value = false }};
                let selected = select_value(&first, &second, condition);
                {mutated}.value = false;
                return selected.value;
            }}
        "#
        );

        let application = compilation(&source);

        assert_goal_state_diagnostic_kind(
            application.check_diagnostics(),
            DiagnosticKind::CheckingConflictingBorrow,
        );
    }
}

#[test]
fn copied_scalar_releases_unknown_returned_borrow_dependencies() {
    let application = compilation(
        r#"
        module app;
        struct Cell { mut value: bool; }
        func select_value(pos first: &Cell, pos second: &Cell, pos condition: bool) -> &Cell {
            if condition { return first; }
            return second;
        }
        func caller(pos condition: bool) -> bool {
            let mut first: Cell = { value = true };
            let mut second: Cell = { value = false };
            let selected = select_value(&first, &second, condition);
            let copied = selected.value;
            first.value = false;
            second.value = true;
            return copied;
        }
    "#,
    );

    let diagnostics = application.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:#?}");
}

#[test]
fn reading_an_independent_carrier_field_does_not_retain_other_field_loans() {
    let application = compilation(
        r#"
        module app;
        struct Cell { mut value: bool; }
        struct Holder { reference: &Cell; flag: bool; }
        func hold(pos cell: &Cell) -> Holder {
            return { reference = cell, flag = true };
        }
        func caller() -> bool {
            let mut cell: Cell = { value = false };
            let holder = hold(&cell);
            cell.value = true;
            return holder.flag;
        }
    "#,
    );

    let diagnostics = application.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:#?}");
}

#[test]
fn reading_through_a_stored_reference_retains_its_loan() {
    let application = compilation(
        r#"
        module app;
        struct Cell { mut value: bool; }
        struct Holder { reference: &Cell; flag: bool; }
        func hold(pos cell: &Cell) -> Holder {
            return { reference = cell, flag = true };
        }
        func caller() -> bool {
            let mut cell: Cell = { value = false };
            let holder = hold(&cell);
            cell.value = true;
            return holder.reference.value;
        }
    "#,
    );

    assert_goal_state_diagnostic_kind(
        application.check_diagnostics(),
        DiagnosticKind::CheckingConflictingBorrow,
    );
}

#[test]
fn normal_exit_references_follow_constructed_fields_and_nullable_conversion() {
    for (result_type, body, depth) in [
        ("&bool", "return value;", 0),
        ("(&bool)?", "return value;", 1),
        ("(&bool, bool)", "return (value, true);", 1),
        ("Holder", "return { reference = value, flag = true };", 1),
        (
            "Holder?",
            "let holder: Holder = { reference = value, flag = true }; return holder;",
            2,
        ),
    ] {
        let source = format!(
            r#"
            module app;
            struct Holder {{ reference: &bool; flag: bool; }}
            func returned(pos value: &bool) -> {result_type} {{
                {body}
            }}
        "#
        );

        let application = compilation(&source);

        let flow = application
            .storage_flow(source_function_body_key(&application, "returned"))
            .unwrap();

        assert!(!flow.diagnostics().has_errors(), "{source}\n{flow:#?}");

        let references = flow.value().dependencies().borrows();

        assert_eq!(references.len(), 1, "{source}\n{references:?}");

        assert_eq!(
            references[0].entry(),
            &bray_symbols::DependencySubject::root(bray_symbols::DependencySubjectRoot::Parameter(
                bray_symbols::SymbolOrdinal::new(0)
            ))
        );

        assert_eq!(references[0].exit().projections().len(), depth, "{source}");
    }
}

#[test]
fn normal_exit_references_keep_original_stored_inputs() {
    for (body, expected_parameter) in [
        ("return holder.reference;", 0),
        (
            "holder.reference = replacement; return holder.reference;",
            1,
        ),
        (
            "let original = holder.reference; holder.reference = replacement; return original;",
            0,
        ),
    ] {
        let source = format!(
            r#"
            module app;
            struct Holder {{ mut reference: &bool; }}
            func returned(pos mut holder: Holder, pos replacement: &bool) -> &bool {{
                {body}
            }}
        "#
        );

        let application = compilation(&source);

        let flow = application
            .storage_flow(source_function_body_key(&application, "returned"))
            .unwrap();

        assert!(
            !flow.diagnostics().has_errors(),
            "{source}\n{:?}",
            flow.diagnostics()
        );

        let references = flow
            .value()
            .dependencies()
            .borrows()
            .iter()
            .filter(|relation| {
                relation.exit().subject_root() == bray_symbols::DependencySubjectRoot::Result
            })
            .collect::<Vec<_>>();

        assert_eq!(references.len(), 1, "{source}\n{references:?}");

        assert_eq!(
            references[0].entry().subject_root(),
            bray_symbols::DependencySubjectRoot::Parameter(bray_symbols::SymbolOrdinal::new(
                expected_parameter
            )),
            "{source}"
        );
    }
}

#[test]
fn absent_return_payload_does_not_invent_a_conflicting_reference() {
    let application = compilation(
        r#"
        module app;
        func returned(pos value: &bool, pos present: bool) -> (&bool)? {
            if present { return value; }
            return none;
        }
    "#,
    );

    let flow = application
        .storage_flow(source_function_body_key(&application, "returned"))
        .unwrap();

    assert!(!flow.diagnostics().has_errors(), "{:?}", flow.diagnostics());

    assert_eq!(
        flow.value().dependencies().borrows(),
        &[bray_symbols::DependencyBorrowRelation::new(
            bray_symbols::DependencySubject::root(bray_symbols::DependencySubjectRoot::Parameter(
                bray_symbols::SymbolOrdinal::new(0)
            )),
            bray_symbols::DependencySubject::new(
                bray_symbols::DependencySubjectRoot::Result,
                [bray_symbols::DependencyProjection::NullableValue]
            ),
            bray_symbols::DependencyReferenceKind::Borrow,
        )]
    );
}
#[test]
fn returned_field_addresses_differ_from_stored_reference_values() {
    for (name, projected, body) in [
        ("stored", false, "return holder.reference;"),
        ("address", true, "return &holder.reference;"),
        ("forwarded", true, "return address(holder);"),
    ] {
        let result = if projected { "&&bool" } else { "&bool" };

        let helper = if name == "forwarded" {
            "func address(pos holder: &Holder) -> &&bool { return &holder.reference; }"
        } else {
            ""
        };

        let application = compilation(&format!(
            r#"
            module app;
            struct Holder {{ reference: &bool; }}
            {helper}
            func {name}(pos holder: &Holder) -> {result} {{ {body} }}
        "#
        ));

        let dependencies = crate::compilation::result_dependencies::source_function_dependencies(
            &application,
            name,
        );

        let references = dependencies
            .borrows()
            .iter()
            .filter(|relation| {
                relation.exit().subject_root() == bray_symbols::DependencySubjectRoot::Result
            })
            .collect::<Vec<_>>();

        assert_eq!(references.len(), 1, "{name}: {dependencies:?}");

        assert_eq!(
            references[0].entry().projections().len(),
            usize::from(!projected),
            "{name}: {references:?}"
        );

        assert_eq!(
            references[0].target_projections().len(),
            usize::from(projected),
            "{name}: {references:?}"
        );
    }
}

#[test]
fn mutation_observations_preserve_sibling_fields_but_invalidate_other_input_aliases() {
    let application = compilation(
        r#"
        module app;
        struct Cell { mut value: bool; }
        struct Pair { mut first: Cell; mut second: Cell; }
        func caller(pos left: &mut Pair, pos right: &mut Pair) -> bool {
            left.first.value = false;
            return left.second.value && right.second.value;
        }
    "#,
    );

    let key = source_function_body_key(&application, "caller");
    let storage = application.storage_plan(key.clone()).unwrap();
    let refinements = application.refinements(key).unwrap();
    let storage = storage.value();

    let mutation = refinements.value().invalidations().values().find(|invalidation| {
        matches!(invalidation, bray_bound_tree::StorageInvalidation::Observed { accesses, .. } if !accesses.is_empty())
    }).expect("assignment publishes resolved observations");

    let bray_bound_tree::StorageInvalidation::Observed { accesses, .. } = mutation else {
        unreachable!()
    };

    let mut checked_left = false;
    let mut checked_right = false;

    for (access, _) in storage.access_entries() {
        let Some(path) = storage.logical_path(access) else {
            continue;
        };

        if path.projections.len() != 2 {
            continue;
        }

        let Some(bray_bound_tree::StorageIdentity::Parameter(parameter)) =
            storage.identity(path.root)
        else {
            continue;
        };

        let graph = application.symbol_graph().unwrap();
        let parameter = graph.callable_parameter(parameter).unwrap();

        let bray_bound_tree::StorageProjection::ProductField(field) = path.projections[0] else {
            continue;
        };

        if graph.member_name(field.into()).unwrap().as_str() != "second" {
            continue;
        }

        if parameter.ordinal() == 0 {
            assert!(
                !accesses.contains_key(&access),
                "same referent sibling is disjoint: {path:?}"
            );

            checked_left = true;
        } else {
            assert!(
                accesses.contains_key(&access),
                "different input may alias: {path:?}"
            );

            checked_right = true;
        }
    }

    assert!(checked_left && checked_right);
}

#[test]
fn protected_payload_addresses_preserve_their_storage_origin() {
    let application = compilation(
        r#"
        module app;
        struct Cell<T> {
            storage: Uninit<T>;
            func pointer() -> RawPointer<T> executes(pure, total) {
                return core.memory.uninit_pointer<T>(&self.storage);
            }
            func forwarded() -> RawPointer<T> executes(pure, total) {
                return self.pointer();
            }
        }
        struct Holder<T> {
            cell: &Cell<T>;
            func storage() -> &Uninit<T> executes(pure, total) {
                return &self.cell.storage;
            }
            func pointer() -> RawPointer<T> executes(pure, total) {
                return direct<T>(self.storage());
            }
        }
        func holder_method<T>(pos holder: &Holder<T>) -> RawPointer<T> {
            return holder.pointer();
        }
        func direct<T>(pos storage: &Uninit<T>) -> RawPointer<T> {
            return core.memory.uninit_pointer<T>(storage);
        }
        func method<T>(pos cell: &Cell<T>) -> RawPointer<T> {
            return cell.forwarded();
        }
        func forwarded<T>(pos storage: &Uninit<T>) -> RawPointer<T> {
            return direct<T>(storage);
        }
        func field<T>(pos holder: &Holder<T>) -> RawPointer<T> {
            return forwarded<T>(&holder.cell.storage);
        }
    "#,
    );

    for (name, entry_length, target_length) in [
        ("direct", 0, 1),
        ("forwarded", 0, 1),
        ("field", 1, 2),
        ("method", 0, 2),
        ("holder_method", 1, 2),
    ] {
        let dependencies = crate::compilation::result_dependencies::source_function_dependencies(
            &application,
            name,
        );

        let relations = dependencies
            .borrows()
            .iter()
            .filter(|relation| {
                relation.exit().subject_root() == bray_symbols::DependencySubjectRoot::Result
            })
            .collect::<Vec<_>>();

        assert_eq!(relations.len(), 1, "{name}: {dependencies:#?}");

        let relation = relations[0];

        assert_eq!(
            relation.kind(),
            bray_symbols::DependencyReferenceKind::Address
        );

        assert_eq!(
            relation.entry().projections().len(),
            entry_length,
            "{name}: {relation:#?}"
        );

        assert_eq!(
            relation.target_projections().len(),
            target_length,
            "{name}: {relation:#?}"
        );

        assert_eq!(
            relation.target_projections().last(),
            Some(&bray_symbols::DependencyProjection::UninitPayload)
        );
    }
}

#[test]
fn protected_payload_values_preserve_reference_origins_and_reject_local_escape() {
    let application = compilation(
        r#"
        trusted module app;
        trusted func identity(pos value: &bool) -> &bool uses(unchecked_init) {
            let storage = core.memory.uninit_from<(&bool)>(value);
            return trusted core.memory.assume_initialized<(&bool)>(storage);
        }
        trusted func bad() -> &bool uses(unchecked_init) {
            let value = true;
            let storage = core.memory.uninit_from<(&bool)>(&value);
            return trusted core.memory.assume_initialized<(&bool)>(storage);
        }
    "#,
    );

    let checked = application
        .storage_flow(source_function_body_key(&application, "identity"))
        .unwrap();

    assert!(checked.diagnostics().is_empty(), "{checked:#?}");

    let dependencies = crate::compilation::result_dependencies::source_function_dependencies(
        &application,
        "identity",
    );

    assert!(
        dependencies.borrows().iter().any(|relation| {
            relation.entry().subject_root()
                == bray_symbols::DependencySubjectRoot::Parameter(bray_symbols::SymbolOrdinal::new(
                    0,
                ))
                && relation.exit().subject_root() == bray_symbols::DependencySubjectRoot::Result
                && relation.kind() == bray_symbols::DependencyReferenceKind::Borrow
        }),
        "{dependencies:#?}"
    );

    let flow = application
        .storage_flow(source_function_body_key(&application, "bad"))
        .unwrap();

    assert_goal_state_diagnostic_kind(
        flow.diagnostics(),
        DiagnosticKind::CheckingEscapingStorageDependency,
    );
}

#[test]
fn call_summary_reference_fields_follow_replacement_and_require_path_agreement() {
    for (body, exact) in [
        ("self.value = next;", true),
        ("if change { self.value = next; }", false),
    ] {
        let application = compilation(&format!(
            r#"
            module app;
            struct Slot {{
                mut value: &bool;
                func get() -> &bool executes(pure, total) {{
                    return self.value;
                }}
                mut func replace(pos next: &bool, pos change: bool) executes(total) {{
                    {body}
                }}
            }}
            func caller(pos slot: &mut Slot, pos next: &bool, pos change: bool) -> &bool {{
                slot.replace(next, change);
                return slot.get();
            }}
        "#
        ));

        let flow = application
            .storage_flow(source_function_body_key(&application, "caller"))
            .unwrap();

        assert!(flow.diagnostics().is_empty(), "{body}: {flow:#?}");

        let dependencies = crate::compilation::result_dependencies::source_function_dependencies(
            &application,
            "caller",
        );

        let origins = dependencies
            .borrows()
            .iter()
            .filter(|relation| {
                relation.exit().subject_root() == bray_symbols::DependencySubjectRoot::Result
            })
            .collect::<Vec<_>>();

        assert_eq!(
            origins.len(),
            usize::from(exact),
            "{body}: {dependencies:#?}"
        );

        if let Some(origin) = origins.first() {
            assert_eq!(
                origin.entry().subject_root(),
                bray_symbols::DependencySubjectRoot::Parameter(bray_symbols::SymbolOrdinal::new(1))
            );
        }
    }
}
