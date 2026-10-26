use super::fixtures::{compilation, execution_consumer, export, implementation_dependency};

#[test]
fn imported_unproved_requirements_reach_concrete_predicate_helpers() {
    let provider = compilation(
        r#"
        module api;

        predicate above(value: i32, limit: i32) = value > limit;

        func selected<T>(pos value: i32, pos limit: i32, marker: &T)
            requires(above(value, limit), limit >= 0)
        {
            marker;
        }
    "#,
    );

    let consumer = crate::test_support::compilation_with_dependencies(
        r#"
        module app;

        using example.package.api;

        func caller(pos value: i32, pos limit: i32)
        {
            let marker: bool = true;

            example.package.api.selected<bool>(value, limit, marker = &marker);
        }

        func main()
        {
            caller(2, 1);
        }
    "#,
        [implementation_dependency(export(&provider))],
    );

    assert!(
        !consumer.check_diagnostics().has_errors(),
        "{:#?}",
        consumer.check_diagnostics()
    );

    let count = consumer
        .codegen_instance_count_for_test(|key| matches!(key, bray_ir::MirUnitKey::Predicate(_)))
        .expect("imported requirement helpers must reach concrete code generation");

    assert_eq!(count, 3);
}

#[test]
fn requirement_templates_preserve_named_argument_mapping() {
    let provider = compilation(
        r#"
        module api;

        const func order(pos first: i32, second: i32) -> bool
            executes(pure, total)
        {
            return first > second;
        }

        func selected(pos value: i32, pos limit: i32)
            requires(order(second = limit, first = value)) {}
    "#,
    );

    let consumer = crate::test_support::compilation_with_dependencies(
        r#"
        module app;

        using example.package.api;

        func caller(pos value: i32, pos limit: i32)
        {
            example.package.api.selected(value, limit);
        }

        func main()
        {
            caller(2, 1);
        }
    "#,
        [implementation_dependency(export(&provider))],
    );

    assert!(
        !consumer.check_diagnostics().has_errors(),
        "{:#?}",
        consumer.check_diagnostics()
    );

    let count = consumer
        .codegen_instance_count_for_test(|key| matches!(key, bray_ir::MirUnitKey::Predicate(_)))
        .expect("imported requirement helpers must reach concrete code generation");

    assert_eq!(count, 1);
}

#[test]
fn imported_completion_observes_the_mutated_receiver() {
    let provider = compilation(
        r#"
        module api;

        struct Resource
        {
            mut open: bool;
            predicate complete(value: &Resource) = !value.open;

            construct() -> Result<Self, bool>
            {
                return Ok(
                    {
                        open = true
                    }
                );
            }

            mut func close(pos fail: bool) -> Result<unit, bool>
                executes(total)
                ensures(result matches Error(_) || Resource.complete(value = &self))
            {
                if fail
                {
                    return Error(false);
                }

                self.open = false;

                return Ok(unit);
            }

            finalize() -> Result<unit, bool>
                when(Resource.complete(value = &self))
                {
                    executes(pure, total)
                    ensures(result matches Ok(_))
                }
            {
                if self.open
                {
                    return Error(false);
                }

                return Ok(unit);
            }
        }

        func require_ok<T, E>(pos input: Result<T, E>) -> T
            when(input matches Error(_))
            {
                ensures(false)
            }
        {
            match consume input
            {
                case Ok(value) { return value; }
                case Error(error) { panic("failed"); }
            }
        }
    "#,
    );

    let dependency = implementation_dependency(export(&provider));

    for extracted in [false, true] {
        for valid in [true, false] {
            let mutation = if valid { "" } else { "value.open = true;" };

            let initializer = if extracted {
                "example.package.api.require_ok<example.package.api.Resource, bool>(example.package.api.Resource())"
            } else {
                "{ open = true }"
            };

            let source = format!(
                r#"
            module app;
            using example.package.api;

            func caller(pos fail: bool)
            {{
                let mut value: example.package.api.Resource = {initializer};
                example.package.api.require_ok<unit, bool>(value.close(fail));
                {mutation}
            }}

            func main()
            {{
                caller(false);
            }}
        "#
            );

            let consumer =
                crate::test_support::compilation_with_dependencies(&source, [dependency.clone()]);

            let diagnostics = consumer.check_diagnostics();

            if valid {
                assert!(!diagnostics.has_errors(), "{diagnostics:?}");
            } else {
                bray_testing::assert_goal_state_diagnostic_kind(
                    diagnostics,
                    bray_diagnostics::DiagnosticKind::CheckingUnresolvedFinalization,
                );
            }
        }
    }
}

#[test]
fn wide_division_requirement_helpers_preserve_block_local_values() {
    assert_composed_requirement_helpers(
        "divisor > 0 && ((value + 1) * (value + 1)) / divisor > limit",
    );
}

#[test]
fn power_requirement_helpers_preserve_block_local_values() {
    assert_composed_requirement_helpers("(value + 1) ** 3 > limit");
}

#[test]
fn shift_requirement_helpers_preserve_block_local_values() {
    assert_composed_requirement_helpers("(value + 1) >> 2 > limit");
}

#[test]
fn callable_requirement_helpers_adapt_exact_arguments() {
    assert_composed_requirement_helpers(
        "value < 2147483647 && ordered(first = value + 1, second = limit)",
    );
}

#[test]
fn named_predicate_requirement_helpers_adapt_exact_arguments() {
    assert_composed_requirement_helpers(
        "value < 2147483647 && above(value = value + 1, limit = limit)",
    );
}

