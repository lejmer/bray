use bray_diagnostics::DiagnosticKind;

use super::fixtures::{compilation, compilation_from_sources, execution_consumer};

#[test]
fn imported_internal_declarations_require_lexical_acknowledgement() {
    for (declaration, parameters, expression, path) in [
        (
            r#"
            internal func hidden() -> i32
            {
                return 1;
            }
            "#,
            "",
            "example.package.api.hidden()",
            "example.package.api.hidden",
        ),
        (
            r#"
            internal static number: i32 = 1;
            "#,
            "",
            "example.package.api.number",
            "example.package.api.number",
        ),
        (
            r#"
            struct Value
            {
                internal data: i32;
            }
            "#,
            "pos value: &example.package.api.Value",
            "value.data",
            "example.package.api.Value.data",
        ),
        (
            r#"
            struct Value
            {
                internal func read<T>(pos input: T) -> i32
                {
                    return 1;
                }
            }
            "#,
            "pos value: &example.package.api.Value",
            "value.read<bool>(true)",
            "example.package.api.Value.read",
        ),
        (
            r#"
            struct Value
            {
                internal static func read() -> i32
                {
                    return 1;
                }
            }
            "#,
            "",
            "example.package.api.Value.read()",
            "example.package.api.Value.read",
        ),
        (
            r#"
            struct Value
            {
                marker: i32;

                internal construct() -> Self
                {
                    return
                    {
                        marker = 0
                    };
                }
            }
            "#,
            "",
            "example.package.api.Value()",
            "example.package.api.Value",
        ),
        (
            r#"
            struct Value
            {
                marker: i32;

                internal construct make() -> Self
                {
                    return
                    {
                        marker = 0
                    };
                }
            }
            "#,
            "",
            "example.package.api.Value.make()",
            "example.package.api.Value.make",
        ),
    ] {
        let provider = compilation(&format!("module api;\n\n{declaration}"));

        assert!(
            provider.check_diagnostics().is_empty(),
            "{declaration}\n{:?}",
            provider.check_diagnostics()
        );

        for (acknowledgement, prefix, valid) in [
            ("".to_owned(), "", false),
            ("".to_owned(), "internal ", true),
            (format!("using internal {path};"), "", true),
        ] {
            let source = format!(
                r#"
                module app;

                using example.package.api;
                {acknowledgement}

                func check({parameters})
                {{
                    let result = {prefix}{expression};
                }}
                "#
            );

            let consumer = execution_consumer(&provider, &source);
            let diagnostics = consumer.check_diagnostics();

            if valid {
                assert!(diagnostics.is_empty(), "{source}\n{diagnostics:?}");
            } else {
                assert!(
                    diagnostics
                        .iter()
                        .any(|diagnostic| diagnostic.kind()
                            == DiagnosticKind::BindingInaccessibleName),
                    "{source}\n{diagnostics:?}"
                );
            }
        }
    }
}

#[test]
fn imported_visibility_is_capped_by_type_and_module_ancestors() {
    for (module, provider) in [
        (
            "api",
            compilation(
                r#"
            module api;

            internal struct Value
            {
                static func read() -> i32
                {
                    return 1;
                }
            }
            "#,
            ),
        ),
        (
            "api",
            compilation(
                r#"
            internal module api;

            struct Value
            {
                static func read() -> i32
                {
                    return 1;
                }
            }
            "#,
            ),
        ),
        (
            "api.nested",
            compilation_from_sources([
                "internal module api;",
                r#"
            module api.nested;

            struct Value
            {
                static func read() -> i32
                {
                    return 1;
                }
            }
            "#,
            ]),
        ),
    ] {
        for (prefix, valid) in [("", false), ("internal ", true)] {
            let source = format!(
                r#"
                module app;

                using internal example.package.{module};

                func check()
                {{
                    let result = {prefix}example.package.{module}.Value.read();
                }}
                "#
            );

            // Ordinary using supplies reachability without acknowledging the internal scopes.
            let source = if valid {
                source
            } else {
                source.replace("using internal", "using")
            };

            let consumer = execution_consumer(&provider, &source);
            let diagnostics = consumer.check_diagnostics();

            if valid {
                assert!(diagnostics.is_empty(), "{source}\n{diagnostics:?}");
            } else {
                assert!(
                    diagnostics
                        .iter()
                        .any(|diagnostic| diagnostic.kind()
                            == DiagnosticKind::BindingInaccessibleName),
                    "{source}\n{diagnostics:?}"
                );
            }
        }
    }
}

