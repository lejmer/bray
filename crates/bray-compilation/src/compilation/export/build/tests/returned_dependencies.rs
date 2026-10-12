use crate::test_support::source_function_body_key;

use super::fixtures::{compilation, execution_consumer};

#[test]
fn imported_call_effects_distinguish_forwarding_from_mutation() {
    let provider = compilation(
        r#"
        module api;
        struct Cell { mut value: bool; }
        func same<T>(pos value: &T) -> &T { return value; }
        func rewrite(pos cell: &mut Cell) { cell.value = false; }
    "#,
    );

    for (body, unchanged) in [
        (
            "func caller(pos cell: &example.package.api.Cell) -> &example.package.api.Cell { return example.package.api.same(cell); }",
            true,
        ),
        (
            "func caller(pos cell: &mut example.package.api.Cell) { example.package.api.rewrite(cell); }",
            false,
        ),
    ] {
        let source = format!(
            "module app; using example.package.api.Cell; using example.package.api.same; using example.package.api.rewrite; {body}"
        );

        let consumer = execution_consumer(&provider, &source);

        let contracts = consumer
            .dependency_contracts(source_function_body_key(&consumer, "caller"))
            .unwrap();

        assert!(
            !contracts.diagnostics().has_errors(),
            "{:?}",
            contracts.diagnostics()
        );

        let calls = contracts.value().calls();

        assert_eq!(calls.len(), 1);

        let call = calls.values().next().unwrap();

        assert_eq!(call.effects().is_empty(), unchanged, "{:?}", call.effects());

        if !unchanged {
            assert!(
                call.effects().iter().any(|effect| matches!(
                    effect,
                    bray_symbols::DependencyStorageEffect::Indirect {
                        kind: bray_symbols::DependencyMutationKind::Write,
                        ..
                    }
                )),
                "the imported body retains its specific write target: {:?}",
                call.effects()
            );
        }
    }
}

#[test]
fn returned_values_preserve_imported_generic_dependencies() {
    let provider = compilation(
        r#"
            module api;

            func same<T>(pos value: &T) -> &T
            {
                return value;
            }
        "#,
    );

    for (body, valid) in [
        (
            r#"
                func caller(pos value: &bool) -> &bool
                {
                    return example.package.api.same(value);
                }
            "#,
            true,
        ),
        (
            r#"
                func caller() -> &bool
                {
                    let value: bool = true;

                    return example.package.api.same(&value);
                }
            "#,
            false,
        ),
    ] {
        let source = format!(
            r#"
                module app;

                using example.package.api.same;

                {body}
            "#
        );

        let consumer = execution_consumer(&provider, &source);

        let flow = consumer
            .storage_flow(source_function_body_key(&consumer, "caller"))
            .unwrap();

        if valid {
            assert!(!flow.diagnostics().has_errors(), "{:?}", flow.diagnostics());
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                flow.diagnostics(),
                bray_diagnostics::DiagnosticKind::CheckingEscapingStorageDependency,
            );
        }
    }
}

#[test]
fn returned_assignments_and_errors_survive_interfaces() {
    let provider = compilation(
        r#"
            module api;

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

            func forward(pos input: Result<bool, &bool>) -> Result<bool, &bool>
            {
                let value = try input;

                return Ok(value);
            }
        "#,
    );

    for (result, call) in [
        (
            "example.package.api.Holder",
            "example.package.api.replace(caller, argument)",
        ),
        (
            "Result<bool, &bool>",
            "example.package.api.forward(Error(argument))",
        ),
    ] {
        for (argument, valid) in [("caller", true), ("&local", false)] {
            let call = call.replace("argument", argument);

            let source = format!(
                r#"
                    module app;

                    using example.package.api;

                    func caller(pos caller: &bool) -> {result}
                    {{
                        let local: bool = true;

                        return {call};
                    }}
                "#
            );

            let consumer = execution_consumer(&provider, &source);

            let flow = consumer
                .storage_flow(source_function_body_key(&consumer, "caller"))
                .unwrap();

            if valid {
                assert!(
                    consumer.check_diagnostics().is_empty(),
                    "{:?}",
                    consumer.check_diagnostics()
                );
            } else {
                bray_testing::assert_goal_state_diagnostic_kind(
                    flow.diagnostics(),
                    bray_diagnostics::DiagnosticKind::CheckingEscapingStorageDependency,
                );
            }
        }
    }
}