fn assert_composed_requirement_helpers(requirement: &str) {
    let provider = compilation(&format!(
        r#"
            module api;

            const func ordered(pos first: i128, second: i128) -> bool
                executes(pure, total)
            {{
                return first > second;
            }}

            predicate above(value: i128, limit: i128) = value > limit;

            func selected(pos value: i128, divisor: i128, limit: i128)
                requires({requirement})
            {{
            }}
        "#
    ));

    assert!(
        !provider.check_diagnostics().has_errors(),
        "{requirement}: {:?}",
        provider.check_diagnostics()
    );

    let consumer = crate::test_support::compilation_with_dependencies(
        r#"
            module app;

            using example.package.api;

            func caller(pos value: i128, divisor: i128, limit: i128)
            {
                example.package.api.selected(value, divisor = divisor, limit = limit);
            }

            func main()
            {
                caller(2, divisor = 1, limit = 1);
            }
        "#,
        [implementation_dependency(export(&provider))],
    );

    assert!(
        !consumer.check_diagnostics().has_errors(),
        "{requirement}: {:?}",
        consumer.check_diagnostics()
    );

    assert!(
        consumer
            .codegen_instance_count_for_test(|key| matches!(key, bray_ir::MirUnitKey::Predicate(_)))
            .expect("composed imported requirement retains valid helper MIR")
            > 0
    );
}

#[test]
fn sequence_requirement_helpers_keep_the_checked_receiver() {
    let provider = compilation(
        r#"
        module api;

        func selected(pos values: &[u8], pos limit: usize)
            requires(values.length() > limit) {}
    "#,
    );

    assert!(
        !provider.check_diagnostics().has_errors(),
        "{:?}",
        provider.check_diagnostics()
    );

    let consumer = crate::test_support::compilation_with_dependencies(
        r#"
        module app;

        using example.package.api;

        callable Action = func(pos values: &[u8], pos limit: usize)
            requires(values.length() > limit);

        func direct(pos values: &[u8], pos limit: usize)
        {
            example.package.api.selected(values, limit);
        }

        func indirect(pos action: Action, pos values: &[u8], pos limit: usize)
        {
            action(values, limit);
        }
    "#,
        [implementation_dependency(export(&provider))],
    );

    assert!(
        !consumer.check_diagnostics().has_errors(),
        "{:?}",
        consumer.check_diagnostics()
    );

    assert_eq!(consumer.codegen_instance_count_for_test(|key| matches!(key, bray_ir::MirUnitKey::Predicate(_)))
        .expect("source, imported and erased sequence requirements retain the selected slice receiver"), 2);
}

#[test]
fn imported_generic_requirements_preserve_all_checking_domains() {
    let provider = compilation(
        r#"
        trusted module api;

        trusted predicate live();

        func selected<T>(pos value: i32, marker: &T)
            requires(value > 0, trusted live(), blocking_execution())
        {
            marker;
        }
    "#,
    );

    let consumer = crate::test_support::compilation_with_dependencies(
        r#"
        module app;

        using example.package.api;

        func caller(pos value: i32)
            requires(trusted example.package.api.live(), blocking_execution())
        {
            let marker: bool = true;

            example.package.api.selected<bool>(value, marker = &marker);
        }
    "#,
        [implementation_dependency(export(&provider))],
    );

    assert!(
        !consumer.check_diagnostics().has_errors(),
        "{:#?}",
        consumer.check_diagnostics()
    );

    let selections = consumer
        .semantic_selections(crate::test_support::source_function_body_key(
            &consumer, "caller",
        ))
        .expect("fixture callable must have semantic selections");

    let domains = selections
        .value()
        .entries()
        .iter()
        .filter_map(|entry| match entry.selection() {
            bray_bound_tree::SemanticSelection::Call(call) => {
                Some(call.phase_behaviors().invocation().predicate_requirements())
            }
            _ => None,
        })
        .flatten()
        .map(|predicate| predicate.domain())
        .collect::<std::collections::BTreeSet<_>>();

    assert_eq!(
        domains,
        [
            bray_symbols::PredicateDomain::Value,
            bray_symbols::PredicateDomain::Trusted,
            bray_symbols::PredicateDomain::Execution
        ]
        .into_iter()
        .collect()
    );
}

#[test]
fn string_requirement_lowering_preserves_imported_and_erased_checks() {
    let provider = compilation(
        r#"
        module api;

        func selected(pos value: &string)
            requires(value == "ok", value != "bad") {}
    "#,
    );

    let source = r#"
        module app;

        using example.package.api;

        callable Action = func(pos value: &string)
            requires(value == "ok", value != "bad");

        func direct(pos value: &string)
        {
            example.package.api.selected(value);
        }

        func indirect(pos action: Action, pos value: &string)
        {
            action(value);
        }
    "#;

    let consumer = crate::test_support::compilation_with_dependencies(
        source,
        [implementation_dependency(export(&provider))],
    );

    assert!(
        !consumer.check_diagnostics().has_errors(),
        "{:#?}",
        consumer.check_diagnostics()
    );

    for (name, erased) in [("direct", false), ("indirect", true)] {
        let key = crate::test_support::source_function_body_key(&consumer, name);
        let lowered = consumer.lowered_unit(key).expect("fixture callable must lower successfully");

        assert!(
            !lowered.diagnostics().has_errors(),
            "{:#?}",
            lowered.diagnostics()
        );

        let mir = lowered.value().as_ref().expect("fixture optional value must be present").mir().expect("fixture instance must retain its MIR");

        let requirements = mir
            .operations()
            .iter()
            .filter_map(|operation| match operation.kind() {
                bray_ir::MirOperationKind::Call(call) => match call.target() {
                    bray_ir::MirCallTarget::Predicate(reference) => Some(reference),
                    _ => None,
                },
                _ => None,
            })
            .collect::<Vec<_>>();

        assert!(!requirements.is_empty());

        assert!(requirements.iter().all(|reference| matches!(
            reference,
            bray_ir::MirPredicateReference::Condition { .. }
        ) == erased));
    }
}

