use super::fixtures::{compilation, compilation_from_sources, execution_consumer, export};

#[test]
fn imported_construction_defaults_use_the_declaring_type_specialization() {
    let provider = compilation(
        r#"
            module types;

            struct Value<T>
            {
                marker: bool;
                data: T? = none;
            }

            union Choice<T>
            {
                Item(data: T? = none);
            }
        "#,
    );

    assert!(
        provider.check_diagnostics().is_empty(),
        "{:?}",
        provider.check_diagnostics()
    );

    let dependency = super::fixtures::implementation_dependency(export(&provider));

    let consumer = crate::test_support::compilation_with_dependencies(
        r#"
            module app;

            using example.package.types;

            func first<U>() -> example.package.types.Value<U>
            {
                return
                {
                    marker = true
                };
            }

            func second<U>() -> example.package.types.Value<U>
            {
                return
                {
                    marker = true
                };
            }

            func third<U>() -> example.package.types.Choice<U>
            {
                return Item();
            }

            func fourth<U>() -> example.package.types.Choice<U>
            {
                return Item();
            }

            func main()
            {
                _ = first<i32>();
                _ = second<i32>();
                _ = first<bool>();
                _ = third<i32>();
                _ = fourth<i32>();
                _ = third<bool>();
            }
        "#,
        [dependency],
    );

    assert!(
        consumer.check_diagnostics().is_empty(),
        "{:?}",
        consumer.check_diagnostics()
    );

    let result = consumer.codegen_instance_count_for_test(|key| {
        matches!(key, bray_ir::MirUnitKey::ImportedExecutable(_))
    });

    assert_eq!(result.expect("fixture `result` call must succeed"), 4);
}

#[test]
fn qualified_union_case_and_generic_constructor_defaults_export() {
    let compilation = compilation(
        r#"
            module app;

            union Radix
            {
                Decimal;
            }

            union Alignment
            {
                Right;
            }

            union Sign
            {
                NegativeOnly;
            }

            union Escaping
            {
                Raw;
            }

            struct Options
            {
                radix: Radix;
                precision: usize?;
                width: usize;
                alignment: Alignment;
                sign: Sign;
                escaping: Escaping;

                construct(
                    radix: Radix = Radix.Decimal,
                    precision: usize? = none,
                    width: usize = 0,
                    alignment: Alignment = Right,
                    sign: Sign = NegativeOnly,
                    escaping: Escaping = Raw,
                ) -> Self
                {
                    return
                    {
                        radix = radix,
                        precision = precision,
                        width = width,
                        alignment = alignment,
                        sign = sign,
                        escaping = escaping,
                    };
                }
            }

            func options_with_precision(precision: usize) -> Options
            {
                return Options(precision = precision);
            }

            internal func default_options() -> Options
            {
                return Options();
            }

            struct Argument<T>
            {
                value: T;
                options: Options;

                construct(value: T, options: Options = default_options()) -> Self
                {
                    return
                    {
                        value = value,
                        options = options
                    };
                }
            }
        "#,
    );

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    let bundle = export(&compilation);

    assert!(!bundle.semantics().callable_parameter_defaults().is_empty());
}

#[test]
fn runtime_defaults_export_after_disabled_target_gated_contributions() {
    let compilation = compilation_from_sources([
        r#"
            @target(false)
            module app;

            func disabled()
            {
            }
        "#,
        r#"
            module app;

            func selected(pos value: i64? = none) -> i64?
            {
                return value;
            }
        "#,
    ]);

    let bundle = export(&compilation);

    assert_eq!(bundle.semantics().callable_parameter_defaults().len(), 1);
}

#[test]
fn runtime_defaults_cannot_consume_an_earlier_owned_argument() {
    let compilation = compilation(
        r#"
        module app;

        struct Guard
        {
            destruct()
            {
            }
        }

        func accept(pos first: Guard, second: Guard = first)
        {
        }
    "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(),
        bray_diagnostics::DiagnosticKind::CheckingMissingStorageOwnership,
    );
}

#[test]
fn defaulted_call_member_access_infers_source_and_imported_receivers() {
    let declarations = r#"
        struct Flag
        {
            value: bool;
        }

        struct Holder
        {
            flag: Flag;
        }

        @copy
        struct Reference
        {
            holder: &Holder;
        }

        func projected(pos reference: Reference, flag: &Flag = &reference.holder.flag) -> &Flag
        {
            return flag;
        }
    "#;

    let provider = compilation(&format!("module api;\n{declarations}"));
    let mut failures = Vec::new();

    for imported in [false, true] {
        for annotated in [true, false] {
            for direct in [false, true] {
                let prefix = if imported { "example.package.api." } else { "" };

                let holder_annotation = if annotated {
                    format!(": {prefix}Holder")
                } else {
                    String::new()
                };

                let reference_annotation = if annotated {
                    format!(": {prefix}Reference")
                } else {
                    String::new()
                };

                let access = if direct {
                    format!("return {prefix}projected(reference).value;")
                } else {
                    format!("let result = {prefix}projected(reference);\nreturn result.value;")
                };

                let body = format!(
                    r#"
                    func check() -> bool
                    {{
                        let holder{holder_annotation} = {prefix}Holder
                        {{
                            flag = {prefix}Flag
                            {{
                                value = true,
                            }},
                        }};

                        let reference{reference_annotation} = {prefix}Reference
                        {{
                            holder = &holder,
                        }};

                        {access}
                    }}
                "#
                );

                let consumer = if imported {
                    execution_consumer(
                        &provider,
                        &format!("module app;\nusing example.package.api;\n{body}"),
                    )
                } else {
                    compilation(&format!("module api;\n{declarations}\n{body}"))
                };

                let diagnostics = consumer.check_diagnostics();

                if diagnostics.has_errors() {
                    failures.push(format!("imported={imported}, annotated={annotated}, direct={direct}: {diagnostics:?}"));
                }
            }
        }
    }

    assert!(failures.is_empty(), "{failures:#?}");
}
