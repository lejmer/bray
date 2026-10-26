use crate::test_support::source_function_body_key;

use super::fixtures::{compilation, compilation_from_sources, execution_consumer};

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
            .expect("fixture `dependency_contracts` call must succeed");

        assert!(
            !contracts.diagnostics().has_errors(),
            "{:?}",
            contracts.diagnostics()
        );

        let calls = contracts.value().calls();

        assert_eq!(calls.len(), 1);

        let call = calls.values().next().expect("fixture iterator must provide its expected item");

        assert_eq!(call.effects().is_empty(), unchanged, "{:?}", call.effects());

        if !unchanged {
            assert!(
                call.effects().iter().any(|effect| matches!(
                    effect,
                    bray_symbols::DependencyStorageEffect::Indirect {
                        kind: bray_symbols::DependencyMutationKind::Replace(_),
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
fn imported_immutable_function_values_reuse_the_producers_report_contract() {
    let provider = compilation(
        r#"
        module api;
        struct Cell { mut value: bool; }
        func rewrite(pos cell: &mut Cell) { cell.value = false; }
    "#,
    );

    let consumer = execution_consumer(
        &provider,
        r#"
        module app;
        using example.package.api.Cell;
        using example.package.api.rewrite;
        func caller(pos cell: &mut example.package.api.Cell)
        {
            let operation: func(pos cell: &mut example.package.api.Cell) = example.package.api.rewrite;
            operation(cell);
            operation(cell);
        }
    "#,
    );

    let key = source_function_body_key(&consumer, "caller");

    let checked = consumer
        .dependency_contracts(key.clone())
        .expect("imported callable value checks");

    assert!(
        !checked.diagnostics().has_errors(),
        "{:#?}",
        checked.diagnostics()
    );

    let calls = checked.value().calls();

    assert_eq!(calls.len(), 2);

    for call in calls.values() {
        assert!(call.reports().completed().is_none(), "{call:#?}");
    }

    let lowered = consumer
        .lowered_unit(key)
        .expect("imported callable value lowers");

    assert!(
        !lowered.diagnostics().has_errors(),
        "{:#?}",
        lowered.diagnostics()
    );

    assert!(
        lowered
            .value()
            .as_ref()
            .expect("fixture optional value must be present")
            .mir()
            .expect("fixture instance must retain its MIR")
            .blocks()
            .iter()
            .all(|block| {
                !matches!(
                    block.terminator().kind(),
                    bray_ir::MirTerminatorKind::Return {
                        report: Some(_),
                        ..
                    }
                )
            })
    );
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
            .expect("fixture callable must complete storage-flow analysis");

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

            struct OptionalHolder
            {
                value: (&bool)?;
            }

            func wrap(pos value: &bool) -> OptionalHolder
            {
                return { value = value };
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
            "example.package.api.OptionalHolder",
            "example.package.api.wrap(argument)",
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
                .expect("fixture callable must complete storage-flow analysis");

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
                diagnostics,
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
                bray_testing::assert_goal_state_diagnostic_kind(diagnostics, expected);
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
            .expect("fixture callable must complete storage-flow analysis");

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
        let storage = consumer.storage_plan(key.clone()).expect("fixture callable must have a storage plan");
        let unit = consumer.bound_unit(key.clone()).expect("fixture callable must bind successfully");
        let contracts = consumer.dependency_contracts(key).expect("fixture `dependency_contracts` call must succeed");

        let (capability, input) = storage
            .value()
            .borrow_capability_entries()
            .find(|(_, capability)| capability.entry_binding().is_some())
            .expect("fixture lookup must find its expected value");

        let root = storage.value().root_identity(input.access()).expect("fixture `root_identity` call must succeed");

        let call = unit
            .value()
            .tree()
            .expressions()
            .find_map(|(id, expression)| {
                matches!(expression, BoundExpression::Call(_)).then_some(id)
            })
            .expect("fixture selection must find its expected value");

        let contract = contracts
            .value()
            .expression(call)
            .and_then(|id| contracts.value().contract(id))
            .expect("fixture `and_then` call must succeed");

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
            .expect("fixture callable must complete storage-flow analysis");

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
        struct Holder<T>
        {
            reference: &T;
            address: DataPointer<T>;
        }
        func make<T>(pos input: &T, pos address: DataPointer<T>) -> Holder<T>
            {
                return
                {
                    reference = input,
                    address = address,
                };
            }
    "#,
    );

    let consumer = execution_consumer(
        &provider,
        r#"
        module app;
        using example.package.api;
        func forwarded(pos input: &bool, pos address: DataPointer<bool>) -> example.package.api.Holder<bool>
            {
                return example.package.api.make(input, address);
            }
    "#,
    );

    let diagnostics = consumer.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:#?}");

    let callable = crate::test_support::source_function(&consumer, "forwarded");

    let context = consumer
        .binding_context(&consumer.state.cancellation)
        .expect("fixture compilation must provide a binding context");

    let dependencies = bray_binder::SymbolQueryProvider::resolve_symbol_query(
        &context,
        bray_symbols::SymbolQueryRequest::<bray_symbols::CallableResultDependenciesQuery>::new(
            callable.into(),
        ),
    )
    .expect("fixture symbol query must resolve");

    let template = consumer
        .semantic_value_store()
        .expect("fixture compilation must provide its semantic value store")
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
            relation.entry().expect("input origin").subject_root(),
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

        func spin()
        {
            while true
            {
            }
        }

        func fail()
        {
            panic("failure");
        }

        func cancel() -> u32
        {
            let outcome: RunResult<unit> = Cancelled;

            try outcome;

            return 0;
        }

        func with_default(pos value: u32 = cancel())
        {
        }
    "#,
    );

    let diagnostics = provider.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:#?}");

    for (callee, expected, cancellation) in [
        (
            "spin",
            false,
            bray_symbols::CurrentRunCancellation::NotEntered,
        ),
        (
            "fail",
            true,
            bray_symbols::CurrentRunCancellation::NotEntered,
        ),
        (
            "cancel",
            true,
            bray_symbols::CurrentRunCancellation::MayEnter,
        ),
        (
            "with_default",
            true,
            bray_symbols::CurrentRunCancellation::MayEnter,
        ),
    ] {
        let consumer = execution_consumer(
            &provider,
            &format!(
                r#"
                module app;

                using example.package.api;

                func root()
                {{
                    example.package.api.{callee}();
                }}
                "#
            ),
        );

        let key = source_function_body_key(&consumer, "root");

        let summary = consumer
            .unwind_behavior(key.clone(), &consumer.state.cancellation)
            .expect("fixture callable must resolve its unwind behavior");

        assert!(
            !summary.result().diagnostics().has_errors(),
            "{:?}",
            summary.result().diagnostics()
        );

        assert_eq!(summary.result().value().may_unwind(), expected, "{callee}");

        assert_eq!(
            summary.result().value().current_run_cancellation(),
            cancellation,
            "{callee}"
        );

        let lowered = consumer.lowered_unit(key).expect("fixture callable must lower successfully");

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
        func forward<T>(pos pointer: DataPointer<T>) -> DataPointer<T>
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

            using core.memory;
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
        func choose<T>(pos first: &T, pos second: &T, pos select: bool) -> Holder<T> {
            let mut result: Holder<T> = { reference = first };
            if select { replace(&mut result, second); }
            return result;
        }
        func set_choice<T>(pos target: &mut Holder<T>, pos first: &T, pos second: &T, pos select: bool) {
            if select { target.reference = first; }
            else { target.reference = second; }
        }
        func forward_choice<T>(pos first: &T, pos second: &T, pos select: bool) -> Holder<T> {
            let mut result: Holder<T> = { reference = first };
            set_choice(&mut result, first, second, select);
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
                && relation.entry().expect("input origin").subject_root()
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

    for function in ["choose", "forward_choice"] {
        let caller = format!(
            r#"
            module app;
            using example.package.api;
            func bad(pos caller: &bool, pos select: bool) -> example.package.api.Holder<bool> {{
                let local = true;
                return example.package.api.{function}(caller, &local, select);
            }}
        "#
        );

        let escaping = execution_consumer(&provider, &caller);

        bray_testing::assert_goal_state_diagnostic_kind(
            escaping.check_diagnostics(),
            bray_diagnostics::DiagnosticKind::CheckingEscapingStorageDependency,
        );
    }
}
#[test]
fn imported_field_addresses_keep_the_projection_from_the_original_reference() {
    let provider = compilation(
        r#"
        module api;
        struct Holder<T> { reference: &T; }
        func address<T>(pos holder: &Holder<T>) -> &&T { return &holder.reference; }
    "#,
    );

    let consumer = execution_consumer(
        &provider,
        r#"
        module app;
        using example.package.api.Holder;
        using example.package.api.address;
        func caller(pos holder: &example.package.api.Holder<bool>) -> &&bool { return example.package.api.address(holder); }
    "#,
    );

    let flow = consumer
        .storage_flow(source_function_body_key(&consumer, "caller"))
        .expect("fixture callable must complete storage-flow analysis");

    assert!(!flow.diagnostics().has_errors(), "{:?}", flow.diagnostics());

    let references = flow
        .value()
        .dependencies()
        .borrows()
        .iter()
        .filter(|relation| {
            relation.exit().subject_root() == bray_symbols::DependencySubjectRoot::Result
        })
        .collect::<Vec<_>>();

    assert_eq!(references.len(), 1, "{references:?}");

    assert!(
        references[0]
            .entry()
            .expect("input origin")
            .projections()
            .is_empty()
    );

    assert!(matches!(
        references[0].target_projections(),
        [bray_symbols::DependencyProjection::ProductField(_)]
    ));
}

#[test]
fn imported_generic_protected_payload_addresses_retain_their_storage_projection() {
    let provider = compilation(
        r#"
        module api;

        using core.memory;
        func pointer<T>(pos storage: &Uninit<T>) -> DataPointer<T> {
            return core.memory.uninit_pointer<T>(storage);
        }
    "#,
    );

    let consumer = execution_consumer(
        &provider,
        r#"
        module app;
        using example.package.api.pointer;
        func caller(pos storage: &Uninit<bool>) -> DataPointer<bool> {
            return example.package.api.pointer<bool>(storage);
        }
    "#,
    );

    let dependencies =
        crate::compilation::result_dependencies::source_function_dependencies(&consumer, "caller");

    let relations = dependencies.borrows();

    assert_eq!(relations.len(), 1, "{dependencies:#?}");

    assert_eq!(
        relations[0].kind(),
        bray_symbols::DependencyReferenceKind::Address
    );

    assert_eq!(
        relations[0].target_projections(),
        &[bray_symbols::DependencyProjection::UninitPayload]
    );
}

#[test]
fn imported_nested_reference_layers_survive_generic_specialization() {
    let provider = compilation(
        r#"
        module api;
        struct Cell<T> { value: T; }
        func value<T>(pos outer: &&Cell<T>) -> &T { return &outer.value; }
    "#,
    );

    let consumer = execution_consumer(
        &provider,
        r#"
        module app;
        using example.package.api.Cell;
        using example.package.api.value;
        func forward(pos outer: &&example.package.api.Cell<bool>) -> &bool { return example.package.api.value<bool>(outer); }
    "#,
    );

    assert!(
        !consumer.check_diagnostics().has_errors(),
        "{:#?}",
        consumer.check_diagnostics()
    );

    let dependencies =
        crate::compilation::result_dependencies::source_function_dependencies(&consumer, "forward");

    assert_eq!(dependencies.borrows().len(), 1, "{dependencies:#?}");

    assert_eq!(
        dependencies.borrows()[0].dereferences(),
        1,
        "{dependencies:#?}"
    );

    assert_eq!(
        dependencies.borrows()[0].target_projections().len(),
        1,
        "{dependencies:#?}"
    );
}

#[test]
fn imported_recursive_results_preserve_normal_completion_and_exact_origin() {
    let provider = compilation(
        r#"
        module api;
        func forward<T>(pos value: &T, pos again: bool) -> &T {
            if again { return forward<T>(value, false); }
            return value;
        }
        func forever<T>(pos value: &T) -> &T { return forever<T>(value); }
    "#,
    );

    let consumer = execution_consumer(
        &provider,
        r#"
        module app;
        using example.package.api.forward;
        using example.package.api.forever;
        func good(pos value: &bool) -> &bool { return example.package.api.forward<bool>(value, true); }
        func diverging(pos value: &bool) -> &bool { return example.package.api.forever<bool>(value); }
    "#,
    );

    assert!(
        !consumer.check_diagnostics().has_errors(),
        "{:#?}",
        consumer.check_diagnostics()
    );

    let good =
        crate::compilation::result_dependencies::source_function_dependencies(&consumer, "good");

    assert!(good.normal_completion());
    assert_eq!(good.borrows().len(), 1, "{good:#?}");

    assert_eq!(
        good.borrows()[0]
            .entry()
            .expect("input origin")
            .subject_root(),
        bray_symbols::DependencySubjectRoot::Parameter(bray_symbols::SymbolOrdinal::new(0))
    );

    let diverging = crate::compilation::result_dependencies::source_function_dependencies(
        &consumer,
        "diverging",
    );

    assert!(!diverging.normal_completion());
    assert!(diverging.borrows().is_empty());
}

#[test]
fn imported_union_payload_dependencies_resolve_at_the_owning_boundary() {
    let provider = compilation(
        r#"
        module api;
        union Choice { Value(pos item: &bool); }
        func get(pos choice: Choice) -> &bool {
            match choice { case .Value(item) { return item; } }
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
            "func caller(pos value: &bool) -> &bool { return example.package.api.get(example.package.api.Choice.Value(value)); }",
            true,
        ),
        (
            "func caller() -> &bool { let value = true; return example.package.api.get(example.package.api.Choice.Value(&value)); }",
            false,
        ),
    ] {
        let source = format!(
            "module app; using example.package.api.Choice; using example.package.api.get; {body}"
        );

        let consumer = execution_consumer(&provider, &source);
        let diagnostics = consumer.check_diagnostics();

        if valid {
            assert!(!diagnostics.has_errors(), "{diagnostics:?}");
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                bray_diagnostics::DiagnosticKind::CheckingEscapingStorageDependency,
            );
        }
    }
}

#[test]
fn generic_owner_forwarding_preserves_only_the_transferred_witness() {
    let forwarding = r#"
        module api;
        extern func fail() -> never;
        extern func interfere();
        func disturbed<T>(pos value: T) -> T { interfere(); return value; }
        struct Box<T> { value: T; }
        struct Cell { mut flag: bool; }
        func changed_referent<T>(pos value: T, pos other: &mut Cell) -> T {
            other.flag = false;
            return value;
        }
        func caught_failure<T>(pos value: T) -> T {
            _ = catch { fail(); };
            return value;
        }
        func scalar(pos value: bool) -> bool { return value; }
        func borrow<T>(pos value: &T) -> &T { return value; }
        func changed(pos value: Cell) -> Cell {
            let mut owner = value;
            owner.flag = false;
            return owner;
        }
        func identity<T>(pos value: T) -> T { return value; }
        func unwrap<T>(pos value: Box<T>) -> T { return value.value; }
        func present<T>(pos value: T?) -> T {
            match consume value {
                case ?item { return item; }
                case none { panic("absent"); }
            }
        }
        func success<T>(pos value: Result<T, bool>) -> T {
            match consume value {
                case Ok(item) { return item; }
                case Error(_) { fail(); }
            }
        }
        func choose<T>(pos first: T, pos second: T, pos choice: bool) -> T {
            if choice { return first; }
            return second;
        }
    "#;

    let provider = compilation(forwarding);

    assert!(
        !provider.check_diagnostics().has_errors(),
        "{:?}",
        provider.check_diagnostics()
    );

    for name in [
        "scalar",
        "borrow",
        "changed",
        "disturbed",
        "changed_referent",
        "caught_failure",
    ] {
        let dependencies =
            crate::compilation::result_dependencies::source_function_dependencies(&provider, name);

        assert!(
            dependencies.returned_input().is_none(),
            "{name}: {dependencies:?}"
        );
    }

    for (body, failure) in [
        (
            "let owner = api.identity<Owner>(create()); observe(&owner);",
            None,
        ),
        (
            "let owner = api.identity<Owner>(api.identity<Owner>(create())); observe(&owner);",
            None,
        ),
        (
            "let wrapped: api.Box<Owner> = { value = create() }; let owner = api.unwrap<Owner>(wrapped); observe(&owner);",
            None,
        ),
        (
            "let owner = api.present<Owner>(create()); observe(&owner);",
            None,
        ),
        (
            "let wrapped: Result<Owner, bool> = Ok(create()); let owner = api.success<Owner>(wrapped); observe(&owner);",
            None,
        ),
        (
            "let owner = api.success<Owner>(create_result(true)); observe(&owner);",
            None,
        ),
        (
            "let owner = api.present<Owner>(create_optional(true)); observe(&owner);",
            None,
        ),
        (
            "let owner = api.identity<Owner>(Owner { marker = Marker {} }); observe(&owner);",
            Some(bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven),
        ),
        (
            "let owner = create(); let forged = api.identity<Owner>(Owner { marker = owner.marker }); observe(&forged);",
            Some(bray_diagnostics::DiagnosticKind::CheckingTrustedWitnessTransferNotProven),
        ),
        (
            "let owner = api.unwrap<Owner>(create_box()); observe(&owner);",
            Some(bray_diagnostics::DiagnosticKind::CheckingTrustedWitnessTransferNotProven),
        ),
        (
            "let owner = api.choose<Owner>(create(), Owner { marker = Marker {} }, false); observe(&owner);",
            Some(bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven),
        ),
    ] {
        let declarations = format!(
            r#"
            trusted module app;
            using api;
            struct Marker {{}}
            struct Owner {{ marker: Marker; destruct() {{}} }}
            trusted predicate live(value: &Marker);
            trusted func create() -> Owner ensures(trusted live(&result.marker)) {{
                return Owner {{ marker = Marker {{}} }};
            }}
            func create_result(pos succeeds: bool) -> Result<Owner, bool>
                ensures(!(result matches Ok(_)) || trusted live(result_marker(&result))) {{
                if succeeds {{ return Ok(create()); }}
                return Error(false);
            }}
            const func result_marker(pos value: &Result<Owner, bool>) -> &Marker
                requires(value matches Ok(_)) executes(pure, total) {{
                match value {{
                    case Ok(owner) {{ return &owner.marker; }}
                    case Error(_) {{ panic("absent"); }}
                }}
            }}
            func create_optional(pos succeeds: bool) -> Owner?
                ensures(!(result matches ?_) || trusted live(optional_marker(&result))) {{
                if succeeds {{ let owner = create(); return owner; }}
                return none;
            }}
            const func optional_marker(pos value: &(Owner?)) -> &Marker
                requires(value matches ?_) executes(pure, total) {{
                match value {{
                    case ?owner {{ return &owner.marker; }}
                    case none {{ panic("absent"); }}
                }}
            }}
            trusted func create_box() -> api.Box<Owner>
                ensures(trusted live(&result.value.marker)) {{
                return {{ value = Owner {{ marker = Marker {{}} }} }};
            }}
            func observe(pos value: &Owner)
                requires(trusted live(&value.marker))
                ensures(trusted live(&value.marker)) executes(pure, total) {{}}
            func caller() {{ {body} }}
        "#
        );

        for imported in [false, true] {
            let consumer = if imported {
                execution_consumer(
                    &provider,
                    &declarations
                        .replace("using api;", "using example.package.api;")
                        .replace("api.", "example.package.api."),
                )
            } else {
                compilation_from_sources([forwarding, &declarations])
            };

            let diagnostics = consumer.check_diagnostics();

            if let Some(failure) = failure {
                bray_testing::assert_goal_state_diagnostic_kind(diagnostics, failure);
            } else {
                assert!(
                    !diagnostics.has_errors(),
                    "imported={imported}, {body}: {diagnostics:#?}"
                );
            }
        }
    }
}

#[test]
fn imported_task_owners_retain_dependencies_through_cleanup_and_consumption() {
    let provider = compilation(
        r#"
        module api;
        extern async func work(pos value: &u32);
        func pending(pos value: &u32) -> Future<unit> {
            return work(value);
        }
    "#,
    );

    for (body, valid) in [
        (
            "let owner = example.package.api.pending(&value).start(); value = 1;",
            false,
        ),
        (
            "_ = example.package.api.pending(&value).start(); value = 1;",
            false,
        ),
        (
            "let owner = example.package.api.pending(&value).start(); let moved = owner; value = 1;",
            false,
        ),
        (
            "let owner = example.package.api.pending(&value).start(); try await owner.join(); value = 1;",
            true,
        ),
        (
            "{ let owner = example.package.api.pending(&value).start(); } value = 1;",
            true,
        ),
    ] {
        let source = format!(
            r#"
            module app;
            using example.package.api.pending;
            async func caller() {{
                let mut value: u32 = 0;
                {body}
            }}
        "#
        );

        let consumer = execution_consumer(&provider, &source);

        let flow = consumer
            .storage_flow(source_function_body_key(&consumer, "caller"))
            .expect("fixture callable must complete storage-flow analysis");

        if valid {
            assert!(
                !flow.diagnostics().has_errors(),
                "{body}: {:?}",
                flow.diagnostics()
            );
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                flow.diagnostics(),
                bray_diagnostics::DiagnosticKind::CheckingConflictingBorrow,
            );
        }
    }
}

#[test]
fn terminal_destructor_checks_imported_generic_replacement_types() {
    let provider = compilation(
        r#"
        module api;

        func replace<T>(pos destination: &mut T, pos value: T)
        {
            destination = value;
        }
        "#,
    );

    let provider_diagnostics = provider.check_diagnostics();

    assert!(
        !provider_diagnostics.has_errors(),
        "{provider_diagnostics:?}"
    );

    let flow = provider
        .storage_flow(source_function_body_key(&provider, "replace"))
        .expect("fixture callable must complete storage-flow analysis");

    assert!(
        flow.value()
            .dependencies()
            .effects()
            .iter()
            .any(|effect| matches!(
                effect,
                bray_symbols::DependencyStorageEffect::Indirect {
                    kind: bray_symbols::DependencyMutationKind::Replace(_),
                    ..
                }
            )),
        "{:?}",
        flow.value().dependencies().effects()
    );

    for (declaration, value, rejected) in [
        ("struct Child { finalize() {} }", "Child {}", true),
        ("struct Child { destruct() {} }", "Child {}", false),
    ] {
        let source = format!(
            r#"
            module app;
            using example.package.api.replace;

            {declaration}

            struct Owner
            {{
                mut child: Child;

                destruct()
                {{
                    example.package.api.replace(&mut self.child, {value});
                }}
            }}
            "#
        );

        let consumer = execution_consumer(&provider, &source);
        let diagnostics = consumer.check_diagnostics();

        if rejected {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                bray_diagnostics::DiagnosticKind::CheckingDestructorOwnershipIntroduction,
            );
        } else {
            assert!(diagnostics.is_empty(), "{diagnostics:?}");
        }
    }
}

#[test]
fn imported_unknown_call_effects_do_not_claim_only_retirement() {
    let provider = compilation(
        r#"
        module api;

        extern func alter<T>(pos value: &mut T);

        func forward<T>(pos value: &mut T)
        {
            alter(value);
        }
        "#,
    );

    let flow = provider
        .storage_flow(source_function_body_key(&provider, "forward"))
        .expect("fixture callable must complete storage-flow analysis");

    assert!(
        flow.value()
            .dependencies()
            .effects()
            .contains(&bray_symbols::DependencyStorageEffect::Ambient)
    );

    assert!(
        flow.value()
            .dependencies()
            .effects()
            .iter()
            .any(|effect| matches!(
                effect,
                bray_symbols::DependencyStorageEffect::Indirect {
                    kind: bray_symbols::DependencyMutationKind::Escape,
                    ..
                }
            ))
    );

    let consumer = execution_consumer(
        &provider,
        r#"
        module app;

        using example.package.api.forward;

        struct Owner
        {
            mut value: i32;

            finalize() {}

            destruct()
            {
                example.package.api.forward(&mut self.value);
            }
        }
        "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        consumer.check_diagnostics(),
        bray_diagnostics::DiagnosticKind::CheckingUnprovedDestructorEffects,
    );
}

#[test]
fn terminal_destructor_can_clear_an_owner_through_an_imported_generic_helper() {
    let provider = compilation(
        r#"
        module api;

        func clear<T>(pos destination: &mut T?)
        {
            destination = none;
        }
        "#,
    );

    let consumer = execution_consumer(
        &provider,
        r#"
        module app;

        using example.package.api.clear;

        struct Child
        {
            finalize() {}
        }

        struct Owner
        {
            mut child: Child?;

            destruct()
            {
                example.package.api.clear(&mut self.child);
            }
        }
        "#,
    );

    let diagnostics = consumer.check_diagnostics();

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn imported_escape_effects_distinguish_retained_arguments_from_opaque_execution() {
    let provider = compilation(
        r#"
        trusted module api;

        using core.memory;

        struct Marker
        {
        }

        struct Owner
        {
            marker: Marker;
            mut value: u32;
        }

        trusted predicate live(value: &Marker);

        extern trusted func opaque();

        extern trusted func publish(pos pointer: DataPointer<Owner>);

        trusted func make() -> Owner
            ensures(trusted live(&result.marker))
        {
            return
            {
                marker = Marker
                {
                },
                value = 0
            };
        }

        trusted func inspect(pos owner: &Owner)
            requires(trusted live(&owner.marker))
            ensures(trusted live(&owner.marker))
        {
            opaque();

            let certify = trusted lambda(pos marker: &Marker)
                ensures(trusted live(marker))
                executes(pure, total) {};

            certify(&owner.marker);
        }

        trusted func mutate(pos owner: &mut Owner)
            requires(trusted live(&owner.marker))
            ensures(trusted live(&owner.marker))
            uses(raw_memory, unchecked_init)
        {
            let pointer = core.memory.address_of_mut<u32>(&mut owner.value);

            trusted core.memory.write<u32>(pointer, 1);
        }

        trusted func expose(pos owner: &Owner)
            requires(trusted live(&owner.marker))
            ensures(trusted live(&owner.marker))
        {
            publish(core.memory.address_of<Owner>(owner));

            let certify = trusted lambda(pos marker: &Marker)
                ensures(trusted live(marker))
                executes(pure, total) {};

            certify(&owner.marker);
        }

        func observe(pos owner: &Owner)
            requires(trusted live(&owner.marker))
            executes(pure, total) {}
        "#,
    );

    let diagnostics = provider.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:?}");

    for (operation, valid) in [("inspect", true), ("mutate", true), ("expose", false)] {
        let borrow = if operation == "mutate" {
            "&mut owner"
        } else {
            "&owner"
        };

        let source = format!(
            r#"
            trusted module app;
            using example.package.api.make;
            using example.package.api.{operation};
            using example.package.api.opaque;
            using example.package.api.observe;

            func caller()
            {{
                let mut owner = example.package.api.make();
                example.package.api.{operation}({borrow});
                example.package.api.opaque();
                example.package.api.observe(&owner);
            }}
            "#
        );

        let consumer = execution_consumer(&provider, &source);
        let diagnostics = consumer.check_diagnostics();

        assert_eq!(
            !diagnostics.has_errors(),
            valid,
            "{operation}: {diagnostics:?}"
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn returned_address_fields_keep_their_origin_through_anonymous_factories() {
    let provider = compilation(
        r#"
        module api;

        using core.atomic;

        struct Owner
        {
            pointer: DataPointer<u32>;
        }

        func forward(pos owner: &Owner, pos counter: &core.atomic.Atomic<usize>) -> Owner
        {
            _ = core.atomic.fetch_add<usize, 0>(counter, 1);

            let make = lambda(pos pointer: DataPointer<u32>) -> Owner
            {
                return
                {
                    pointer = pointer
                };
            };

            return make(owner.pointer);
        }
        "#,
    );

    let diagnostics = provider.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:?}");

    let dependencies =
        crate::compilation::result_dependencies::source_function_dependencies(&provider, "forward");

    let returned = dependencies
        .borrows()
        .iter()
        .filter(|relation| {
            relation.exit().subject_root() == bray_symbols::DependencySubjectRoot::Result
        })
        .collect::<Vec<_>>();

    assert_eq!(returned.len(), 1, "{dependencies:#?}");

    assert_eq!(
        returned[0].kind(),
        bray_symbols::DependencyReferenceKind::Address
    );
}

#[test]
fn returned_union_payload_addresses_remain_bounded_after_extraction() {
    let provider = compilation(
        r#"
        trusted module api;

        struct Marker
        {
        }

        struct Owner
        {
            marker: Marker;
            pointer: DataPointer<u32>;
        }

        trusted predicate live(marker: &Marker);

        extern func pointer() -> DataPointer<u32>;

        extern func touch(pos pointer: DataPointer<u32>);

        func make() -> Result<Owner, bool>
            ensures(!(result matches Ok(_)) || trusted live(result_marker(&result)))
        {
            let make_owner = trusted lambda(pos pointer: DataPointer<u32>) -> Owner
                ensures(trusted live(&result.marker))
            {
                return Owner
                {
                    marker = Marker
                    {
                    },
                    pointer = pointer
                };
            };

            return Ok(make_owner(pointer()));
        }

        const func result_marker(pos value: &Result<Owner, bool>) -> &Marker
            requires(value matches Ok(_))
            executes(pure, total)
        {
            match value
            {
                case Ok(owner) { return &owner.marker; }
                case Error(_) { panic("absent"); }
            }
        }

        func copy_pointer(pos owner: &Owner) -> DataPointer<u32>
        {
            return owner.pointer;
        }

        func observe(pos owner: &Owner)
            requires(trusted live(&owner.marker))
            executes(pure, total) {}
        "#,
    );

    let diagnostics = provider.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:?}");

    let consumer = execution_consumer(
        &provider,
        r#"
        module app;

        using example.package.api;

        func caller() -> Result<unit, bool>
        {
            let owner = try example.package.api.make();

            example.package.api.touch(example.package.api.copy_pointer(&owner));
            example.package.api.observe(&owner);
            return Ok(unit);
        }
        "#,
    );

    let diagnostics = consumer.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:?}");
}

#[test]
fn propagation_preserves_the_selected_payload_reference_relation() {
    for (ty, input, result, count) in [
        ("DataPointer<u32>?", "input?", "pointer", 1),
        ("Wrapper?", "input?", "pointer", 1),
        ("Outer?", "input?", "pointer", 1),
        ("Result<Wrapper, bool>", "try input", "Ok(pointer)", 1),
        ("Result<Outer, Wrapper>", "try input", "Ok(pointer)", 2),
        (
            "Result<DataPointer<u32>, bool>",
            "try input",
            "Ok(pointer)",
            1,
        ),
        (
            "Result<DataPointer<u32>, DataPointer<u32>>",
            "try input",
            "Ok(pointer)",
            2,
        ),
    ] {
        for yield_region in [false, true] {
            let body = if yield_region {
                let yielded = if ty.ends_with('?') {
                    format!("yield {result};")
                } else {
                    format!("let value: {ty} = {result};\nyield value;")
                };

                format!(
                    r#"
                    let forwarded: {ty} =
                    {{
                        let pointer = {input};
                        {yielded}
                    }};
                    return forwarded;
                "#
                )
            } else {
                format!(
                    r#"
                    let pointer = {input};
                    return {result};
                "#
                )
            };

            let provider = compilation(&format!(
                r#"
                module api;

                struct Wrapper
                {{
                    pointer: DataPointer<u32>;
                }}

                struct Outer
                {{
                    inner: Wrapper;
                }}

                func forward(pos input: {ty}) -> {ty}
                {{
                    {body}
                }}
                "#
            ));

            let diagnostics = provider.check_diagnostics();

            assert!(
                !diagnostics.has_errors(),
                "{ty}, yield={yield_region}: {diagnostics:?}"
            );

            let dependencies =
                crate::compilation::result_dependencies::source_function_dependencies(
                    &provider, "forward",
                );

            let returned = dependencies
                .borrows()
                .iter()
                .filter(|relation| {
                    relation.exit().subject_root() == bray_symbols::DependencySubjectRoot::Result
                })
                .collect::<Vec<_>>();

            assert_eq!(
                returned.len(),
                count,
                "{ty}, yield={yield_region}: {dependencies:#?}"
            );

            for relation in returned {
                assert_eq!(
                    relation.kind(),
                    bray_symbols::DependencyReferenceKind::Address
                );

                assert_eq!(
                    relation.entry().expect("input origin").projections(),
                    relation.exit().projections()
                );
            }
        }
    }
}

#[test]
fn compound_assignment_preserves_operator_effects_at_terminal_and_imported_boundaries() {
    let declarations = r#"
        module api;

        struct Child
        {
            finalize() {}
        }

        struct Value
        {
            child: DataPointer<Child>;
            fail: bool;
        }

        extern func replace(pos child: DataPointer<Child>);

        impl ValueAdd = Value(Add<Value>)
        {
            type Output = Value;

            func add(pos rhs: &Value) -> Value
            {
                replace(self.child);

                if rhs.fail
                {
                    assert(false);
                }

                return Value
                {
                    child = rhs.child,
                    fail = false
                };
            }
        }

        struct Container
        {
            mut value: Value;
        }

        func update(pos container: &mut Container)
        {
            container.value += container.value;
        }
    "#;

    let provider = compilation(declarations);
    let diagnostics = provider.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:?}");

    let dependencies =
        crate::compilation::result_dependencies::source_function_dependencies(&provider, "update");

    assert!(
        dependencies
            .effects()
            .iter()
            .any(|effect| matches!(effect, bray_symbols::DependencyStorageEffect::Ambient)),
        "the operator's opaque effects must survive its scalar-only replacement: {dependencies:#?}"
    );

    for imported in [false, true] {
        for operation in [
            "self.container.value += self.container.value;",
            "update(&mut self.container);",
        ] {
            let owner = format!(
                r#"
                struct Owner
                {{
                    mut container: Container;

                    finalize() {{}}

                    destruct()
                    {{
                        {operation}
                    }}
                }}
            "#
            );

            let consumer = if imported {
                let owner = owner
                    .replace("Container", "example.package.api.Container")
                    .replace("update(", "example.package.api.update(");

                execution_consumer(
                    &provider,
                    &format!(
                        r#"
                    module app;
                    using example.package.api.Container;
                    using example.package.api.update;
                    using example.package.api.ValueAdd;

                    {owner}
                "#
                    ),
                )
            } else {
                compilation(&format!("{declarations}\n{owner}"))
            };

            let diagnostics = consumer.check_diagnostics();

            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                bray_diagnostics::DiagnosticKind::CheckingUnprovedDestructorEffects,
            );
        }
    }
}

#[test]
fn unresolved_assignment_retains_stored_reference_capture() {
    let declarations = r#"
        module api;

        struct Slot
        {
            mut pointer: DataPointer<bool>;
        }

        func choose(pos left: &mut Slot, pos right: &mut Slot, pos first: bool) -> &mut Slot
        {
            if first
            {
                return left;
            }
            return right;
        }

        func store(pos left: &mut Slot, pos right: &mut Slot, pos pointer: DataPointer<bool>, pos first: bool)
        {
            let destination = choose(left, right, first);
            destination.pointer = pointer;
        }
    "#;

    let provider = compilation(declarations);
    let diagnostics = provider.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:?}");

    for imported in [false, true] {
        let body = r#"
            extern func opaque();

            func forward(pos left: &mut Slot, pos right: &mut Slot, pos pointer: DataPointer<bool>, pos first: bool)
            {
                store(left, right, pointer, first);
                opaque();
            }
        "#;

        let consumer = if imported {
            let body = body
                .replace("Slot", "example.package.api.Slot")
                .replace("store(", "example.package.api.store(");

            execution_consumer(
                &provider,
                &format!(
                    "module app; using example.package.api.Slot; using example.package.api.store; {body}"
                ),
            )
        } else {
            compilation(&format!("{declarations}\n{body}"))
        };

        let diagnostics = consumer.check_diagnostics();

        assert!(!diagnostics.has_errors(), "{diagnostics:?}");

        let dependencies = crate::compilation::result_dependencies::source_function_dependencies(
            &consumer, "forward",
        );

        assert!(
            dependencies.effects().iter().any(|effect| matches!(
                effect,
                bray_symbols::DependencyStorageEffect::Indirect {
                    kind: bray_symbols::DependencyMutationKind::Escape,
                    ..
                }
            )),
            "stored references remain exposed across an opaque call; imported={imported}: {dependencies:#?}"
        );
    }
}