#[test]
fn operator_templates_preserve_selected_and_constrained_dispatch() {
    let provider = compilation(
        r#"
        module api;

        @copy
        struct Value
        {
            value: i32;
        }

        impl ValueEqual = Value(Equatable<Value>)
        {
            func equals(pos rhs: &Value) -> bool
                executes(pure, total)
            {
                return self.value == rhs.value;
            }
        }

        impl ValueCompare = Value(Comparable<Value>)
        {
            func compare(pos rhs: &Value) -> Ordering
                executes(pure, total)
            {
                if self.value < rhs.value
                {
                    return Ordering.Less;
                }

                if self.value > rhs.value
                {
                    return Ordering.Greater;
                }

                return Ordering.Equal;
            }
        }

        impl ValueNegate = Value(Negate)
        {
            type Output = i32;

            func negate() -> i32
                executes(pure, total)
            {
                return self.value;
            }
        }

        func selected(
            pos left: Value,
            pos right: Value,
            different: bool = left != right,
            less: bool = left < right,
            at_most: bool = left <= right,
            greater: bool = right > left,
            at_least: bool = right >= left,
            negated: i32 = -left,
        )
        {
        }

        func generic<T>(pos left: T, pos right: T, different: bool = left != right, less: bool = left < right)
            with(T: Equatable<T>, T: Comparable<T>) {}
    "#,
    );

    assert!(
        !provider.check_diagnostics().has_errors(),
        "{:#?}",
        provider.check_diagnostics()
    );

    let bundle = export(&provider);

    let dispatches = bundle
        .semantics()
        .checked_templates()
        .iter()
        .flat_map(|template| template.nodes())
        .filter_map(|node| match node.operation() {
            bray_package_interface::InterfaceCheckedTemplateOperation::Call {
                dispatch, ..
            } => dispatch.as_ref(),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert!(dispatches.iter().any(|dispatch| matches!(
        dispatch,
        bray_bound_tree::CheckedTemplateDispatch::Implementation(..)
    )));

    assert!(dispatches.iter().any(|dispatch| matches!(
        dispatch,
        bray_bound_tree::CheckedTemplateDispatch::Constraint { .. }
    )));

    let consumer = crate::test_support::compilation_with_dependencies(
        r#"
        module app;

        using example.package.api;
        using example.package.api.ValueEqual;
        using example.package.api.ValueCompare;

        func caller(pos left: example.package.api.Value, pos right: example.package.api.Value)
        {
            example.package.api.selected(left, right);
            example.package.api.generic<example.package.api.Value>(left, right);
        }
    "#,
        [implementation_dependency(bundle)],
    );

    assert!(
        !consumer.check_diagnostics().has_errors(),
        "{:#?}",
        consumer.check_diagnostics()
    );

    assert!(
        consumer
            .codegen_instance_count_for_test(|key| matches!(
                key,
                bray_ir::MirUnitKey::ImportedExecutable(_)
            ))
            .expect("imported default helpers retain concrete and abstract selected dispatch")
            > 0
    );
}

#[test]
fn imported_predicate_helpers_share_const_body_defaults_and_internal_dependencies() {
    let provider = compilation(
        r#"
        module api;

        internal const func positive(pos value: u8) -> bool
        {
            return value > 0;
        }

        const func selected(pos value: u8, limit: u8 = 1) -> bool
        {
            if value >= limit
            {
                return positive(value);
            }

            return false;
        }

        func require_selected(pos value: u8)
            requires(selected(value)) {}
    "#,
    );

    let consumer = crate::test_support::compilation_with_dependencies(
        r#"
        module app;

        using example.package.api;

        func caller(pos value: u8)
        {
            example.package.api.require_selected(value);
        }

        func main()
        {
            caller(3);
        }
    "#,
        [implementation_dependency(export(&provider))],
    );

    assert!(
        !consumer.check_diagnostics().has_errors(),
        "{:#?}",
        consumer.check_diagnostics()
    );

    let helpers = consumer
        .codegen_instance_count_for_test(|key| matches!(key, bray_ir::MirUnitKey::Predicate(_)))
        .expect("admitted imported predicate helpers must reach actual native realization");

    assert!(helpers > 0);
}

#[test]
fn default_observations_substitute_preceding_inputs_before_checking_entry() {
    let declaration = r#"
        const func selected(pos value: u8, pos selected: u8 = value) -> u8
            requires(selected > 0)
        {
            return selected;
        }
    "#;

    let provider = compilation(&format!(
        r#"
        module api;

        {declaration}
    "#
    ));

    for (condition, valid) in [("value > 0", true), ("value == 0", false)] {
        let body = format!(
            r#"
            func caller(pos value: u8)
                requires({condition})
                executes(total)
            {{
                selected(value);
            }}
        "#
        );

        let source = compilation(&format!(
            r#"
            module api;

            {declaration}

            {body}
        "#
        ));

        let imported = super::fixtures::execution_consumer(
            &provider,
            &format!(
                r#"
            module app;

            using example.package.api;

            {}
        "#,
                body.replace("selected(value)", "example.package.api.selected(value)")
            ),
        );

        for checked in [&source, &imported] {
            assert_eq!(
                !checked.check_diagnostics().has_errors(),
                valid,
                "{condition}: {:#?}",
                checked.check_diagnostics()
            );
        }
    }
}

#[test]
fn floating_self_comparisons_do_not_satisfy_unconditional_call_requirements() {
    for condition in [
        "value == value",
        "value <= value",
        "value >= value",
        "!(value != value)",
    ] {
        let declaration = format!(
            r#"
                module api;

                func selected(pos value: r64)
                    requires({condition})
                    executes(total)
                {{
                }}
            "#
        );

        let provider = compilation(&declaration);

        assert!(
            !provider.check_diagnostics().has_errors(),
            "{:?}",
            provider.check_diagnostics()
        );

        let consumer = super::fixtures::execution_consumer(
            &provider,
            r#"
                module app;

                using example.package.api;

                func caller(pos value: r64)
                    executes(total)
                {
                    example.package.api.selected(value);
                }
            "#,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            consumer.check_diagnostics(),
            bray_diagnostics::DiagnosticKind::CheckingExecutionGuaranteeNotProven,
        );
    }
}

#[test]
fn byte_address_preserves_only_the_supplied_allocation_authority() {
    for (argument, repeated, valid) in [
        ("pointer", false, true),
        ("other", false, false),
        ("pointer", true, false),
    ] {
        let second = if repeated {
            "trusted core.memory.deallocate(address, bytes = 4, align = 2);"
        } else {
            ""
        };

        let source = format!(
            r#"
                trusted module app;

                using core.memory;

                trusted func release(pos pointer: DataPointer<u16>, other: DataPointer<u16>)
                    requires(
                        trusted core.memory.owned_allocation(
                            pointer = core.memory.byte_address<u16>(pointer),
                            bytes = 4,
                            align = 2,
                        ),
                    )
                    uses(manual_alloc)
                {{
                    let address = core.memory.byte_address<u16>({argument});

                    trusted core.memory.deallocate(address, bytes = 4, align = 2);
                    {second}
                }}
            "#
        );

        let checked = compilation(&source);

        if valid {
            assert!(
                !checked.check_diagnostics().has_errors(),
                "{:#?}",
                checked.check_diagnostics()
            );
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                checked.check_diagnostics(),
                bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn byte_address_rejects_function_pointers() {
    let checked = compilation(
        r#"
            module app;

            using core.memory;

            callable Callback = @abi(c) func();

            func caller(pos pointer: FunctionPointer<Callback>)
            {
                let address = core.memory.byte_address<Callback>(pointer);
            }
        "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        checked.check_diagnostics(),
        bray_diagnostics::DiagnosticKind::CheckingIncompatibleExpressionType,
    );
}

#[test]
fn byte_address_cannot_hide_a_function_pointer_in_a_generic_wrapper() {
    let checked = compilation(
        r#"
            module app;

            using core.memory;

            callable Callback = @abi(c) func();

            const func project<T>(pos pointer: DataPointer<T>) -> DataPointer<u8>
            {
                return core.memory.byte_address<T>(pointer);
            }

            func caller(pos pointer: FunctionPointer<Callback>)
            {
                let address = project<Callback>(pointer);
            }
        "#,
    );

    assert!(
        checked.check_diagnostics().has_errors(),
        "a generic data-pointer parameter must reject a raw function pointer",
    );
}

#[test]
fn byte_address_accepts_callable_storage_through_source_and_imported_generics() {
    let declaration = r#"
        module api;

        using core.memory;

        const func project<T>(pos pointer: DataPointer<T>) -> DataPointer<u8>
        {
            return core.memory.byte_address<T>(pointer);
        }
    "#;

    let provider = compilation(declaration);

    let body = r#"
        callable Callback = @abi(c) func();

        func caller(pos pointer: DataPointer<Callback>) -> DataPointer<u8>
        {
            return project<Callback>(pointer);
        }
    "#;

    let source = compilation(&format!("{declaration}\n{body}"));

    let imported = super::fixtures::execution_consumer(
        &provider,
        &format!(
            r#"
                module app;

                using example.package.api;

                {}
            "#,
            body.replace("project<Callback>", "example.package.api.project<Callback>")
        ),
    );

    for checked in [&source, &imported] {
        assert!(
            !checked.check_diagnostics().has_errors(),
            "{:#?}",
            checked.check_diagnostics()
        );
    }
}

#[test]
fn data_pointer_to_a_callable_addresses_its_stored_value() {
    let checked = compilation(
        r#"
            trusted module app;

            using core.memory;

            callable Callback = @abi(c) func();

            trusted func load(pos value: &Callback) -> Callback
                uses(raw_memory)
            {
                let pointer: DataPointer<Callback> = core.memory.address_of<Callback>(value);

                return trusted core.memory.read<Callback>(pointer);
            }
        "#,
    );

    assert!(
        !checked.check_diagnostics().has_errors(),
        "{:#?}",
        checked.check_diagnostics()
    );
}

#[test]
fn checked_body_sequences_keep_discarded_calls_reachable_after_export() {
    let provider = compilation(
        r#"
            module api;

            const func echo(pos value: u8) -> u8
            {
                return value;
            }

            const func observed(pos value: u8) -> bool
            {
                let snapshot = echo(value);
                echo(value);

                if value == 0
                {
                    return snapshot == 0;
                }

                return snapshot == value;
            }

            func selected(pos value: u8) requires(observed(value))
            {
            }
        "#,
    );

    assert!(
        !provider.check_diagnostics().has_errors(),
        "{:#?}",
        provider.check_diagnostics()
    );

    let bundle = export(&provider);
    let mut covered = false;

    for body in bundle.constant_callable_bodies() {
        let template = body.template();

        let calls = template
            .nodes()
            .iter()
            .filter(|node| {
                matches!(
                    node.operation(),
                    bray_package_interface::InterfaceCheckedTemplateOperation::Call { .. }
                )
            })
            .count();

        if calls < 2 {
            continue;
        }

        let mut pending = vec![template.result()];
        let mut reachable = std::collections::BTreeSet::new();

        while let Some(node) = pending.pop() {
            if !reachable.insert(node) {
                continue;
            }

            template.nodes()[usize::try_from(node.raw()).expect("fixture value must fit the requested representation")]
                .operation()
                .try_for_each_node_reference(|child| {
                    pending.push(child);

                    Ok::<_, std::convert::Infallible>(())
                })
                .expect("fixture for each node reference must succeed");
        }

        assert_eq!(
            reachable
                .iter()
                .filter(|node| matches!(
                    template.nodes()[usize::try_from(node.raw()).expect("fixture value must fit the requested representation")].operation(),
                    bray_package_interface::InterfaceCheckedTemplateOperation::Call { .. }
                ))
                .count(),
            calls
        );

        covered = true;
    }

    assert!(
        covered,
        "export must retain the body containing both evaluated calls"
    );

    let consumer = crate::test_support::compilation_with_dependencies(
        r#"
            module app;

            using example.package.api;

            func caller(pos value: u8)
            {
                example.package.api.selected(value);
            }

            func main()
            {
                caller(1);
            }
        "#,
        [implementation_dependency(bundle)],
    );

    assert!(
        !consumer.check_diagnostics().has_errors(),
        "{:#?}",
        consumer.check_diagnostics()
    );

    assert!(
        consumer
            .codegen_instance_count_for_test(|_| true)
            .expect("sequenced imported predicates must realize native bodies")
            > 0
    );
}

#[test]
fn compound_execution_requirements_do_not_emit_runtime_authority_queries() {
    let provider = compilation(
        r#"
            module api;

            func selected(pos ready: bool)
                requires(ready || blocking_execution())
            {
            }
        "#,
    );

    let consumer = crate::test_support::compilation_with_dependencies(
        r#"
            module app;

            using example.package.api;

            func guarded(pos ready: bool)
                requires(ready)
            {
                example.package.api.selected(ready);
            }

            func main()
            {
                guarded(true);
            }
        "#,
        [implementation_dependency(export(&provider))],
    );

    assert!(
        !consumer.check_diagnostics().has_errors(),
        "{:#?}",
        consumer.check_diagnostics()
    );

    assert_eq!(
        consumer
            .codegen_instance_count_for_test(|key| matches!(key, bray_ir::MirUnitKey::Predicate(_)))
            .expect("established execution requirements must reach native realization"),
        0
    );
}

#[test]
fn imported_nested_array_and_nullable_conversions_evaluate_and_realize() {
    let provider = compilation(
        r#"
            module api;

            predicate widened(value: ([u8; 2], u8?)) =
                (value as ([u16; 2], u16?)).0[1] > 0;

            const func check(pos value: ([u8; 2], u8?)) -> bool
                executes(pure, total)
            {
                return (value as ([u16; 2], u16?)).0[1] > 0;
            }

            func selected(pos value: ([u8; 2], u8?))
                requires(widened(value))
            {
            }
        "#,
    );

    assert!(
        !provider.check_diagnostics().has_errors(),
        "{:#?}",
        provider.check_diagnostics()
    );

    let bundle = export(&provider);

    assert!(
        bundle
            .semantics()
            .checked_templates()
            .iter()
            .any(|template| template.nodes().iter().any(|node| matches!(
                node.operation(),
                bray_package_interface::InterfaceCheckedTemplateOperation::ArrayTransform { .. }
            )))
    );

    let consumer = crate::test_support::compilation_with_dependencies(
        r#"
            module app;

            using example.package.api;

            const CHECKED: bool = example.package.api.check(([0, 1], none));

            func caller(pos value: ([u8; 2], u8?))
                requires(CHECKED)
            {
                example.package.api.selected(value);
            }

            func main()
            {
                caller(([0, 1], none));
            }
        "#,
        [implementation_dependency(bundle)],
    );

    assert!(
        !consumer.check_diagnostics().has_errors(),
        "{:#?}",
        consumer.check_diagnostics()
    );

    assert!(
        consumer
            .codegen_instance_count_for_test(|key| matches!(key, bray_ir::MirUnitKey::Predicate(_)))
            .expect("imported aggregate conversion predicates reach concrete native helpers")
            > 0
    );
}

#[test]
fn checked_array_conversion_graph_size_is_independent_of_array_length() {
    let mut sizes = Vec::new();

    for length in [2, 4096] {
        let provider = compilation(&format!(
            r#"
                module api;

                predicate widened(value: [u8; {length}]) =
                    (value as [u16; {length}])[0] > 0;
            "#,
        ));

        let bundle = export(&provider);

        let template = bundle
            .semantics()
            .checked_templates()
            .iter()
            .find(|template| {
                template.nodes().iter().any(|node| matches!(node.operation(),
                bray_package_interface::InterfaceCheckedTemplateOperation::ArrayTransform { .. }))
            })
            .expect("array conversion retains one element computation");

        sizes.push(template.nodes().len());
    }

    assert_eq!(sizes[0], sizes[1]);
}

#[test]
fn imported_const_local_updates_preserve_branch_values_and_snapshots() {
    let provider = compilation(
        r#"
            module api;

            const func selected(pos first: bool, pos second: bool) -> i32
            {
                let mut value: i32 = 1;
                let original: i32 = value;

                if first
                {
                    value = 2;
                }
                else if second
                {
                    value = 3;
                }
                else
                {
                    value = 4;
                }

                match first
                {
                    case true
                    {
                        value = 5;
                    }
                    case false {}
                }

                return original * 10 + value;
            }

            func require_selected(pos first: bool, pos second: bool)
                requires(selected(first, second) == 15)
            {
            }
        "#,
    );

    assert!(
        !provider.check_diagnostics().has_errors(),
        "{:#?}",
        provider.check_diagnostics()
    );

    let consumer = crate::test_support::compilation_with_dependencies(
        r#"
            module app;

            using example.package.api;

            const FIRST: i32 = example.package.api.selected(true, false);
            const SECOND: i32 = example.package.api.selected(false, true);
            const LAST: i32 = example.package.api.selected(false, false);

            func main()
                requires(FIRST == 15, SECOND == 13, LAST == 14)
            {
                example.package.api.require_selected(true, false);
            }
        "#,
        [implementation_dependency(export(&provider))],
    );

    assert!(
        !consumer.check_diagnostics().has_errors(),
        "{:#?}",
        consumer.check_diagnostics()
    );

    assert!(
        consumer
            .codegen_instance_count_for_test(|key| matches!(key, bray_ir::MirUnitKey::Predicate(_)))
            .expect("functional local updates reach concrete native predicate helpers")
            > 0
    );
}

#[test]
fn const_local_slot_updates_do_not_admit_writes_through_entry_borrows() {
    let provider = compilation(
        r#"
            module api;

            const func invalid(pos target: &mut i32)
            {
                target = 7;
            }
        "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        provider.check_diagnostics(),
        bray_diagnostics::DiagnosticKind::CheckingExecutionGuaranteeNotProven,
    );
}

#[test]
fn imported_const_machine_updates_wrap_before_later_contract_arithmetic() {
    let provider = compilation(
        r#"
            module api;

            const func advance(pos initial: u8) -> u8
            {
                let mut value: u8 = initial;
                value += 1;
                return value;
            }

            const func power(pos value: u8, pos exponent: u8) -> u8
            {
                return value ** exponent;
            }

            func selected(pos value: u8)
                requires(advance(value) == 0)
            {
            }
        "#,
    );

    assert!(
        !provider.check_diagnostics().has_errors(),
        "{:#?}",
        provider.check_diagnostics()
    );

    let bundle = export(&provider);

    assert!(bundle.constant_callable_bodies().iter().any(|body| {
        body.template().nodes().iter().any(|node| {
            matches!(
                node.operation(),
                bray_package_interface::InterfaceCheckedTemplateOperation::MachineBinary { .. }
            )
        })
    }));

    let consumer = crate::test_support::compilation_with_dependencies(
        r#"
            module app;

            using example.package.api;

            const WRAPPED: u8 = example.package.api.advance(255);
            const POWER: u8 = example.package.api.power(2, 255);

            func main()
                requires(WRAPPED == 0, POWER == 0)
            {
                example.package.api.selected(255);
            }
        "#,
        [implementation_dependency(bundle)],
    );

    assert!(
        !consumer.check_diagnostics().has_errors(),
        "{:#?}",
        consumer.check_diagnostics()
    );

    assert!(
        consumer
            .codegen_instance_count_for_test(|key| matches!(key, bray_ir::MirUnitKey::Predicate(_)))
            .expect("machine updates reach native predicate helpers")
            > 0
    );
}

#[test]
fn machine_overflow_does_not_certify_an_exact_postcondition() {
    let invalid = compilation(
        r#"
            module api;

            func increment(pos value: u8) -> u8
                ensures(result > value)
            {
                return value + 1;
            }
        "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        invalid.check_diagnostics(),
        bray_diagnostics::DiagnosticKind::CheckingExecutionGuaranteeNotProven,
    );

    let valid = compilation(
        r#"
            module api;

            func increment(pos value: u8) -> u8
                requires(value < 255)
                ensures(result == value + 1)
            {
                return value + 1;
            }
        "#,
    );

    assert!(
        !valid.check_diagnostics().has_errors(),
        "{:#?}",
        valid.check_diagnostics()
    );
}

#[test]
fn generic_nullable_requirements_keep_instantiated_literal_types() {
    let provider_source = r#"
        module api;

        const func accepted<T>(pos value: &(T?)) -> bool
            requires(value matches ?_)
        {
            return true;
        }
        func local(pos value: &(u8?)) executes(total)
        {
            if value matches ?_
            {
                _ = accepted<u8>(value);
            }
        }
    "#;

    let provider = compilation(provider_source);

    assert!(
        !provider.check_diagnostics().has_errors(),
        "{:#?}",
        provider.check_diagnostics()
    );

    let consumer = crate::test_support::compilation_with_dependencies(
        r#"
        module app;

        using example.package.api;

        func caller(pos value: &(u8?)) executes(total)
        {
            if value matches ?_
            {
                _ = example.package.api.accepted<u8>(value);
            }
        }
    "#,
        [implementation_dependency(export(&provider))],
    );

    assert!(
        !consumer.check_diagnostics().has_errors(),
        "{:#?}",
        consumer.check_diagnostics()
    );
}

#[test]
fn staged_reborrows_follow_source_and_imported_result_contracts() {
    for (result_type, result, body, valid) in [
        (
            "&mut Value",
            "value",
            "let left = selected(value); let right = selected(value); left.state = 1; right.state = 2;",
            false,
        ),
        (
            "DataPointer<Value>",
            "core.memory.address_of_mut<Value>(value)",
            "let left = selected(value); value.state = 1; let right = selected(value); _ = (left, right);",
            true,
        ),
    ] {
        let declaration = format!(
            r#"
            trusted module api;

            using core.memory;

            struct Value
            {{
                mut state: u8;
            }}

            func selected(pos value: &mut Value, ignored: bool = false) -> {result_type}
            {{
                return {result};
            }}
        "#
        );

        let caller = format!(
            r#"
            func caller(pos value: &mut Value)
            {{
                {body}
            }}
        "#
        );

        let provider = compilation(&declaration);
        let source = compilation(&format!("{declaration}\n{caller}"));

        let imported_caller = caller
            .replace("selected(", "example.package.api.selected(")
            .replace("&mut Value", "&mut example.package.api.Value");

        let imported = crate::test_support::compilation_with_dependencies(
            &format!(
                r#"
                module app;

                using example.package.api;

                {imported_caller}
            "#
            ),
            [implementation_dependency(export(&provider))],
        );

        for checked in [&source, &imported] {
            let diagnostics = checked.check_diagnostics();

            assert_eq!(
                !diagnostics.has_errors(),
                valid,
                "{declaration}\n{caller}: {diagnostics:#?}"
            );

            if !valid {
                bray_testing::assert_goal_state_diagnostic_kind(
                    diagnostics,
                    bray_diagnostics::DiagnosticKind::CheckingConflictingBorrow,
                );
            }
        }
    }
}

#[test]
fn joined_result_bounds_and_checked_arithmetic_compose_across_interfaces() {
    let declaration = r#"
        module api;

        func cleanup(pos kind: u32) -> usize
            requires(kind >= 1 && kind <= 4)
            ensures(result < 4)
            executes(pure, total)
        {
            return (kind - 1) as usize;
        }
    "#;

    let provider = compilation(declaration);
    let dependency = implementation_dependency(export(&provider));

    for (upper, valid) in [(3, true), (4, false)] {
        let caller = format!(
            r#"
            func selected(pos choice: bool, pos kind: u32) -> (i8, usize)
                requires(kind >= 1 && kind <= 4)
                ensures(result.0 < 4, result.1 < 6)
            {{
                return (
                    if choice
                    {{
                        yield {upper};
                    }}
                    else
                    {{
                        yield -2;
                    }},
                    cleanup(kind)
                );
            }}
        "#
        );

        let source = compilation(&format!(
            "{declaration}
{caller}"
        ));

        let imported = crate::test_support::compilation_with_dependencies(
            &format!(
                r#"
                module app;

                using example.package.api;

                {}
            "#,
                caller.replace("cleanup(kind)", "example.package.api.cleanup(kind)")
            ),
            [dependency.clone()],
        );

        for checked in [&source, &imported] {
            let diagnostics = checked.check_diagnostics();

            assert_eq!(
                !diagnostics.has_errors(),
                valid,
                "{caller}: {diagnostics:#?}"
            );

            if !valid {
                bray_testing::assert_goal_state_diagnostic_kind(
                    diagnostics,
                    bray_diagnostics::DiagnosticKind::CheckingExecutionGuaranteeNotProven,
                );
            }
        }
    }
}

#[test]
fn exact_memory_writes_establish_nullable_contents_across_interfaces() {
    for (slot, local, stored, returned, valid) in [
        ("storage", "", "none", "previous", true),
        ("storage", "", "previous", "none", false),
        (
            "&mut other",
            "let mut other: T? = none;",
            "none",
            "previous",
            false,
        ),
    ] {
        let source = format!(
            r#"
            trusted module api;

            using core.memory;

            trusted func take<T>(pos storage: &mut (T?)) -> T?
                ensures(storage matches none)
                executes(total)
                uses(raw_memory, unchecked_init)
            {{
                {local}
                let pointer: DataPointer<(T?)> = core.memory.address_of_mut<(T?)>({slot});
                let previous: T? = trusted core.memory.read<(T?)>(pointer);

                trusted core.memory.write<(T?)>(pointer, {stored});

                return {returned};
            }}
        "#
        );

        let provider = compilation(&source);

        assert_eq!(
            !provider.check_diagnostics().has_errors(),
            valid,
            "{source}: {:#?}",
            provider.check_diagnostics()
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                provider.check_diagnostics(),
                bray_diagnostics::DiagnosticKind::CheckingExecutionGuaranteeNotProven,
            );

            continue;
        }

        let consumer = execution_consumer(
            &provider,
            r#"
            trusted module app;

            using example.package.api;

            struct Record
            {
                mut value: u32?;
            }

            func clear(pos record: &mut Record)
                ensures(record.value matches none)
            {
                let previous = trusted example.package.api.take<u32>(&mut record.value);
            }
        "#,
        );

        assert!(
            !consumer.check_diagnostics().has_errors(),
            "{:#?}",
            consumer.check_diagnostics()
        );
    }
}

#[test]
fn protected_initializers_in_runtime_requirements_reach_native_operations() {
    for (storage, initializer) in [
        ("Uninit<u8>", "core.memory.uninit<u8>()"),
        ("Uninit<u8>", "core.memory.uninit_from<u8>(1)"),
        ("core.atomic.Atomic<u64>", "core.atomic.initialize<u64>(1)"),
    ] {
        let provider = compilation(&format!(
            r#"
            module api;

            using core.memory;
            using core.atomic;

            const func observed(pos storage: {storage}, pos value: u32) -> bool
            {{
                return value > 0;
            }}

            func selected(pos value: u32)
                requires(observed({initializer}, value))
            {{
            }}
        "#
        ));

        assert!(
            !provider.check_diagnostics().has_errors(),
            "{initializer}: {:#?}",
            provider.check_diagnostics()
        );

        let consumer = crate::test_support::compilation_with_dependencies(
            r#"
            module app;

            using example.package.api;

            func caller(pos value: u32)
            {
                example.package.api.selected(value);
            }

            func main()
            {
                caller(1);
            }
        "#,
            [implementation_dependency(export(&provider))],
        );

        assert!(
            !consumer.check_diagnostics().has_errors(),
            "{:#?}",
            consumer.check_diagnostics()
        );

        assert!(
            consumer
                .codegen_instance_count_for_test(|key| matches!(
                    key,
                    bray_ir::MirUnitKey::Predicate(_)
                ))
                .expect("protected initialization lowers to its existing native operation")
                > 0
        );
    }
}

#[test]
fn wrapping_power_uses_its_machine_domain_and_preserves_exact_predicate_limits() {
    let provider = compilation(
        r#"
        module api;

        const func wrapped(pos value: i32, pos exponent: i32) -> i32
            requires(exponent >= 0)
        {
            return value ** exponent;
        }
    "#,
    );

    assert!(
        !provider.check_diagnostics().has_errors(),
        "{:#?}",
        provider.check_diagnostics()
    );

    for (requirement, valid) in [("requires(exponent >= 0)", true), ("", false)] {
        let source = format!(
            r#"
            module app;

            using example.package.api;

            func caller(pos value: i32, pos exponent: i32) -> i32
                {requirement}
                executes(total)
            {{
                return example.package.api.wrapped(value, exponent);
            }}
        "#
        );

        let consumer = execution_consumer(&provider, &source);

        assert_eq!(
            !consumer.check_diagnostics().has_errors(),
            valid,
            "{source}: {:#?}",
            consumer.check_diagnostics()
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                consumer.check_diagnostics(),
                bray_diagnostics::DiagnosticKind::CheckingExecutionGuaranteeNotProven,
            );
        }
    }

    for source in [
        r#"
        module app;

        const func invalid(pos value: i32, pos exponent: i32) -> i32
        {
            return value ** exponent;
        }
    "#,
        r#"
        module app;

        func exact(pos value: i32, pos exponent: i32)
            requires(exponent >= 0 && value ** exponent >= 0)
        {
        }
    "#,
    ] {
        let checked = compilation(source);

        bray_testing::assert_goal_state_diagnostic_kind(
            checked.check_diagnostics(),
            bray_diagnostics::DiagnosticKind::CheckingExecutionGuaranteeNotProven,
        );
    }
}

#[test]
fn closed_callable_requirements_share_one_helper_across_caller_contexts() {
    let source = r#"
        module api;

        callable Required = func(pos value: bool)
            requires(value);

        public func first(pos operation: Required, pos value: bool)
        {
            operation(value);
        }

        public func second(pos operation: Required, pos value: bool)
        {
            operation(value);
        }

        func generic_first<T>(pos operation: Required, pos value: bool, marker: &T)
        {
            marker;
            operation(value);
        }

        func generic_second<T>(pos operation: Required, pos value: bool, marker: &T)
        {
            marker;
            operation(value);
        }

        public func generic_roots(pos operation: Required, pos value: bool)
        {
            let marker: i32 = 0;

            generic_first<i32>(operation, value, marker = &marker);
            generic_second<i32>(operation, value, marker = &marker);
        }
    "#;

    let provider = compilation(source);

    let consumer = crate::test_support::compilation_with_dependencies(
        r#"
        module app;

        using example.package.api;

        public func first(pos operation: example.package.api.Required, pos value: bool)
        {
            operation(value);
        }

        public func second(pos operation: example.package.api.Required, pos value: bool)
        {
            operation(value);
        }

        public func generic_roots(pos operation: example.package.api.Required, pos value: bool)
        {
            let marker: i32 = 0;

            example.package.api.generic_first<i32>(operation, value, marker = &marker);
            example.package.api.generic_second<i32>(operation, value, marker = &marker);
        }
    "#,
        [implementation_dependency(export(&provider))],
    );

    for compilation in [&provider, &consumer] {
        assert!(
            !compilation.check_diagnostics().has_errors(),
            "{:#?}",
            compilation.check_diagnostics()
        );

        assert_eq!(
            compilation
                .codegen_instance_count_for_test(|key| matches!(
                    key,
                    bray_ir::MirUnitKey::Predicate(_)
                ))
                .expect("closed requirements have one realization independently of their callers"),
            1
        );
    }
}
