use bray_diagnostics::DiagnosticKind;

use super::fixtures::{compilation, compilation_from_sources, execution_consumer};

const API: &str = r#"
module api
{
    const expected: i32 = 1;

    struct Value
    {
        number: i32;
    }

    const func positive(pos value: i32) -> bool
    {
        return value > 0;
    }

    func read() -> i32
    {
        return 1;
    }
}

module api.child
{
    func read() -> i32
    {
        return 2;
    }
}

module api2
{
    func read() -> i32
    {
        return 3;
    }
}
"#;

#[test]
fn source_and_imported_namespace_paths_require_matching_using() {
    let provider = compilation(API);

    assert!(provider.check_diagnostics().is_empty());

    for imported in [false, true] {
        let prefix = if imported { "example.package." } else { "" };

        for (using, reference, valid) in [
            ("", "api.read", false),
            ("api", "api.read", true),
            ("api.read", "api.read", true),
            ("api.child", "api.read", false),
            ("api2", "api.read", false),
            ("api", "api2.read", false),
            ("api", "api.child.read", true),
            ("api.read", "api.child.read", false),
            ("api.child.read", "api.child.read", true),
        ] {
            let declaration = if using.is_empty() {
                String::new()
            } else {
                format!("using {prefix}{using};")
            };

            let source = format!(
                r#"
                module app;

                {declaration}

                func run() -> i32
                {{
                    return {prefix}{reference}();
                }}
                "#
            );

            let consumer = if imported {
                execution_consumer(&provider, &source)
            } else {
                compilation_from_sources([API, &source])
            };

            let diagnostics = consumer.check_diagnostics();

            assert_eq!(diagnostics.is_empty(), valid, "{source}\n{diagnostics:?}");

            if !valid {
                assert!(
                    diagnostics.iter().any(|diagnostic| {
                        diagnostic.kind() == DiagnosticKind::BindingUndeclaredDependency
                    }),
                    "{source}\n{diagnostics:?}"
                );
            }
        }
    }
}

