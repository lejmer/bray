use crate::test_support::{compilation, source_function_body_key};
use bray_diagnostics::DiagnosticKind;
use bray_testing::assert_goal_state_diagnostic_kind;

#[test]
fn abstract_trait_results_converge_through_recursive_defaults() {
    let compilation = compilation(
        r#"
            module app;

            trait Project
            {
                func leaf() -> &bool;

                func project(pos repeat: bool) -> &bool
                {
                    if repeat
                    {
                        return self.project(false);
                    }

                    return self.leaf();
                }
            }

            struct Holder
            {
                value: bool;
            }

            impl HolderProject = Holder(Project)
            {
                func leaf() -> &bool
                {
                    return &self.value;
                }
            }

            func forward<T>(pos value: &T) -> &bool with(T: Project)
            {
                return value(Project).project(true);
            }

            func bad() -> &bool
            {
                let local = Holder
                {
                    value = true,
                };

                return forward(&local);
            }

            func good(pos value: &Holder) -> &bool
            {
                return forward(value);
            }
        "#,
    );

    let flow = compilation
        .storage_flow(source_function_body_key(&compilation, "bad"))
        .unwrap();

    assert_goal_state_diagnostic_kind(
        flow.diagnostics(),
        DiagnosticKind::CheckingEscapingStorageDependency,
    );

    let flow = compilation
        .storage_flow(source_function_body_key(&compilation, "good"))
        .unwrap();

    assert!(!flow.diagnostics().has_errors(), "{:?}", flow.diagnostics());
}

#[test]
fn abstract_trait_results_converge_through_generic_recursive_defaults() {
    let compilation = compilation(
        r#"
            module app;

            trait Project
            {
                func leaf() -> &bool;

                func project<V>(pos repeat: bool) -> &bool
                {
                    if repeat
                    {
                        return self.project<(V, V)>(false);
                    }

                    return self.leaf();
                }
            }

            struct Holder
            {
                value: bool;
            }

            impl HolderProject = Holder(Project)
            {
                func leaf() -> &bool
                {
                    return &self.value;
                }
            }

            func forward<T>(pos value: &T) -> &bool with(T: Project)
            {
                return value(Project).project<u32>(true);
            }

            func bad() -> &bool
            {
                let local = Holder
                {
                    value = true,
                };

                return forward(&local);
            }

            func good(pos value: &Holder) -> &bool
            {
                return forward(value);
            }
        "#,
    );

    let flow = compilation
        .storage_flow(source_function_body_key(&compilation, "bad"))
        .unwrap();

    assert_goal_state_diagnostic_kind(
        flow.diagnostics(),
        DiagnosticKind::CheckingEscapingStorageDependency,
    );

    let flow = compilation
        .storage_flow(source_function_body_key(&compilation, "good"))
        .unwrap();

    assert!(!flow.diagnostics().has_errors(), "{:?}", flow.diagnostics());
}

#[test]
fn abstract_trait_results_instantiate_parameter_defaults() {
    let compilation = compilation(
        r#"
            module app;

            trait Project
            {
                func project() -> &bool;
            }

            struct Holder
            {
                value: bool;
            }

            impl HolderProject = Holder(Project)
            {
                func project() -> &bool
                {
                    return &self.value;
                }
            }

            func choose<T>(pos value: &T, picked: &bool = value.project()) -> &bool with(T: Project)
            {
                return picked;
            }

            func bad() -> &bool
            {
                let local = Holder
                {
                    value = true,
                };

                return choose(&local);
            }

            func good(pos value: &Holder) -> &bool
            {
                return choose(value);
            }
        "#,
    );

    let flow = compilation
        .storage_flow(source_function_body_key(&compilation, "bad"))
        .unwrap();

    assert_goal_state_diagnostic_kind(
        flow.diagnostics(),
        DiagnosticKind::CheckingEscapingStorageDependency,
    );

    let flow = compilation
        .storage_flow(source_function_body_key(&compilation, "good"))
        .unwrap();

    assert!(!flow.diagnostics().has_errors(), "{:?}", flow.diagnostics());
}