#[test]
fn copied_parameter_storage_does_not_escape_through_source_or_imported_calls() {
    let declarations = r#"
        module api;

        @copy
        struct Metadata
        {
            value: bool;
            namespace: [u64; 4];
        }

        extern func expose(pos value: &bool);

        func scalar(pos value: bool)
        {
            expose(&value);
        }

        func aggregate(pos value: Metadata)
        {
            expose(&value.value);
        }

        extern func accept(pos value: Metadata);

        func forwarded(pos value: Metadata)
        {
            accept(value);
        }

        func reference(pos value: &Metadata)
        {
            expose(&value.value);
        }
    "#;

    let provider = compilation(declarations);
    let diagnostics = provider.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:?}");

    for imported in [false, true] {
        for (name, argument) in [
            ("scalar", "value.value"),
            ("aggregate", "value"),
            ("forwarded", "value"),
            ("reference", "&value"),
        ] {
            let body = format!(
                r#"
                func caller(pos value: Metadata)
                {{
                    {name}({argument});
                }}
                "#
            );

            let consumer = if imported {
                let body = body
                    .replace("Metadata", "example.package.api.Metadata")
                    .replace(&format!("{name}("), &format!("example.package.api.{name}("));

                execution_consumer(
                    &provider,
                    &format!(
                        "module app; using example.package.api.Metadata; using example.package.api.{name}; {body}"
                    ),
                )
            } else {
                compilation(&format!("{declarations}\n{body}"))
            };

            let contracts = consumer
                .dependency_contracts(source_function_body_key(&consumer, "caller"))
                .expect("fixture `dependency_contracts` call must succeed");

            assert!(
                !contracts.diagnostics().has_errors(),
                "{:?}",
                contracts.diagnostics()
            );

            let calls = contracts.value().calls();

            assert_eq!(calls.len(), 1);

            let effects = calls.values().next().expect("fixture iterator must provide its expected item").effects();

            assert!(
                !effects.iter().any(|effect| matches!(
                    effect,
                    bray_symbols::DependencyStorageEffect::Direct { .. }
                )),
                "a copied parameter's inline storage does not belong to its caller: imported={imported}, {name}: {effects:?}"
            );

            assert_eq!(
                effects.iter().any(|effect| matches!(
                    effect,
                    bray_symbols::DependencyStorageEffect::Indirect {
                        kind: bray_symbols::DependencyMutationKind::Escape,
                        ..
                    }
                )),
                name == "reference",
                "borrowed referent escapes must remain visible: imported={imported}, {name}: {effects:?}"
            );
        }
    }
}

