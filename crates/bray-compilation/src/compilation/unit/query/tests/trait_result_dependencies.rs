use crate::test_support::{compilation, source_function_body_key};
use bray_diagnostics::DiagnosticKind;
use bray_testing::assert_goal_state_diagnostic_kind;

#[test]
fn abstract_trait_results_reject_local_storage_escape() {
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

            func forward<T>(pos value: &T) -> &bool with(T: Project)
            {
                return value.project();
            }

            func bad() -> &bool
            {
                let local = Holder
                {
                    value = true,
                };

                return forward(&local);
            }
        "#,
    );

    let key = source_function_body_key(&compilation, "bad");

    let flow = compilation
        .storage_flow(key)
        .expect("storage flow must publish");

    assert_goal_state_diagnostic_kind(
        flow.diagnostics(),
        DiagnosticKind::CheckingEscapingStorageDependency,
    );
}

#[test]
fn abstract_trait_results_resolve_local_inputs_with_the_selected_witness() {
    for (result, implementation, valid) in [
        ("bool", "return self.value;", true),
        ("&bool", "return input;", false),
    ] {
        for call in ["value.project(&local)", "value.via_default()"] {
            let source = format!(
                r#"
                module app;

                trait Project<R>
                {{
                    func project(pos input: &bool) -> R;

                    func via_default() -> R
                    {{
                        let local: bool = true;

                        return self.project(&local);
                    }}
                }}

                struct Holder
                {{
                    value: bool;
                }}

                impl HolderProject = Holder(Project< {result}>)
                {{
                    func project(pos input: &bool) -> {result}
                    {{
                        {implementation}
                    }}
                }}

                func forward<T, R>(pos value: &T) -> R with(T: Project<R>)
                {{
                    let local: bool = true;

                    return {call};
                }}

                func caller(pos value: &Holder) -> {result}
                {{
                    return forward<Holder, {result}>(value);
                }}
            "#
            );

            let compilation = compilation(&source);

            assert!(
                !compilation.syntax_tree_result().diagnostics().has_errors(),
                "{source}: {:?}",
                compilation.syntax_tree_result().diagnostics()
            );

            let generic = compilation
                .storage_flow(source_function_body_key(&compilation, "forward"))
                .unwrap();

            assert!(
                !generic.diagnostics().has_errors(),
                "{source}: {:?}",
                generic.diagnostics()
            );

            let diagnostics = compilation.check_diagnostics();

            if valid {
                assert!(!diagnostics.has_errors(), "{source}: {:?}", diagnostics);
            } else {
                assert_goal_state_diagnostic_kind(
                    &diagnostics,
                    DiagnosticKind::CheckingEscapingStorageDependency,
                );
            }
        }
    }
}

#[test]
fn abstract_trait_results_preserve_mutable_and_aggregate_dependencies() {
    for (result, receiver, field, body, borrow) in [
        (
            "&mut bool",
            "mut ",
            "mut ",
            "return &mut self.value;",
            "&mut ",
        ),
        ("&view Marker", "", "", "return &self;", "&"),
        (
            "HolderRef",
            "",
            "",
            r#"return HolderRef
                    {
                        value = &self.value,
                    };"#,
            "&",
        ),
    ] {
        let source = format!(
            r#"
                module app;

                struct HolderRef
                {{
                    value: &bool;
                }}

                trait Marker
                {{
                }}

                impl HolderMarker = Holder(Marker)
                {{
                }}

                trait Project
                {{
                    {receiver}func project() -> {result};
                }}

                struct Holder
                {{
                    {field}value: bool;
                }}

                impl HolderProject = Holder(Project)
                {{
                    {receiver}func project() -> {result}
                    {{
                        {body}
                    }}
                }}

                func forward<T>(pos value: {borrow}T) -> {result} with(T: Project)
                {{
                    return value.project();
                }}

                func bad() -> {result}
                {{
                    let mut local = Holder
                    {{
                        value = true,
                    }};

                    return forward({borrow}local);
                }}

                func good(pos value: {borrow}Holder) -> {result}
                {{
                    return forward(value);
                }}
            "#
        );

        let compilation = compilation(&source);

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

        assert!(
            !flow.diagnostics().has_errors(),
            "{source}: {:?}",
            flow.diagnostics()
        );
    }
}