#[test]
fn returned_borrow_alias_assignments_survive_interfaces() {
    let provider = compilation(
        r#"
            module api;

            func same<T>(pos value: &mut T) -> &mut T
            {
                return value;
            }
        "#,
    );

    for (argument, valid) in [("caller", true), ("&local", false)] {
        let source = format!(
            r#"
                module app;

                using example.package.api;

                struct Holder
                {{
                    mut value: &bool;
                }}

                func replace(pos mut result: Holder, pos second: &bool) -> Holder
                {{
                    let owner = example.package.api.same(&mut result);
                    owner.value = second;

                    return result;
                }}

                func caller(pos caller: &bool) -> Holder
                {{
                    let local: bool = true;

                    return replace(Holder {{ value = caller }}, {argument});
                }}
            "#
        );

        let consumer = execution_consumer(&provider, &source);
        let diagnostics = consumer.check_diagnostics();

        if valid {
            assert!(diagnostics.is_empty(), "{diagnostics:?}");
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                &diagnostics,
                bray_diagnostics::DiagnosticKind::CheckingEscapingStorageDependency,
            );
        }
    }
}

#[test]
fn implicit_call_reborrows_preserve_source_and_imported_parameter_modes() {
    let declaration = r#"
func touch<T>(pos value: &mut T)
{
}

func take<T>(pos value: T)
{
}

func same(pos value: &mut bool) -> &mut bool
{
    return value;
}
"#;

    let provider = compilation(&format!("module api;\n{declaration}"));

    assert!(
        !provider.check_diagnostics().has_errors(),
        "{:?}",
        provider.check_diagnostics()
    );

    let moved = bray_diagnostics::DiagnosticKind::CheckingUseOfMovedStorage;
    let conflict = bray_diagnostics::DiagnosticKind::CheckingConflictingBorrow;

    for (body, expected) in [
        (
            r#"let local: &mut bool = caller;

    touch(local);
    touch(local);"#,
            None,
        ),
        (
            r#"let local: &mut bool = same(caller);

    touch(local);
    touch(local);"#,
            None,
        ),
        (
            r#"let local: &mut bool = caller;

    take(local);
    touch(local);"#,
            Some(moved),
        ),
        (
            r#"let local: &mut bool = caller;
    let escaped: &mut bool = same(local);

    touch(local);
    touch(escaped);"#,
            Some(conflict),
        ),
    ] {
        let body = format!(
            r#"
func check(pos caller: &mut bool)
{{
    {body}
}}
"#
        );

        let source = compilation(&format!("module app;\n{declaration}\n{body}"));

        let imported_body = body
            .replace("touch(", "example.package.api.touch(")
            .replace("take(", "example.package.api.take(")
            .replace("same(", "example.package.api.same(");

        let imported = execution_consumer(
            &provider,
            &format!("module app;\nusing example.package.api;\n{imported_body}"),
        );

        for consumer in [&source, &imported] {
            let diagnostics = consumer.check_diagnostics();

            if let Some(expected) = expected {
                bray_testing::assert_goal_state_diagnostic_kind(&diagnostics, expected);
            } else {
                assert!(!diagnostics.has_errors(), "{body}: {diagnostics:?}");

                let key = source_function_body_key(consumer, "check");

                let lowered = consumer
                    .lowered_unit(key)
                    .expect("source or imported call reborrow lowering");

                assert!(
                    !lowered.diagnostics().has_errors(),
                    "{body}: {:?}",
                    lowered.diagnostics()
                );
            }
        }
    }
}

#[test]
fn returned_values_preserve_default_wrapper_dependencies_in_interfaces() {
    let provider = compilation(
        r#"
            module api;

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
        "#,
    );

    assert!(
        !provider.check_diagnostics().has_errors(),
        "{:?}",
        provider.check_diagnostics()
    );

    for (body, valid) in [
        (
            r#"
                func caller(pos anchor: &bool) -> example.package.api.Holder
                {
                    let transient: bool = true;

                    return example.package.api.choose(&transient, anchor);
                }
            "#,
            true,
        ),
        (
            r#"
                func caller() -> example.package.api.Holder
                {
                    let anchor: bool = true;

                    return example.package.api.choose(&anchor, &anchor);
                }
            "#,
            false,
        ),
    ] {
        let consumer = execution_consumer(
            &provider,
            &format!(
                r#"
                    module app;

                    using example.package.api;

                    {body}
                "#
            ),
        );

        let flow = consumer
            .storage_flow(source_function_body_key(&consumer, "caller"))
            .unwrap();

        if valid {
            assert!(
                !consumer.check_diagnostics().has_errors(),
                "{:?}",
                consumer.check_diagnostics()
            );
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                flow.diagnostics(),
                bray_diagnostics::DiagnosticKind::CheckingEscapingStorageDependency,
            );
        }
    }
}