#[test]
fn composite_conversion_effects_use_the_aggregate_input_paths() {
    use bray_symbols::{
        DependencyProjection, DependencyStorageEffect, DependencySubjectRoot, SymbolOrdinal,
    };

    for (input, output, expected) in [
        (
            "(Source, Source)",
            "(Target, Target)",
            vec![
                DependencyProjection::TupleElement(SymbolOrdinal::new(0)),
                DependencyProjection::TupleElement(SymbolOrdinal::new(1)),
            ],
        ),
        (
            "Source?",
            "Target?",
            vec![DependencyProjection::NullableValue],
        ),
        ("[Source; 2]", "[Target; 2]", vec![]),
    ] {
        let source = r#"
        module api;
        struct Value
        {
            mut active: bool;
        }
        struct Source
        {
            value: &mut Value;
        }
        struct Target {}
        impl Conversion = Source(ConvertTo<Target>)
        {
            consume func convert() -> Target
            {
                let mut reference = self.value;
                reference.active = true;
                return Target {};
            }
        }
        func caller(pos value: Input) -> Output
        {
            return value as Output;
        }
        "#
        .replace("Input", input)
        .replace("Output", output);

        let application = compilation(&source);
        let diagnostics = application.check_diagnostics();

        assert!(!diagnostics.has_errors(), "{source}: {diagnostics:#?}");

        let consumer = execution_consumer(
            &application,
            &format!(
                "module app; using example.package.api.Source; using example.package.api.Target; using example.package.api.caller; func forward(pos value: {}) -> {} {{ return example.package.api.caller(value); }}",
                input.replace("Source", "example.package.api.Source"),
                output.replace("Target", "example.package.api.Target")
            ),
        );

        let imported_diagnostics = consumer.check_diagnostics();

        assert!(
            !imported_diagnostics.has_errors(),
            "{imported_diagnostics:#?}"
        );

        for (application, name) in [(&application, "caller"), (&consumer, "forward")] {
            let key = crate::test_support::source_function_body_key(application, name);
            let flow = application.storage_flow(key).expect("fixture callable must complete storage-flow analysis");
            let summary = flow.value().dependencies().effects();

            let summary_roots = summary
                .iter()
                .filter_map(|effect| match effect {
                    DependencyStorageEffect::Indirect { reference, .. } => Some(reference),
                    _ => None,
                })
                .collect::<Vec<_>>();

            assert_eq!(
                summary_roots.len(),
                expected.len(),
                "{name}: {source}: {summary:#?}"
            );

            for reference in summary_roots {
                assert_eq!(
                    reference.subject_root(),
                    DependencySubjectRoot::Parameter(SymbolOrdinal::new(0))
                );

                assert!(
                    expected.contains(reference.projections().first().expect("fixture sequence must contain its first item")),
                    "{summary:#?}"
                );
            }

            assert_eq!(
                summary
                    .iter()
                    .any(|effect| matches!(effect, DependencyStorageEffect::Unknown)),
                expected.is_empty(),
                "{name}: {source}: {summary:#?}"
            );
        }

        let key = crate::test_support::source_function_body_key(&application, "caller");
        let contracts = application.dependency_contracts(key).expect("fixture `dependency_contracts` call must succeed");

        let effects = contracts
            .value()
            .calls()
            .values()
            .flat_map(|call| call.effects())
            .collect::<Vec<_>>();

        let indirect = effects
            .iter()
            .filter_map(|effect| match effect {
                DependencyStorageEffect::Indirect { reference, .. } => Some(reference),
                _ => None,
            })
            .collect::<Vec<_>>();

        let actual = indirect
            .iter()
            .map(|reference| {
                assert_eq!(reference.subject_root(), DependencySubjectRoot::Receiver);

                *reference
                    .projections()
                    .first()
                    .expect("composite reference retains its aggregate prefix")
            })
            .collect::<Vec<_>>();

        assert_eq!(actual, expected, "{source}: {effects:#?}");

        if expected.is_empty() {
            assert!(
                effects
                    .iter()
                    .any(|effect| matches!(effect, DependencyStorageEffect::Unknown)),
                "{effects:#?}"
            );
        }
    }
}

