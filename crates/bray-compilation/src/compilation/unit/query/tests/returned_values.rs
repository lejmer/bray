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