#[test]
fn storage_projection_guarantees_survive_generic_interfaces() {
    use bray_bound_tree::{
        BoundDependencyRequirement, BoundDependencyRequirementKind, BoundDependencySubject,
        BoundExpression,
    };

    for borrow in ["&", "&mut "] {
        let provider = compilation(
            &r#"
                module api;

                        func project<T>(pos value: BORROWbox T) -> BORROWT executes(pure, total)
                        {
                            return match value
                            {
                                case box(inner)
                                {
                                    yield BORROWinner;
                                }
                            };
                        }
            "#
            .replace("BORROW", borrow),
        );

        assert!(
            !provider.check_diagnostics().has_errors(),
            "{:?}",
            provider.check_diagnostics()
        );

        let consumer = execution_consumer(
            &provider,
            &r#"
                module app;
                        using example.package.api.project;

                        func root(pos value: BORROWbox bool) -> BORROWbool executes(pure, total)
                        {
                            return example.package.api.project<bool>(value);
                        }
            "#
            .replace("BORROW", borrow),
        );

        assert!(
            !consumer.check_diagnostics().has_errors(),
            "{:?}",
            consumer.check_diagnostics()
        );

        let key = source_function_body_key(&consumer, "root");
        let storage = consumer.storage_plan(key.clone()).unwrap();
        let unit = consumer.bound_unit(key.clone()).unwrap();
        let contracts = consumer.dependency_contracts(key).unwrap();

        let (capability, input) = storage
            .value()
            .borrow_capability_entries()
            .find(|(_, capability)| capability.entry_binding().is_some())
            .unwrap();

        let root = storage.value().root_identity(input.access()).unwrap();

        let call = unit
            .value()
            .tree()
            .expressions()
            .find_map(|(id, expression)| {
                matches!(expression, BoundExpression::Call(_)).then_some(id)
            })
            .unwrap();

        let contract = contracts
            .value()
            .expression(call)
            .and_then(|id| contracts.value().contract(id))
            .unwrap();

        assert!(
            contract.requirements().iter().any(|requirement| matches!(
                requirement,
                BoundDependencyRequirement::Direct {
                    subject: BoundDependencySubject::BorrowCapability(actual),
                    kind: BoundDependencyRequirementKind::BorrowCapabilityActive(kind),
                } if *actual == capability && *kind == input.kind()
            )),
            "{contract:?}"
        );

        assert!(
            contract.requirements().iter().any(|requirement| matches!(
                requirement,
                BoundDependencyRequirement::Direct {
                    subject: BoundDependencySubject::StorageAccess(access),
                    kind: BoundDependencyRequirementKind::StorageAlive,
                } if storage.value().root_identity(*access) == Some(root)
            )),
            "{contract:?}"
        );

        let escaping = execution_consumer(
            &provider,
            &r#"
                module app;

                using example.package.api.project;

                func root() -> BORROWbool
                {
                    let mut value = box(true);

                    return example.package.api.project<bool>(BORROWvalue);
                }
            "#
            .replace("BORROW", borrow),
        );

        let flow = escaping
            .storage_flow(source_function_body_key(&escaping, "root"))
            .unwrap();

        bray_testing::assert_goal_state_diagnostic_kind(
            flow.diagnostics(),
            bray_diagnostics::DiagnosticKind::CheckingEscapingStorageDependency,
        );
    }
}

#[test]
fn exact_borrow_origin_survives_generic_export_and_import() {
    let provider = compilation(
        r#"
        module api;
        struct Holder<T> { reference: &T; address: RawPointer<T>; }
        func make<T>(pos input: &T, pos address: RawPointer<T>) -> Holder<T> { return { reference = input, address = address }; }
    "#,
    );

    let consumer = execution_consumer(
        &provider,
        r#"
        module app;
        using example.package.api;
        func forwarded(pos input: &bool, pos address: RawPointer<bool>) -> example.package.api.Holder<bool> { return example.package.api.make(input, address); }
    "#,
    );

    let diagnostics = consumer.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:#?}");

    let callable = crate::test_support::source_function(&consumer, "forwarded");

    let context = consumer
        .binding_context(&consumer.state.cancellation)
        .unwrap();

    let dependencies = bray_binder::SymbolQueryProvider::resolve_symbol_query(
        &context,
        bray_symbols::SymbolQueryRequest::<bray_symbols::CallableResultDependenciesQuery>::new(
            callable.into(),
        ),
    )
    .unwrap();

    let template = consumer
        .semantic_value_store()
        .unwrap()
        .dependency_contract_template_data(*dependencies.value());

    assert_eq!(template.borrows().len(), 2, "{template:#?}");

    for (parameter, kind) in [
        (0, bray_symbols::DependencyReferenceKind::Borrow),
        (1, bray_symbols::DependencyReferenceKind::Address),
    ] {
        let relation = template
            .borrows()
            .iter()
            .find(|relation| relation.kind() == kind)
            .unwrap_or_else(|| panic!("imported origin must retain {kind:?}: {template:#?}"));

        assert_eq!(
            relation.entry().subject_root(),
            bray_symbols::DependencySubjectRoot::Parameter(bray_symbols::SymbolOrdinal::new(
                parameter
            ))
        );

        assert_eq!(
            relation.exit().subject_root(),
            bray_symbols::DependencySubjectRoot::Result
        );

        assert_eq!(relation.exit().projections().len(), 1);
    }
}