#[test]
fn custom_index_effects_preserve_owned_and_borrowed_input_origins() {
    use bray_symbols::{
        DependencyProjection, DependencyStorageEffect, DependencySubjectRoot, SymbolOrdinal,
    };

    let provider = compilation(
        r#"
        module api;
        struct Value { mut active: bool; }
        struct Indexed { mut output: i32; mut visits: usize; target: &mut Value; }
        impl IndexAccess = Indexed(MutableElementIndex<i32>)
        {
            type Output = i32;
            mut func index(pos selector: &i32) -> &mut i32
            {
                self.visits = 1;
                self.target.active = true;
                return &mut self.output;
            }
        }
        func owned(pos mut indexed: Indexed) { _ = &mut indexed[0]; }
        func borrowed(pos indexed: &mut Indexed) { _ = &mut indexed[0]; }
    "#,
    );

    let diagnostics = provider.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:#?}");

    let consumer = execution_consumer(
        &provider,
        r#"
        module app;
        using example.package.api.Indexed;
        using example.package.api.owned;
        using example.package.api.borrowed;
        func owned(pos indexed: example.package.api.Indexed) { example.package.api.owned(indexed); }
        func borrowed(pos indexed: &mut example.package.api.Indexed) { example.package.api.borrowed(indexed); }
    "#,
    );

    let diagnostics = consumer.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:#?}");

    for application in [&provider, &consumer] {
        for name in ["owned", "borrowed"] {
            let flow = application
                .storage_flow(source_function_body_key(application, name))
                .expect("fixture callable must complete storage-flow analysis");

            let effects = flow.value().dependencies().effects();

            assert!(
                !effects
                    .iter()
                    .any(|effect| matches!(effect, DependencyStorageEffect::Unknown)),
                "{name}: {effects:#?}"
            );

            assert!(effects.iter().any(|effect| matches!(effect,
                DependencyStorageEffect::Indirect { reference, .. }
                if reference.subject_root() == DependencySubjectRoot::Parameter(SymbolOrdinal::new(0))
                    && matches!(reference.projections(), [DependencyProjection::ProductField(_)])
            )), "{name}: {effects:#?}");
        }
    }
}