#[test]
fn acknowledgement_does_not_cover_sibling_declarations_or_consumers() {
    let provider = compilation(
        r#"
        module api;

        internal func first() -> i32
        {
            return 1;
        }

        internal func second() -> i32
        {
            return 2;
        }
        "#,
    );

    for (expression, valid) in [
        ("example.package.api.first()", true),
        ("example.package.api.second()", false),
    ] {
        let source = format!(
            r#"
            module app;

            using example.package.api;
            using internal example.package.api.first;

            func check()
            {{
                let result = {expression};
            }}
            "#
        );

        let consumer = execution_consumer(&provider, &source);
        let diagnostics = consumer.check_diagnostics();

        assert_eq!(diagnostics.is_empty(), valid, "{source}\n{diagnostics:?}");
    }

    let consumer = execution_consumer(
        &provider,
        r#"
        module app;

        using example.package.api;

        func check()
        {
            let result = example.package.api.first();
        }
        "#,
    );

    assert!(
        consumer
            .check_diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.kind() == DiagnosticKind::BindingInaccessibleName)
    );
}

#[test]
fn acknowledgement_does_not_flow_through_call_or_field_results() {
    let provider = compilation(
        r#"
        module api;

        struct Child
        {
            internal secret: i32;
        }

        struct Value
        {
            internal secret: i32;
            child: Child;
        }

        internal func get() -> Value
        {
            return { secret = 1, child = { secret = 2 } };
        }

        internal static value: Value = { secret = 1, child = { secret = 2 } };

        struct Factory
        {
            internal static func create() -> Value
            {
                return { secret = 1, child = { secret = 2 } };
            }
        }
        "#,
    );

    assert!(provider.check_diagnostics().is_empty());

    for (acknowledged, expression, field) in [
        ("get", "get().secret", "Value.secret"),
        ("value", "value.secret", "Value.secret"),
        ("Factory.create", "Factory.create().secret", "Value.secret"),
        ("value", "value.child.secret", "Child.secret"),
    ] {
        for field_acknowledged in [false, true] {
            let field_using = if field_acknowledged {
                format!("using internal example.package.api.{field};")
            } else {
                String::new()
            };

            let source = format!(
                r#"
                module app;

                using example.package.api;
                using internal example.package.api.{acknowledged};
                {field_using}

                func check() -> i32
                {{
                    return example.package.api.{expression};
                }}
                "#
            );

            let consumer = execution_consumer(&provider, &source);
            let diagnostics = consumer.check_diagnostics();

            if field_acknowledged {
                assert!(diagnostics.is_empty(), "{source}\n{diagnostics:?}");
            } else {
                assert!(
                    diagnostics
                        .iter()
                        .any(|diagnostic| diagnostic.kind()
                            == DiagnosticKind::BindingInaccessibleName),
                    "{source}\n{diagnostics:?}"
                );
            }
        }
    }
}

#[test]
fn inherent_members_use_the_same_type_and_member_acknowledgement() {
    for (declaration, parameters, expression, member) in [
        (
            "internal func read() -> i32 { return 1; }",
            "pos value: &example.package.api.Value",
            "value.read()",
            "Value.read",
        ),
        (
            "internal func read<T>(pos input: T) -> i32 { return 1; }",
            "pos value: &example.package.api.Value",
            "value.read<bool>(true)",
            "Value.read",
        ),
        (
            "internal static func read() -> i32 { return 1; }",
            "",
            "example.package.api.Value.read()",
            "Value.read",
        ),
        (
            "internal construct() -> Self { return { marker = 1 }; }",
            "",
            "example.package.api.Value()",
            "Value",
        ),
        (
            "internal construct make() -> Self { return { marker = 1 }; }",
            "",
            "example.package.api.Value.make()",
            "Value.make",
        ),
    ] {
        let provider = compilation(&format!(
            r#"
            module api;

            struct Value
            {{
                marker: i32;
            }}

            struct Other {{}}

            impl Value
            {{
                {declaration}
            }}
            "#
        ));

        assert!(provider.check_diagnostics().is_empty(), "{declaration}");

        for (path, valid) in [("Other", false), ("Value", true), (member, true)] {
            let source = format!(
                r#"
                module app;

                using example.package.api;
                using internal example.package.api.{path};

                func check({parameters})
                {{
                    let result = {expression};
                }}
                "#
            );

            let consumer = execution_consumer(&provider, &source);
            let diagnostics = consumer.check_diagnostics();

            if valid {
                assert!(diagnostics.is_empty(), "{source}\n{diagnostics:?}");
            } else {
                assert!(
                    diagnostics
                        .iter()
                        .any(|diagnostic| diagnostic.kind()
                            == DiagnosticKind::BindingInaccessibleName),
                    "{source}\n{diagnostics:?}"
                );
            }
        }
    }
}