#[test]
fn inferred_unwind_behavior_survives_export_and_import() {
    let provider = compilation(
        r#"
        module api;
        func spin() { while true {} }
        func fail() { panic("failure"); }
    "#,
    );

    for (callee, expected) in [("spin", false), ("fail", true)] {
        let consumer = execution_consumer(
            &provider,
            &format!(
                "module app; using example.package.api; func root() {{ example.package.api.{callee}(); }}"
            ),
        );

        let key = source_function_body_key(&consumer, "root");

        let summary = consumer
            .unwind_behavior(key.clone(), &consumer.state.cancellation)
            .unwrap();

        assert!(
            !summary.result().diagnostics().has_errors(),
            "{:?}",
            summary.result().diagnostics()
        );

        assert_eq!(*summary.result().value(), expected, "{callee}");

        let lowered = consumer.lowered_unit(key).unwrap();

        assert!(
            !lowered.diagnostics().has_errors(),
            "{:?}",
            lowered.diagnostics()
        );
    }
}

#[test]
fn imported_generic_addresses_retain_authority_only_while_the_owner_lives() {
    let provider = compilation(
        r#"
        module api;
        func forward<T>(pos pointer: RawPointer<T>) -> RawPointer<T>
            executes(pure, total)
        { return pointer; }
    "#,
    );

    for (body, valid) in [
        (
            r#"
            let value: u8 = 1;
            let pointer = example.package.api.forward<u8>(core.memory.address_of<u8>(&value));
            return core.memory.read<u8>(pointer);
        "#,
            true,
        ),
        (
            r#"
            let mut pointer = core.memory.null<u8>();
            {
                let value: u8 = 1;
                pointer = example.package.api.forward<u8>(core.memory.address_of<u8>(&value));
            };
            return core.memory.read<u8>(pointer);
        "#,
            false,
        ),
    ] {
        let consumer = execution_consumer(
            &provider,
            &format!(
                r#"
            trusted module app;
            using example.package.api;
            trusted func caller() -> u8 uses(raw_memory) {{ {body} }}
        "#
            ),
        );

        let diagnostics = consumer.check_diagnostics();

        assert_eq!(!diagnostics.has_errors(), valid, "{body}: {diagnostics:?}");

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn imported_reference_replacements_preserve_both_identity_and_lifetime() {
    let provider = compilation(
        r#"
        module api;
        struct Holder<T> { mut reference: &T; }
        func replace<T>(pos target: &mut Holder<T>, pos input: &T) { target.reference = input; }
        func forward<T>(pos first: &T, pos second: &T) -> Holder<T> {
            let mut result: Holder<T> = { reference = first };
            replace(&mut result, second);
            return result;
        }
    "#,
    );

    let consumer = execution_consumer(
        &provider,
        r#"
        module app;
        using example.package.api;
        func caller(pos first: &bool, pos second: &bool) -> example.package.api.Holder<bool> {
            let mut result: example.package.api.Holder<bool> = { reference = first };
            example.package.api.replace(&mut result, second);
            return result;
        }
    "#,
    );

    let diagnostics = consumer.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:#?}");

    let template =
        crate::compilation::result_dependencies::source_function_dependencies(&consumer, "caller");

    assert!(
        template
            .borrows()
            .iter()
            .any(|relation| relation.exit().subject_root()
                == bray_symbols::DependencySubjectRoot::Result
                && relation.entry().subject_root()
                    == bray_symbols::DependencySubjectRoot::Parameter(
                        bray_symbols::SymbolOrdinal::new(1)
                    )),
        "{template:#?}"
    );

    let escaping = execution_consumer(
        &provider,
        r#"
        module app;
        using example.package.api;
        func bad(pos caller: &bool) -> example.package.api.Holder<bool> {
            let local = true;
            return example.package.api.forward(caller, &local);
        }
    "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        escaping.check_diagnostics(),
        bray_diagnostics::DiagnosticKind::CheckingEscapingStorageDependency,
    );
}