#[test]
fn imported_calls_distinguish_normal_preservation_from_caught_failure_effects() {
    let provider = compilation(
        r#"
        module api;

        struct Cell
        {
            mut value: bool;
        }

        func check(pos fail: bool)
        {
            if fail
            {
                panic("failed");
            }
        }

        func change(pos cell: &mut Cell)
        {
            cell.value = false;
        }

        func change_then_fail(pos cell: &mut Cell, pos fail: bool)
        {
            if fail
            {
                cell.value = false;
                panic("failed");
            }
        }
    "#,
    );

    let consumer = execution_consumer(
        &provider,
        r#"
        module app;
        using example.package.api;

        func forward(pos fail: bool)
        {
            example.package.api.check(fail);
        }

        func mutate(pos cell: &mut example.package.api.Cell)
        {
            example.package.api.change(cell);
        }

        func preserve(pos cell: &mut example.package.api.Cell, pos fail: bool)
        {
            example.package.api.change_then_fail(cell, fail);
        }

        func recover(pos cell: &mut example.package.api.Cell, pos fail: bool)
        {
            _ = catch
            {
                example.package.api.change_then_fail(cell, fail);
            };
        }
    "#,
    );

    assert!(
        !consumer.check_diagnostics().has_errors(),
        "{:?}",
        consumer.check_diagnostics()
    );

    for (name, preserved) in [
        ("forward", true),
        ("mutate", false),
        ("preserve", true),
        ("recover", false),
    ] {
        let dependencies =
            crate::compilation::result_dependencies::source_function_dependencies(&consumer, name);

        if name != "forward" {
            assert!(
                !dependencies.effects().is_empty(),
                "{name}: {dependencies:?}"
            );
        }

        assert_eq!(
            dependencies.preserves_storage_on_normal_return(),
            preserved,
            "{name}: {dependencies:?}"
        );
    }
}

