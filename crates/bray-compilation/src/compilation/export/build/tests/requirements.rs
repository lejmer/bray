use super::fixtures::{compilation, export, implementation_dependency};

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
        .unwrap();

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
        let lowered = consumer.lowered_unit(key).unwrap();

        assert!(
            !lowered.diagnostics().has_errors(),
            "{:#?}",
            lowered.diagnostics()
        );

        let mir = lowered.value().as_ref().unwrap().mir().unwrap();

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