#[test]
fn source_namespace_reexports_require_consumer_acknowledgement() {
    let provider = compilation(
        r#"
        module api;

        internal func hidden() -> i32
        {
            return 1;
        }
        "#,
    );

    for (acknowledgement, prefix, valid) in [
        ("", "", false),
        ("using internal facade.hidden;", "", true),
        ("", "internal ", true),
    ] {
        let source = format!(
            r#"
            module facade
            {{
                using internal example.package.api.hidden;
                export example.package.api.hidden;
            }}

            module app
            {{
                using facade;
                {acknowledgement}

                func check() -> i32
                {{
                    return {prefix}facade.hidden();
                }}
            }}
            "#
        );

        let consumer = execution_consumer(&provider, &source);
        let diagnostics = consumer.check_diagnostics();

        if valid {
            assert!(diagnostics.is_empty(), "{source}\n{diagnostics:?}");
        } else {
            assert!(
                diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.kind() == DiagnosticKind::BindingInaccessibleName),
                "{source}\n{diagnostics:?}"
            );
        }
    }
}

#[test]
fn internal_reexports_preserve_identity_without_transferring_acknowledgement() {
    let provider = compilation_from_sources([
        r#"
        module original;

        internal func hidden() -> i32
        {
            return 1;
        }

        func visible() -> i32
        {
            return 2;
        }
        "#,
        r#"
        module api;

        using internal original.hidden;
        export original.hidden;
        export original.visible;
        "#,
    ]);

    for (expression, valid) in [
        ("example.package.api.visible()", true),
        ("example.package.api.hidden()", false),
        ("internal example.package.api.hidden()", true),
    ] {
        let source = format!(
            r#"
            module app;

            using example.package.api;

            func check()
            {{
                let result = {expression};
            }}
            "#
        );

        let consumer = execution_consumer(&provider, &source);
        let diagnostics = consumer.check_diagnostics();

        assert_eq!(diagnostics.is_empty(), valid, "{source}\n{diagnostics:?}");
    }
}

#[test]
fn public_defaults_and_generic_bodies_keep_definition_site_internal_references() {
    let provider = compilation(
        r#"
        module api;

        internal const func hidden() -> i32
        {
            return 7;
        }

        func read<T>(pos value: T, pos count: i32 = hidden()) -> i32
        {
            return hidden() + count;
        }
        "#,
    );

    let consumer = execution_consumer(
        &provider,
        r#"
        module app;

        using example.package.api;

        func check() -> i32
        {
            return example.package.api.read<bool>(true);
        }
        "#,
    );

    assert!(
        consumer.check_diagnostics().is_empty(),
        "{:?}",
        consumer.check_diagnostics()
    );
}

#[test]
fn aggregate_field_writes_require_the_same_access_as_field_reads() {
    let provider = compilation(
        r#"
        module api;

        struct Value
        {
            internal data: i32 = 1;
        }
        "#,
    );

    for (acknowledgement, valid) in [
        ("", false),
        ("using internal example.package.api.Value.data;", true),
    ] {
        let source = format!(
            r#"
            module app;

            using example.package.api;
            {acknowledgement}

            func check() -> example.package.api.Value
            {{
                return
                {{
                    data = 2
                }};
            }}
            "#
        );

        let consumer = execution_consumer(&provider, &source);
        let diagnostics = consumer.check_diagnostics();

        assert_eq!(diagnostics.is_empty(), valid, "{source}\n{diagnostics:?}");
    }
}

#[test]
fn available_internal_metadata_does_not_root_unused_executable_templates() {
    let provider = compilation(
        r#"
        module api;

        func visible() -> i32
        {
            return 1;
        }

        internal func hidden() -> i32
        {
            return 2;
        }
        "#,
    );

    let bundle = super::fixtures::export(&provider);

    assert!(
        bundle
            .surface()
            .exports()
            .iter()
            .any(|edge| edge.name().as_str() == "hidden"
                && edge.visibility() == bray_symbols::MemberVisibility::Internal)
    );

    let dependency = super::fixtures::implementation_dependency(bundle);

    let consumer = crate::test_support::compilation_with_dependencies(
        r#"
        module app;

        using example.package.api;

        func main()
        {
            let value = example.package.api.visible();
        }
        "#,
        [dependency],
    );

    assert!(
        consumer.check_diagnostics().is_empty(),
        "{:?}",
        consumer.check_diagnostics()
    );

    assert_eq!(
        consumer
            .codegen_instance_count_for_test(|key| matches!(
                key,
                bray_ir::MirUnitKey::ImportedExecutable(_)
            ))
            .unwrap(),
        1
    );
}