#[test]
fn imported_conditional_null_pointer_bounds_never_become_exact_identity() {
    let provider = compilation(
        r#"
        module api;
        using core.memory;

        func pointer<T>(pos value: &T, pos present: bool) -> DataPointer<T>
        {
            if present { return core.memory.address_of<T>(value); }
            return core.memory.null<T>();
        }
        "#,
    );

    let consumer = execution_consumer(
        &provider,
        r#"
        module app;
        using core.memory;
        using example.package.api.pointer;

        func caller(pos value: &u32, pos present: bool) -> DataPointer<u32>
        {
            return example.package.api.pointer<u32>(value, present);
        }
        "#,
    );

    let diagnostics = consumer.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:#?}");

    let dependencies =
        crate::compilation::result_dependencies::source_function_dependencies(&consumer, "caller");

    assert_eq!(dependencies.borrows().len(), 1, "{dependencies:#?}");
    assert!(!dependencies.borrows()[0].is_exact(), "{dependencies:#?}");
}

#[test]
fn slice_arguments_bound_opaque_effects_without_exposing_other_inputs() {
    let source = r#"
        module api;
        extern func opaque(pos value: &mut [u8]);

        func update(pos preserved: &mut bool, pos value: &mut [u8])
        {
            opaque(value);
        }
    "#;

    let provider = compilation(source);

    let caller = r#"
        module app;
        using example.package.api;

        func caller(pos preserved: &mut bool)
        {
            let mut buffer: bytes<1> = [0];
            example.package.api.update(preserved, &mut buffer[..]);
        }
    "#;

    for imported in [false, true] {
        let consumer = if imported {
            execution_consumer(&provider, caller)
        } else {
            compilation_from_sources([source, &caller.replace("example.package.", "")])
        };

        let diagnostics = consumer.check_diagnostics();

        assert!(
            !diagnostics.has_errors(),
            "imported={imported}: {diagnostics:#?}"
        );

        let summary = crate::compilation::result_dependencies::source_function_dependencies(
            &consumer, "caller",
        );

        assert!(
            summary.effects().iter().all(|effect| !matches!(
                effect,
                bray_symbols::DependencyStorageEffect::Escape
                    | bray_symbols::DependencyStorageEffect::Indirect {
                        kind: bray_symbols::DependencyMutationKind::Escape,
                        ..
                    }
                    | bray_symbols::DependencyStorageEffect::Direct {
                        kind: bray_symbols::DependencyMutationKind::Escape,
                        ..
                    }
            )),
            "imported={imported}: {summary:#?}"
        );
    }
}