#[test]
fn returned_dependencies_do_not_expand_unneeded_recursive_type_arguments() {
    let compilation = compilation(
        r#"
            module app;

            func recurse<T>(pos value: &bool, pos stop: bool) -> &bool
            {
                if stop
                {
                    return value;
                }

                return recurse<(T, T)>(value, true);
            }

            func caller(pos value: &bool) -> &bool
            {
                return recurse<u32>(value, false);
            }
        "#,
    );

    let cancellation = compilation.state.cancellation.clone();

    std::thread::scope(|scope| {
        let (finished, completion) = std::sync::mpsc::channel();

        let cancellation_ref = &cancellation;

        scope.spawn(move || {
            if completion
                .recv_timeout(std::time::Duration::from_secs(3))
                .is_err()
            {
                cancellation_ref.cancel();
            }
        });

        let result = compilation.storage_flow(source_function_body_key(&compilation, "caller"));

        assert!(
            result.is_ok(),
            "ordinary returned dependencies must not expand recursive type arguments: {result:?}"
        );

        let result = result.unwrap();

        assert!(
            !result.diagnostics().has_errors(),
            "{:?}",
            result.diagnostics()
        );

        let _ = finished.send(());
    });
}

#[test]
fn abstract_dependencies_do_not_expand_unneeded_recursive_arguments() {
    for (parameters, initial, nested, mutual) in [
        ("T, U", "u32, Holder", "(T, T), U", false),
        ("const count: usize, U", "0, Holder", "count + 1, U", false),
        ("T, U", "u32, Holder", "(T, T), U", true),
    ] {
        for generic_method in [false, true] {
            let constant = parameters.starts_with("const");

            let method_parameters = match (generic_method, constant) {
                (true, true) => "<const amount: usize>",
                (true, false) => "<V>",
                (false, _) => "",
            };

            let method_arguments = match (generic_method, constant) {
                (true, true) => "<count>",
                (true, false) => "<T>",
                (false, _) => "",
            };

            let step = if mutual {
                r#"
                func step<V, W>(pos value: &W) -> &bool with(W: Project)
                {
                    return recurse<(V, V), W>(value, true);
                }
                "#
            } else {
                ""
            };

            let recursive_call = if mutual {
                "step<(T, T), U>(value)".to_owned()
            } else {
                format!("recurse<{nested}>(value, true)")
            };

            let source = format!(
                r#"
                module app;

                trait Project
                {{
                    func project{method_parameters}() -> &bool;
                }}

                struct Holder
                {{
                    value: bool;
                }}

                impl HolderProject = Holder(Project)
                {{
                    func project{method_parameters}() -> &bool
                    {{
                        return &self.value;
                    }}
                }}

                func recurse<{parameters}>(pos value: &U, pos stop: bool) -> &bool
                    with(U: Project)
                {{
                    if stop
                    {{
                        return value.project{method_arguments}();
                    }}

                    return {recursive_call};
                }}

                {step}

                func caller(pos value: &Holder) -> &bool
                {{
                    return recurse<{initial}>(value, false);
                }}

                func bad() -> &bool
                {{
                    let local = Holder
                    {{
                        value = true,
                    }};

                    return recurse<{initial}>(&local, false);
                }}
            "#
            );

            let compilation = compilation(&source);
            let cancellation = compilation.state.cancellation.clone();

            std::thread::scope(|scope| {
                let (finished, completion) = std::sync::mpsc::channel();

                let cancellation_ref = &cancellation;

                scope.spawn(move || {
                    if completion
                        .recv_timeout(std::time::Duration::from_secs(3))
                        .is_err()
                    {
                        cancellation_ref.cancel();
                    }
                });

                let result = compilation
                    .storage_flow(source_function_body_key(&compilation, "caller"))
                    .expect("abstract dependencies must not expand unused recursive arguments");

                assert!(
                    !result.diagnostics().has_errors(),
                    "{source}: {:?}",
                    result.diagnostics()
                );

                let result = compilation
                    .storage_flow(source_function_body_key(&compilation, "bad"))
                    .unwrap();

                assert_goal_state_diagnostic_kind(
                    result.diagnostics(),
                    DiagnosticKind::CheckingEscapingStorageDependency,
                );

                let _ = finished.send(());
            });
        }
    }
}