#[test]
fn dependency_reexports_keep_the_defining_visibility_and_identity() {
    use crate::{
        Compilation, CompilationOptions, CompilationRequest, PackageInterfaceExportRequest,
        SelectedTarget, WorkerBudget,
    };

    use bray_package_interface::{
        InterfaceLanguageRevision, InterfaceProductIdentity, InterfaceProductKind,
        PackageInterfaceIdentity,
    };

    use bray_symbols::{PackageIdentity, ProductKind};

    let provider = compilation(
        r#"
        module api;

        internal func hidden() -> i32
        {
            return 1;
        }
        "#,
    );

    let package = PackageIdentity::try_new("proxy.package").unwrap();

    let identity = PackageInterfaceIdentity::try_new(
        package.clone(),
        crate::test_support::package_version(),
        InterfaceProductIdentity::try_new("library").unwrap(),
        InterfaceProductKind::Library,
        "public",
    )
    .unwrap();

    let request = CompilationRequest::with_options(
        package,
        bray_testing::test_source_inputs(
            "proxy",
            [r#"
            module api;

            using internal example.package.api.hidden;
            export example.package.api.hidden;
            "#],
        ),
        CompilationOptions::new(
            WorkerBudget::default(),
            ProductKind::Library,
            SelectedTarget::default(),
        ),
    )
    .with_dependency_interfaces([super::fixtures::execution_dependency(&provider)])
    .with_package_interface_export(PackageInterfaceExportRequest::new(
        identity,
        InterfaceLanguageRevision::new(0),
    ));

    let proxy = Compilation::load(request).unwrap();

    assert!(
        proxy.check_diagnostics().is_empty(),
        "{:?}",
        proxy.check_diagnostics()
    );

    let proxy_bundle = super::fixtures::export(&proxy);

    let [edge] = proxy_bundle.surface().exports() else {
        panic!("proxy must contain exactly its internal re-export");
    };

    assert_eq!(edge.visibility(), bray_symbols::MemberVisibility::Internal);

    assert!(matches!(
        edge.target(),
        bray_package_interface::InterfaceSymbolReference::Dependency { .. }
    ));

    for (acknowledgement, valid) in [("", false), ("internal ", true)] {
        let source = format!(
            r#"
            module app;

            using proxy.package.api;

            func check()
            {{
                let result = {acknowledgement}proxy.package.api.hidden();
            }}
            "#
        );

        let consumer = crate::test_support::compilation_with_dependencies(
            &source,
            [
                super::fixtures::execution_dependency(&provider),
                super::fixtures::execution_dependency(&proxy),
            ],
        );

        let diagnostics = consumer.check_diagnostics();

        assert_eq!(diagnostics.is_empty(), valid, "{source}\n{diagnostics:?}");
    }
}

#[test]
fn inaccessible_trait_candidates_do_not_make_public_member_selection_ambiguous() {
    let provider = compilation(
        r#"
        module api;

        struct Value
        {
        }

        trait Visible
        {
            func read() -> i32;
        }

        internal trait Hidden
        {
            func read() -> i32;
        }

        impl VisibleImpl = Value(Visible)
        {
            func read() -> i32
            {
                return 1;
            }
        }

        internal impl HiddenImpl = Value(Hidden)
        {
            func read() -> i32
            {
                return 2;
            }
        }
        "#,
    );

    assert!(
        provider.check_diagnostics().is_empty(),
        "{:?}",
        provider.check_diagnostics()
    );

    for expression in [
        "value.read()",
        "value(internal example.package.api.Hidden).read()",
    ] {
        let source = format!(
            r#"
            module app;

            using example.package.api;
            using example.package.api.VisibleImpl;
            using internal example.package.api.HiddenImpl;

            func check(pos value: &example.package.api.Value) -> i32
            {{
                return {expression};
            }}
            "#
        );

        let consumer = execution_consumer(&provider, &source);
        let diagnostics = consumer.check_diagnostics();

        assert!(diagnostics.is_empty(), "{source}\n{diagnostics:?}");
    }
}

#[test]
fn destructuring_requires_the_same_field_acknowledgement_as_selection() {
    let provider = compilation(
        r#"
        module api;

        struct Value
        {
            internal data: i32;
        }
        "#,
    );

    for (acknowledgement, valid) in [
        ("", false),
        ("using internal example.package.api.Value.data;", true),
    ] {
        let source = format!(
            r#"
            module app;

            using example.package.api;
            {acknowledgement}

            func check(pos value: example.package.api.Value) -> i32
            {{
                let {{ data }} = value;

                return data;
            }}
            "#
        );

        let consumer = execution_consumer(&provider, &source);
        let diagnostics = consumer.check_diagnostics();

        assert_eq!(diagnostics.is_empty(), valid, "{source}\n{diagnostics:?}");

        if !valid {
            assert!(
                diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.kind() == DiagnosticKind::BindingInaccessibleName)
            );
        }
    }
}