#[test]
fn recursive_nullable_pointer_bounds_include_every_normal_origin() {
    let provider = compilation(
        r#"
        module api;
        using core.memory;

        func choose(pos first: &u32, pos second: &u32, pos depth: u32, pos present: bool) -> DataPointer<u32>
        {
            if depth == 0
            {
                if present { return core.memory.address_of<u32>(first); }
                return core.memory.null<u32>();
            }
            return choose(second, first, depth - 1, present);
        }
        "#,
    );

    let consumer = execution_consumer(
        &provider,
        r#"
        module app;
        using example.package.api.choose;

        func caller(pos first: &u32, pos second: &u32, pos depth: u32, pos present: bool) -> DataPointer<u32>
        {
            return example.package.api.choose(first, second, depth, present);
        }
        "#,
    );

    for (application, name) in [(&provider, "choose"), (&consumer, "caller")] {
        let diagnostics = application.check_diagnostics();

        assert!(!diagnostics.has_errors(), "{name}: {diagnostics:#?}");

        let summary = crate::compilation::result_dependencies::source_function_dependencies(
            application,
            name,
        );

        assert_eq!(summary.borrows().len(), 2, "{name}: {summary:#?}");

        assert!(
            summary
                .borrows()
                .iter()
                .all(|relation| !relation.is_exact()),
            "{name}: {summary:#?}"
        );

        let origins = summary
            .borrows()
            .iter()
            .map(|relation| relation.entry().expect("input origin").subject_root())
            .collect::<std::collections::BTreeSet<_>>();

        assert_eq!(
            origins,
            std::collections::BTreeSet::from([
                bray_symbols::DependencySubjectRoot::Parameter(bray_symbols::SymbolOrdinal::new(0)),
                bray_symbols::DependencySubjectRoot::Parameter(bray_symbols::SymbolOrdinal::new(1)),
            ])
        );
    }
}

#[test]
fn copied_handles_do_not_publish_their_borrowed_containers() {
    let source = r#"
        trusted module api;
        using core.target;
        using core.memory;

        struct Owner
        {
            handle: usize;
            mut complete: bool;
        }

        extern trusted func opaque(pos pointer: DataPointer<u8>);

        trusted func from_handle(pos handle: usize) -> DataPointer<u8>
            uses(intrinsic)
        {
            return trusted core.target.from_exposed_address<u8>(address = handle);
        }

        func update(pos owner: &mut Owner)
        {
            opaque(from_handle(owner.handle));
            owner.complete = true;
        }
    "#;

    let provider = compilation(source);

    let caller = r#"
        module app;
        using example.package.api;

        func caller(pos owner: &mut example.package.api.Owner)
        {
            example.package.api.update(owner);
        }
    "#;

    for imported in [false, true] {
        let consumer = if imported {
            execution_consumer(&provider, caller)
        } else {
            compilation_from_sources([source, &caller.replace("example.package.", "")])
        };

        let diagnostics = consumer.check_diagnostics();

        assert!(
            !diagnostics.has_errors(),
            "imported={imported}: {diagnostics:#?}"
        );

        let summary = crate::compilation::result_dependencies::source_function_dependencies(
            &consumer, "caller",
        );

        assert!(
            summary.effects().iter().all(|effect| !matches!(
                effect,
                bray_symbols::DependencyStorageEffect::Escape
                    | bray_symbols::DependencyStorageEffect::Indirect {
                        kind: bray_symbols::DependencyMutationKind::Escape,
                        ..
                    }
                    | bray_symbols::DependencyStorageEffect::Direct {
                        kind: bray_symbols::DependencyMutationKind::Escape,
                        ..
                    }
            )),
            "imported={imported}: {summary:#?}"
        );
    }
}