#[test]
fn package_using_covers_its_descendant_modules() {
    let provider = compilation(API);

    let consumer = execution_consumer(
        &provider,
        r#"
        module app;

        using example.package;

        func run() -> i32
        {
            return example.package.api.read() + example.package.api2.read();
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
fn type_and_contract_paths_obey_the_same_dependency_policy() {
    let provider = compilation(API);

    for imported in [false, true] {
        let prefix = if imported { "example.package." } else { "" };

        for declared in [false, true] {
            let using = if declared {
                format!("using {prefix}api;")
            } else {
                String::new()
            };

            let source = format!(
                r#"
                module app;

                {using}

                func run(pos value: &{prefix}api.Value) -> i32
                    requires({prefix}api.positive(value.number))
                {{
                    return {prefix}api.read();
                }}
                "#
            );

            let consumer = if imported {
                execution_consumer(&provider, &source)
            } else {
                compilation_from_sources([API, &source])
            };

            let diagnostics = consumer.check_diagnostics();

            assert_eq!(
                diagnostics.is_empty(),
                declared,
                "{source}\n{diagnostics:?}"
            );

            if !declared {
                assert!(diagnostics.iter().any(|diagnostic| {
                    diagnostic.kind() == DiagnosticKind::BindingUndeclaredDependency
                }));
            }
        }
    }
}

#[test]
fn split_contributions_share_dependencies_but_other_modules_do_not() {
    for (module, valid) in [("app", true), ("app.child", false), ("other", false)] {
        let contribution = format!(
            r#"
            module {module}
            {{
                func run() -> i32
                {{
                    return api.read();
                }}
            }}
            "#
        );

        let consumer =
            compilation_from_sources([API, "module app; using api;", contribution.as_str()]);

        let diagnostics = consumer.check_diagnostics();

        assert_eq!(diagnostics.is_empty(), valid, "{module}\n{diagnostics:?}");
    }
}

#[test]
fn disabled_contributions_do_not_supply_dependencies() {
    for enabled in [false, true] {
        let dependency = format!("@target({enabled}) module app; using api;");

        let consumer = compilation_from_sources([
            API,
            dependency.as_str(),
            "module app; func run() -> i32 { return api.read(); }",
        ]);

        let diagnostics = consumer.check_diagnostics();

        assert_eq!(diagnostics.is_empty(), enabled, "{diagnostics:?}");

        if !enabled {
            assert!(diagnostics.iter().any(|diagnostic| {
                diagnostic.kind() == DiagnosticKind::BindingUndeclaredDependency
            }));
        }
    }
}

#[test]
fn qualified_same_module_references_need_no_self_import() {
    let consumer = compilation_from_sources([
        "module app.deep; func read() -> i32 { return 1; }",
        "module app.deep; func run() -> i32 { return app.deep.read(); }",
    ]);

    assert!(
        consumer.check_diagnostics().is_empty(),
        "{:?}",
        consumer.check_diagnostics()
    );
}

#[test]
fn internal_and_trusted_access_do_not_supply_dependency_declarations() {
    for (using, internal, valid) in [
        ("", "", false),
        ("", "internal ", false),
        ("using api;", "internal ", true),
        ("using internal api;", "", true),
    ] {
        let source = format!(
            r#"
            trusted module app;

            {using}

            trusted func run() -> i32
            {{
                return {internal}api.read();
            }}
            "#
        );

        let consumer = compilation_from_sources([
            "module api; internal func read() -> i32 { return 1; }",
            &source,
        ]);

        let diagnostics = consumer.check_diagnostics();

        assert_eq!(diagnostics.is_empty(), valid, "{source}\n{diagnostics:?}");

        if !valid {
            assert!(diagnostics.iter().any(|diagnostic| {
                diagnostic.kind() == DiagnosticKind::BindingUndeclaredDependency
            }));
        }
    }
}

#[test]
fn reexports_and_definition_site_bodies_do_not_import_provider_dependencies() {
    let provider_source = r#"
        module helper
        {
            func value() -> i32
            {
                return 1;
            }
        }

        module api
        {
            using helper;

            export helper.value;

            func read() -> i32
            {
                return helper.value();
            }
        }
    "#;

    let provider = compilation(provider_source);

    assert!(provider.check_diagnostics().is_empty());

    for imported in [false, true] {
        let prefix = if imported { "example.package." } else { "" };

        for (reference, valid) in [
            ("api.value", true),
            ("api.read", true),
            ("helper.value", false),
        ] {
            let source = format!(
                r#"
                module app;

                using {prefix}api;

                func run() -> i32
                {{
                    return {prefix}{reference}();
                }}
                "#
            );

            let consumer = if imported {
                execution_consumer(&provider, &source)
            } else {
                compilation_from_sources([provider_source, &source])
            };

            let diagnostics = consumer.check_diagnostics();

            assert_eq!(diagnostics.is_empty(), valid, "{source}\n{diagnostics:?}");

            if !valid {
                assert!(diagnostics.iter().any(|diagnostic| {
                    diagnostic.kind() == DiagnosticKind::BindingUndeclaredDependency
                }));
            }
        }
    }
}

#[test]
fn exports_require_their_own_dependency_declaration() {
    for using in ["", "using api;"] {
        let source = format!("module facade; {using} export api.read;");
        let consumer = compilation_from_sources([API, &source]);
        let diagnostics = consumer.check_diagnostics();

        assert_eq!(diagnostics.is_empty(), !using.is_empty(), "{diagnostics:?}");

        if using.is_empty() {
            assert!(diagnostics.iter().any(|diagnostic| {
                diagnostic.kind() == DiagnosticKind::BindingUndeclaredDependency
            }));
        }
    }
}

#[test]
fn named_core_modules_need_using_but_ambient_names_do_not() {
    for using in ["", "using core.memory;"] {
        let source = format!(
            "module app; {using} func pointer() -> DataPointer<i32> {{ return core.memory.null<i32>(); }}"
        );

        let consumer = compilation(&source);
        let diagnostics = consumer.check_diagnostics();

        assert_eq!(diagnostics.is_empty(), !using.is_empty(), "{diagnostics:?}");
    }

    let consumer =
        compilation("module app; func read(pos value: i32) -> bool { return value > 0; }");

    assert!(consumer.check_diagnostics().is_empty());
}

#[test]
fn named_patterns_and_trait_projections_require_declared_namespaces() {
    let traits = r#"
        module api;

        trait Provides
        {
            type Item;
        }

        struct Value
        {
        }

        impl Provided = Value(Provides)
        {
            type Item = i32;
        }
    "#;

    for imported in [false, true] {
        let prefix = if imported { "example.package." } else { "" };

        for declared in [false, true] {
            let using = if declared {
                format!("using {prefix}api;")
            } else {
                String::new()
            };

            let patterns = format!(
                r#"
                module app;

                {using}

                func run(pos value: i32) -> i32
                {{
                    match value
                    {{
                        case {prefix}api.expected
                        {{
                            return 1;
                        }}

                        case _
                        {{
                            return 0;
                        }}
                    }}
                }}
                "#
            );

            let projection = format!(
                r#"
                module app;

                using {prefix}api.Provided;
                {using}

                struct Holder
                {{
                    item: {prefix}api.Value({prefix}api.Provides).Item;
                }}
                "#
            );

            for (provider_source, source) in [(API, &patterns), (traits, &projection)] {
                let provider = compilation(provider_source);

                let consumer = if imported {
                    execution_consumer(&provider, source)
                } else {
                    compilation_from_sources([provider_source, source])
                };

                let diagnostics = consumer.check_diagnostics();

                assert_eq!(
                    diagnostics.is_empty(),
                    declared,
                    "{source}\n{diagnostics:?}"
                );

                if !declared {
                    assert!(diagnostics.iter().any(|diagnostic| {
                        diagnostic.kind() == DiagnosticKind::BindingUndeclaredDependency
                    }));
                }
            }
        }
    }
}

#[test]
fn reexported_type_paths_use_the_same_source_and_imported_lookup() {
    let provider_source = r#"
        module model
        {
            struct Value
            {
                number: i32;
            }
        }

        module api
        {
            using model;

            export model.Value;
        }
    "#;

    let provider = compilation(provider_source);

    assert!(provider.check_diagnostics().is_empty());

    for imported in [false, true] {
        let prefix = if imported { "example.package." } else { "" };

        let source = format!(
            r#"
            module app;

            using {prefix}api.Value;

            func read(pos value: &{prefix}api.Value) -> i32
            {{
                return value.number;
            }}
            "#
        );

        let consumer = if imported {
            execution_consumer(&provider, &source)
        } else {
            compilation_from_sources([provider_source, &source])
        };

        assert!(
            consumer.check_diagnostics().is_empty(),
            "{:?}",
            consumer.check_diagnostics()
        );
    }
}