#[test]
fn abstract_trait_results_preserve_reborrowed_fields() {
    let compilation = compilation(
        r#"
            module app;

            struct Inner
            {
                value: bool;
            }

            trait Project
            {
                func project() -> &Inner;
            }

            struct Holder
            {
                inner: Inner;
            }

            impl HolderProject = Holder(Project)
            {
                func project() -> &Inner
                {
                    return &self.inner;
                }
            }

            func forward<T>(pos value: &T) -> &bool with(T: Project)
            {
                return &value.project().value;
            }

            func bad() -> &bool
            {
                let local = Holder
                {
                    inner = Inner
                    {
                        value = true,
                    },
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
fn abstract_trait_results_substitute_generic_members() {
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

            trait Forward
            {
                func forward<U>(pos value: &U) -> &bool with(U: Project);
            }

            struct Adapter
            {
            }

            impl AdapterForward = Adapter(Forward)
            {
                func forward<U>(pos value: &U) -> &bool with(U: Project)
                {
                    return value.project();
                }
            }

            func forward<T, U>(pos adapter: &T, pos value: &U) -> &bool
                with(T: Forward, U: Project)
            {
                return adapter.forward(value);
            }

            func bad() -> &bool
            {
                let adapter = Adapter
                {
                };
                let local = Holder
                {
                    value = true,
                };

                return forward(&adapter, &local);
            }

            func good(pos value: &Holder) -> &bool
            {
                let adapter = Adapter
                {
                };

                return forward(&adapter, value);
            }
        "#,
    );

    assert!(!compilation.syntax_tree_result().diagnostics().has_errors());

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
fn abstract_trait_results_converge_through_recursive_generic_composition() {
    let source = r#"
            module app;

            trait Step
            {
                func step() -> &Self;
            }

            struct Node
            {
                value: bool;
            }

            impl NodeStep = Node(Step)
            {
                func step() -> &Self
                {
                    return &self;
                }
            }

            func good(pos value: &Node) -> &Node
            {
                return walk<Node>(value, 2);
            }

            func bad() -> &Node
            {
                let local = Node
                {
                    value = true,
                };

                return walk<Node>(&local, 2);
            }

            func walk<T>(pos value: &T, pos count: u32) -> &T with(T: Step)
            {
                if count == 0
                {
                    return value;
                }

                return walk<T>(value.step(), count - 1);
            }
        "#;

    let loop_source = source.replace(
        "return walk<T>(value.step(), count - 1);",
        r#"let mut current: &T = value;
                let mut remaining = count;

                while remaining > 0
                {
                    current = current.step();
                    remaining = remaining - 1;
                }

                return current;"#,
    );

    for source in [source.to_owned(), loop_source] {
        let compilation = compilation(&source);
        let callable = crate::test_support::source_function(&compilation, "walk");
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

            let context = compilation.binding_context(&cancellation).unwrap();

            let result =
                    bray_binder::SymbolQueryProvider::resolve_symbol_query(
                        &context,
                        bray_symbols::SymbolQueryRequest::<
                            bray_symbols::CallableResultDependenciesQuery,
                        >::new(callable.into()),
                    );

            assert!(
                result.is_ok(),
                "recursive generic result must converge: {result:?}"
            );

            let result = result.unwrap();

            assert!(
                !result.diagnostics().has_errors(),
                "{:?}",
                result.diagnostics()
            );

            let contract = compilation
                .semantic_value_store()
                .unwrap()
                .dependency_contract_template_data(*result.value());

            assert!(!contract.requirements().is_empty());

            let bad = compilation
                .storage_flow(source_function_body_key(&compilation, "bad"))
                .unwrap();

            assert_goal_state_diagnostic_kind(
                bad.diagnostics(),
                DiagnosticKind::CheckingEscapingStorageDependency,
            );

            let good = compilation
                .storage_flow(source_function_body_key(&compilation, "good"))
                .unwrap();

            assert!(!good.diagnostics().has_errors(), "{:?}", good.diagnostics());

            let _ = finished.send(());
        });
    }
}