#[test]
fn owned_computations_retain_checked_effects_without_granting_authority() {
    let source = r#"
        trusted module api;
        using core.memory;

        struct Marker {}
        struct Owner { marker: Marker; }

        trusted predicate live(value: &Marker);
        extern trusted func opaque();
        extern trusted func publish(pos pointer: DataPointer<Owner>);

        trusted func make() -> Owner
            ensures(trusted live(&result.marker))
        {
            return { marker = Marker {} };
        }

        async func work(pos owner: &Owner)
            requires(trusted live(&owner.marker))
        {
            opaque();
        }

        async func escaping(pos owner: &Owner)
            requires(trusted live(&owner.marker))
        {
            publish(core.memory.address_of<Owner>(owner));
        }

        struct Guard
        {
            owner: &Owner;

            destruct()
            {
                publish(core.memory.address_of<Owner>(self.owner));
            }
        }

        async func retained_result(pos owner: &Owner) -> Guard
        {
            return { owner = owner };
        }

        async func retained_input(pos guard: Guard) -> Guard
        {
            return guard;
        }

        func inspect(pos owner: &Owner)
            requires(trusted live(&owner.marker))
            executes(pure, total)
        {
        }
    "#;

    let provider = compilation(source);

    for (body, valid) in [
        (
            "let task = api.work(&owner).start(); try await task.join();",
            true,
        ),
        (
            "let task = api.work(&owner).start(); let moved = task; try await moved.join();",
            true,
        ),
        (
            "let task = api.work(&owner).start(); await task.cancel();",
            true,
        ),
        (
            "{ let task = api.work(&owner).start(); await api.work(&owner); }",
            true,
        ),
        (
            "let task = api.work(&owner).start(); await api.work(&owner); try await task.join();",
            true,
        ),
        ("{ let future = api.work(&owner); }", true),
        ("api.work(&owner);", true),
        ("api.retained_result(&owner);", true),
        ("{ let task = api.retained_result(&owner).start(); }", false),
        (
            "let guard = api.Guard { owner = &owner }; { let task = api.retained_input(guard).start(); }",
            false,
        ),
        (
            "let task = api.escaping(&owner).start(); try await task.join();",
            false,
        ),
    ] {
        let caller = format!(
            r#"
            trusted module app;
            using api;

            async func caller()
            {{
                let owner = api.make();
                {body}
                api.opaque();
                api.inspect(&owner);
            }}
        "#
        );

        for imported in [false, true] {
            let consumer = if imported {
                execution_consumer(
                    &provider,
                    &caller
                        .replace("using api;", "using example.package.api;")
                        .replace("api.", "example.package.api."),
                )
            } else {
                compilation_from_sources([source, &caller])
            };

            let diagnostics = consumer.check_diagnostics();

            assert_eq!(
                !diagnostics.has_errors(),
                valid,
                "imported={imported}, {body}: {diagnostics:#?}"
            );

            if !valid {
                bray_testing::assert_goal_state_diagnostic_kind(
                    diagnostics,
                    bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven,
                );
            }
        }
    }
}

#[test]
fn independent_pointer_origins_survive_calls_without_publishing_other_inputs() {
    let source = r#"
        trusted module api;
        using core.memory;
        struct Marker {}
        struct Owner { marker: Marker; }
        trusted predicate live(value: &Marker);
        extern trusted func opaque();
        extern trusted func external() -> DataPointer<Owner>;
        extern trusted func publish(pos pointer: DataPointer<Owner>);
        trusted func make() -> Owner
            ensures(trusted live(&result.marker))
        {
            return { marker = Marker {} };
        }
        func independent(pos anchor: &Owner) -> DataPointer<Owner>
        {
            return external();
        }
        func direct(pos owner: &Owner) -> DataPointer<Owner>
        {
            return core.memory.address_of<Owner>(owner);
        }
        func mixed(pos owner: &Owner, pos choose_input: bool) -> DataPointer<Owner>
        {
            if choose_input { return direct(owner); }
            return independent(owner);
        }
        async func independent_async(pos anchor: &Owner) -> DataPointer<Owner>
        {
            return independent(anchor);
        }
        async func mixed_async(pos owner: &Owner, pos choose_input: bool) -> DataPointer<Owner>
        {
            return mixed(owner, choose_input);
        }
        func inspect(pos owner: &Owner)
            requires(trusted live(&owner.marker))
            executes(pure, total)
        {
        }
    "#;

    let provider = compilation(source);

    for (pointer, valid) in [
        ("api.independent(&owner)", true),
        ("api.direct(&owner)", false),
        ("api.mixed(&owner, choose_input)", false),
        ("await api.independent_async(&owner)", true),
        ("await api.mixed_async(&owner, choose_input)", false),
    ] {
        let caller = format!(
            r#"
            trusted module app;
            using api;
            async func caller(pos choose_input: bool)
            {{
                let owner = api.make();
                api.publish({pointer});
                api.opaque();
                api.inspect(&owner);
            }}
        "#
        );

        for imported in [false, true] {
            let consumer = if imported {
                execution_consumer(
                    &provider,
                    &caller
                        .replace("using api;", "using example.package.api;")
                        .replace("api.", "example.package.api."),
                )
            } else {
                compilation_from_sources([source, &caller])
            };

            let diagnostics = consumer.check_diagnostics();

            assert_eq!(
                !diagnostics.has_errors(),
                valid,
                "imported={imported}, {pointer}: {diagnostics:#?}"
            );

            if !valid {
                bray_testing::assert_goal_state_diagnostic_kind(
                    diagnostics,
                    bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven,
                );
            }
        }
    }

    let contract =
        crate::compilation::result_dependencies::source_function_dependencies(&provider, "mixed");

    assert!(
        contract
            .borrows()
            .iter()
            .any(|relation| relation.entry().is_none()),
        "{contract:#?}"
    );

    assert!(
        contract
            .borrows()
            .iter()
            .any(|relation| relation.entry().is_some()),
        "{contract:#?}"
    );

    assert!(
        contract
            .borrows()
            .iter()
            .all(|relation| !relation.is_exact()),
        "{contract:#?}"
    );
}

#[test]
fn completed_composite_values_drop_only_the_obsolete_frame_capture() {
    let source = r#"
        trusted module api;
        using core.memory;
        struct Marker {}
        struct Owner { marker: Marker; }
        struct View { pointer: DataPointer<Owner>; }
        struct DeferredView { pointer: DataPointer<Owner>; computation: Future<View>; }
        trusted predicate live(value: &Marker);
        extern trusted func opaque();
        extern trusted func external() -> DataPointer<Owner>;
        extern trusted func publish(pos value: View);
        extern trusted func publish_deferred(pos value: DeferredView);
        trusted func make() -> Owner
            ensures(trusted live(&result.marker))
        {
            return { marker = Marker {} };
        }
        func independent(pos anchor: &mut Owner) -> View
        {
            return { pointer = external() };
        }
        func direct(pos owner: &mut Owner) -> View
        {
            return { pointer = core.memory.address_of_mut<Owner>(owner) };
        }
        async func independent_async(pos anchor: &mut Owner) -> View
        {
            return independent(anchor);
        }
        async func direct_async(pos owner: &mut Owner) -> View
        {
            return direct(owner);
        }
        func deferred(pos owner: &mut Owner) -> Future<View>
        {
            return direct_async(owner);
        }

        func inspect(pos owner: &Owner)
            requires(trusted live(&owner.marker))
            executes(pure, total)
        {
        }
    "#;

    let provider = compilation(source);

    let completion = crate::compilation::result_dependencies::source_function_dependencies(
        &provider,
        "independent_async",
    );

    assert!(
        completion
            .borrows()
            .iter()
            .any(|relation| relation.entry().is_none()),
        "the body publishes the independent nested result origin: {completion:#?}"
    );

    for (body, valid) in [
        ("api.publish(api.independent(&mut owner));", true),
        ("api.publish(api.direct(&mut owner));", false),
        (
            "api.publish(await api.independent_async(&mut owner));",
            true,
        ),
        ("api.publish(await api.direct_async(&mut owner));", false),
        (
            "let pending = api.independent_async(&mut owner); let moved = pending; api.publish(await moved);",
            true,
        ),
        (
            "let pending = api.direct_async(&mut owner); let moved = pending; api.publish(await moved);",
            false,
        ),
        (
            "api.publish_deferred(api.DeferredView { pointer = api.external(), computation = api.deferred(&mut owner) });",
            false,
        ),
    ] {
        let caller = format!(
            r#"
            trusted module app;
            using api;
            async func caller()
            {{
                let mut owner = api.make();
                {body}
                api.opaque();
                api.inspect(&owner);
            }}
        "#
        );

        for imported in [false, true] {
            let consumer = if imported {
                execution_consumer(
                    &provider,
                    &caller
                        .replace("using api;", "using example.package.api;")
                        .replace("api.", "example.package.api."),
                )
            } else {
                compilation_from_sources([source, &caller])
            };

            let diagnostics = consumer.check_diagnostics();

            assert_eq!(
                !diagnostics.has_errors(),
                valid,
                "imported={imported}, {body}: {diagnostics:#?}, completion={completion:#?}"
            );

            if !valid {
                bray_testing::assert_goal_state_diagnostic_kind(
                    diagnostics,
                    bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven,
                );
            }
        }
    }
}