#[test]
fn recursive_dependencies_retain_transitive_witness_arguments() {
    let compilation = compilation(
        r#"
            module app;

            trait Project
            {
                func project(pos input: &bool) -> &bool;
            }

            struct Holder
            {
                value: bool;
            }

            struct Forwarder
            {
                value: bool;
            }

            impl HolderProject = Holder(Project)
            {
                func project(pos input: &bool) -> &bool
                {
                    return &self.value;
                }
            }

            impl ForwarderProject = Forwarder(Project)
            {
                func project(pos input: &bool) -> &bool
                {
                    return input;
                }
            }

            func first<T, U>(pos left: &T, pos right: &U, pos input: &bool) -> &bool
                with(T: Project, U: Project)
            {
                return second<U, T>(right, left, input, false);
            }

            func second<V, W>(pos left: &V, pos right: &W, pos input: &bool, pos stop: bool) -> &bool
                with(V: Project, W: Project)
            {
                if stop
                {
                    return left.project(input);
                }

                return first<V, W>(left, right, input);
            }

            func bad(pos forwarder: &Forwarder, pos holder: &Holder) -> &bool
            {
                let local: bool = true;

                return first<Forwarder, Holder>(forwarder, holder, &local);
            }

            func good(pos forwarder: &Forwarder, pos holder: &Holder, pos input: &bool) -> &bool
            {
                return first<Forwarder, Holder>(forwarder, holder, input);
            }
        "#,
    );

    let flow = compilation
        .storage_flow(source_function_body_key(&compilation, "bad"))
        .unwrap();

    assert_goal_state_diagnostic_kind(
        flow.diagnostics(),
        DiagnosticKind::CheckingEscapingStorageDependency,
    );

    let flow = compilation
        .storage_flow(source_function_body_key(&compilation, "good"))
        .unwrap();

    assert!(!flow.diagnostics().has_errors(), "{:?}", flow.diagnostics());
}

#[test]
fn recursive_witness_parameters_follow_the_selected_implementation() {
    let compilation = compilation(
        r#"
            module app;

            trait Leaf
            {
                func leaf() -> &bool;
            }

            trait Project
            {
                func project<T>(pos input: &T) -> &bool with(T: Leaf);
            }

            struct Holder
            {
                value: bool;
            }

            struct Forwarder
            {
                value: bool;
            }

            impl HolderLeaf = Holder(Leaf)
            {
                func leaf() -> &bool
                {
                    return &self.value;
                }
            }

            impl HolderProject = Holder(Project)
            {
                func project<V>(pos input: &V) -> &bool with(V: Leaf)
                {
                    return &self.value;
                }
            }

            impl ForwarderProject = Forwarder(Project)
            {
                func project<W>(pos input: &W) -> &bool with(W: Leaf)
                {
                    return input.leaf();
                }
            }

            func recurse<X, T, U>(pos value: &U, pos input: &T, pos stop: bool) -> &bool
                with(T: Leaf, U: Project)
            {
                if stop
                {
                    return value.project<T>(input);
                }

                return recurse<(X, X), T, U>(value, input, true);
            }

            func good(pos value: &Holder) -> &bool
            {
                let local = Holder
                {
                    value = true,
                };

                return recurse<u32, Holder, Holder>(value, &local, false);
            }

            func bad(pos value: &Forwarder) -> &bool
            {
                let local = Holder
                {
                    value = true,
                };

                return recurse<u32, Holder, Forwarder>(value, &local, false);
            }
        "#,
    );

    let flow = compilation
        .storage_flow(source_function_body_key(&compilation, "good"))
        .unwrap();

    assert!(!flow.diagnostics().has_errors(), "{:?}", flow.diagnostics());

    let flow = compilation
        .storage_flow(source_function_body_key(&compilation, "bad"))
        .unwrap();

    assert_goal_state_diagnostic_kind(
        flow.diagnostics(),
        DiagnosticKind::CheckingEscapingStorageDependency,
    );
}
