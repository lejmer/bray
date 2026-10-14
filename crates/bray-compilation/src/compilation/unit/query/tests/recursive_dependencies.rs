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

#[test]
fn recursive_reference_summaries_require_a_normal_base_and_agreeing_targets() {
    let application = compilation(
        r#"
        module app;
        func direct(pos value: &bool, pos repeat: bool) -> &bool {
            if repeat { return direct(value, false); }
            return value;
        }
        func first(pos value: &bool, pos repeat: bool) -> &bool {
            if repeat { return second(value, false); }
            return value;
        }
        func second(pos value: &bool, pos repeat: bool) -> &bool {
            return first(value, repeat);
        }
        func different(pos value: &bool, pos other: &bool, pos repeat: bool) -> &bool {
            if repeat { return different(other, value, false); }
            return value;
        }
        func forever(pos value: &bool) -> &bool { return forever(value); }
        "#,
    );

    for name in ["direct", "first", "second", "different", "forever"] {
        let dependencies = crate::compilation::result_dependencies::source_function_dependencies(
            &application,
            name,
        );

        assert_eq!(
            dependencies.normal_completion(),
            name != "forever",
            "{name}: {dependencies:#?}"
        );

        let origins = dependencies
            .borrows()
            .iter()
            .filter(|relation| {
                relation.exit().subject_root() == bray_symbols::DependencySubjectRoot::Result
            })
            .collect::<Vec<_>>();

        if matches!(name, "direct" | "first" | "second") {
            assert_eq!(origins.len(), 1, "{name}: {dependencies:#?}");

            assert_eq!(
                origins[0].entry().subject_root(),
                bray_symbols::DependencySubjectRoot::Parameter(bray_symbols::SymbolOrdinal::new(0))
            );
        } else {
            assert!(origins.is_empty(), "{name}: {dependencies:#?}");
        }
    }
}

#[test]
fn deferred_results_keep_exact_borrow_origins_and_defaults_execute_at_creation() {
    let application = compilation(
        r#"
        module app;
        async func forward(pos value: &bool) -> &bool { return value; }
        async func wrapper(pos value: &bool) -> &bool {
            let future = forward(value);
            return await future;
        }
        func forever() -> i32 { return forever(); }
        async func work(value: i32 = forever()) {}
        func create() { let future = work(); }
    "#,
    );

    let diagnostics = application.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:#?}");

    let result = crate::compilation::result_dependencies::source_function_dependencies(
        &application,
        "wrapper",
    );

    let origins = result
        .borrows()
        .iter()
        .filter(|relation| {
            relation.exit().subject_root() == bray_symbols::DependencySubjectRoot::Result
        })
        .collect::<Vec<_>>();

    assert_eq!(origins.len(), 1, "{result:#?}");

    assert_eq!(
        origins[0].entry().subject_root(),
        bray_symbols::DependencySubjectRoot::Parameter(bray_symbols::SymbolOrdinal::new(0))
    );

    let creation = crate::compilation::result_dependencies::source_function_dependencies(
        &application,
        "create",
    );

    assert!(!creation.normal_completion(), "{creation:#?}");
}

#[test]
fn awaiting_a_future_consumes_it_and_unawaited_frames_remain_owned() {
    let invalid = compilation(
        r#"
        module app;
        async func child() {}
        async func caller() { let pending = child(); await pending; await pending; }
    "#,
    );

    let diagnostics = invalid.check_diagnostics();

    bray_testing::assert_goal_state_diagnostic_kind(
        diagnostics,
        bray_diagnostics::DiagnosticKind::CheckingUseOfMovedStorage,
    );

    let valid = compilation(
        r#"
        module app;
        async func child() {}
        func caller() { let pending = child(); }
    "#,
    );

    assert!(
        !valid.check_diagnostics().has_errors(),
        "{:?}",
        valid.check_diagnostics()
    );

    let key = crate::test_support::source_function_body_key(&valid, "caller");
    let analysis = valid.async_analysis(key).unwrap();

    assert!(
        analysis
            .value()
            .scope_exits()
            .iter()
            .any(|exit| !exit.lifecycle_resolution().is_empty())
    );
}

#[test]
fn deferred_and_scoped_calls_without_normal_returns_do_not_create_result_origins() {
    let application = compilation(
        r#"
        module app;
        func forever(pos value: &bool) -> &bool { return forever(value); }
        async func delayed(pos value: &bool) -> &bool { return forever(value); }
        async func awaiting(pos value: &bool) -> &bool { return await delayed(value); }
        struct Resource { value: bool; }
        impl Resource {
            enter() -> &bool { return forever(&self.value); }
            exit(pos lease: &bool) {}
        }
        func entering(pos resource: &Resource) -> &bool {
            return with lease = resource { yield lease; };
        }
        struct Leaving { value: bool; }
        impl Leaving {
            enter() -> &bool { return &self.value; }
            exit(pos lease: &bool) { let _: &bool = forever(lease); }
        }
        func exiting(pos resource: &Leaving) -> &bool {
            return with lease = resource { yield lease; };
        }
    "#,
    );

    let diagnostics = application.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:#?}");

    for name in ["delayed", "awaiting", "entering", "exiting"] {
        let result = crate::compilation::result_dependencies::source_function_dependencies(
            &application,
            name,
        );

        assert!(!result.normal_completion(), "{name}: {result:#?}");
        assert!(result.borrows().is_empty(), "{name}: {result:#?}");

        let lowered = application
            .lowered_unit(source_function_body_key(&application, name))
            .unwrap();

        assert!(lowered.value().is_some(), "{name}: {lowered:#?}");
    }
}

#[test]
fn recovered_inline_type_cycles_do_not_expand_reference_demands() {
    let compilation = compilation(
        r#"
        module app;
        struct Recursive { next: Recursive; pointer: &u8; }
        func select(pos value: Recursive) -> &u8 {
            let child = value.next;
            return child.pointer;
        }
    "#,
    );

    let diagnostics = compilation.check_diagnostics();

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic.kind()
            == bray_diagnostics::DiagnosticKind::CheckingRecursiveTypeRepresentation),
        "{diagnostics:?}"
    );
}

#[test]
fn starting_a_task_transfers_the_future_borrow_until_join_completes() {
    for (body, valid) in [
        (
            "let task = work(&value).start(); value.number = 1; try await task.join();",
            false,
        ),
        (
            "let task = work(&value).start(); try await task.join(); value.number = 1;",
            true,
        ),
    ] {
        let application = compilation(&format!(
            r#"
            module app;
            struct Cell {{ mut number: u32; }}
            async func work(pos value: &Cell) -> u32 {{ return value.number; }}
            async func caller() {{ let mut value: Cell = {{ number = 0 }}; {body} }}
        "#
        ));

        let diagnostics = application.check_diagnostics();

        if valid {
            assert!(!diagnostics.has_errors(), "{body}: {diagnostics:?}");
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                bray_diagnostics::DiagnosticKind::CheckingConflictingBorrow,
            );
        }
    }
}
