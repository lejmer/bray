use bray_bound_tree::BoundUnitKey;
use bray_checker::{ExecutionObligation, ExecutionProperty};
use bray_diagnostics::DiagnosticBag;
use bray_source::SourceSpan;

use crate::compilation::Compilation;
use crate::fact::{CancellationToken, FactQueryError};

impl Compilation {
    pub(super) fn check_static_initializer_eligibility(
        &self,
        key: &BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticBag, FactQueryError> {
        let checked = self.execution_candidates_with_cancellation(key.clone(), cancellation)?;
        let mut diagnostics = checked.result().diagnostics().clone();

        if diagnostics.has_errors() {
            return Ok(diagnostics);
        }

        let pure = &checked.result().value()
            [&ExecutionObligation::Property(ExecutionProperty::Pure, None)];

        let mut failure = pure.constant_failure();
        let semantics = self.expression_semantics_with_cancellation(key.clone(), cancellation)?;

        let sites = pure
            .invocations()
            .chain(
                pure.dependencies()
                    .iter()
                    .map(|dependency| dependency.occurrence),
            )
            .collect::<std::collections::BTreeSet<_>>();

        let mut targets = std::collections::BTreeSet::new();

        for site in sites {
            for target in bray_checker::constant_invocation_targets(
                semantics.result().value().selections(),
                site,
            ) {
                if !targets.insert(target) {
                    continue;
                }

                let permitted = match target {
                    bray_checker::ExecutionDependencyTarget::Default(provider) => {
                        let template = self.checked_default_template(provider, cancellation)?;

                        diagnostics.add_range(template.diagnostics().iter().cloned());

                        template.value().as_ref().is_some_and(|template| {
                            template.behavior().execution().constant_eligible()
                        })
                    }
                    _ => match target
                        .callable()
                        .expect("non-default invocation retains its callable")
                    {
                        bray_bound_tree::BoundCallableTarget::Declaration(callable) => {
                            self.declares_constant_callable(callable.definition(), cancellation)?
                        }
                        bray_bound_tree::BoundCallableTarget::Anonymous(_)
                        | bray_bound_tree::BoundCallableTarget::Indirect(_) => {
                            matches!(site.expression().and_then(|expression|
                                semantics.result().value().selections().expression(expression)),
                                Some(bray_bound_tree::SemanticSelection::Call(call))
                                    if call.constness() == bray_symbols::CallableConstness::Constant)
                        }
                        bray_bound_tree::BoundCallableTarget::Predicate(_) => true,
                    },
                };

                if !permitted {
                    let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;

                    let span = site
                        .expression()
                        .and_then(|expression| bound.result().value().view().expression(expression))
                        .map(|expression| expression.origin().source_anchor().syntax())
                        .unwrap_or(key.source().syntax());

                    failure.get_or_insert(SourceSpan::new(span.source_id(), span.full_range()));
                }
            }
        }

        for obligation in [
            ExecutionObligation::Observation,
            ExecutionObligation::Termination,
        ] {
            let (rejected, _, _) = self.certify_execution_obligation(
                key,
                obligation,
                &mut diagnostics,
                false,
                cancellation,
            )?;

            if let Some((source, _)) = rejected {
                failure.get_or_insert(source);
            }
        }

        if let Some(failure) = failure {
            let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;

            diagnostics.add(bray_checker::constant_eligibility_diagnostic(
                bray_diagnostics::DiagnosticId::new(0),
                bound.result().value(),
                failure,
            ));
        }

        Ok(diagnostics)
    }

    pub(in crate::compilation) fn check_predicate_eligibility(
        &self,
        key: &BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticBag, FactQueryError> {
        if key.kind() != bray_bound_tree::BoundUnitKind::RuntimeDefault {
            let checked = self.certified_execution_with_cancellation(key.clone(), cancellation)?;

            return Ok(checked.result().diagnostics().clone());
        }

        let mut diagnostics = DiagnosticBag::new();
        let anchor = key.source().syntax();
        let source = SourceSpan::new(anchor.source_id(), anchor.full_range());

        for property in [ExecutionProperty::Pure, ExecutionProperty::Total] {
            let obligation = ExecutionObligation::Property(property, None);

            let (failure, _, _) = self.certify_execution_obligation(
                key,
                obligation,
                &mut diagnostics,
                true,
                cancellation,
            )?;

            if let Some((cause, circular)) = failure {
                diagnostics.add(super::proof::guarantee_diagnostic(
                    obligation, source, cause, circular,
                ));
            }
        }

        Ok(diagnostics)
    }
}

#[cfg(test)]
mod tests {
    use crate::test_support::compilation;
    use bray_diagnostics::DiagnosticKind;

    #[test]
    fn named_constants_bound_arithmetic_without_trusted_obligations() {
        for (guard, valid) in [
            ("index != MAXIMUM", true),
            ("index <= MAXIMUM - 1", true),
            ("true", false),
        ] {
            let source = format!(
                r"
                module app;

                const MAXIMUM: usize = ~0;

                const func positive(pos value: usize) -> bool
                    executes(pure, total)
                {{
                    return value > 0;
                }}

                func observe(pos index: usize)
                    requires({guard} && positive(index + 1))
                {{
                }}

            "
            );

            let checked = compilation(&source);

            assert_eq!(
                !checked.check_diagnostics().has_errors(),
                valid,
                "{source}: {:#?}",
                checked.check_diagnostics()
            );

            if !valid {
                bray_testing::assert_goal_state_diagnostic_kind(
                    checked.check_diagnostics(),
                    DiagnosticKind::CheckingExecutionGuaranteeNotProven,
                );
            }
        }
    }

    #[test]
    fn checked_unsigned_division_guards_preserve_exact_products() {
        for (limit, valid) in [("MAXIMUM / factor", true), ("MAXIMUM", false)] {
            for factor in ["left", "selected(left)"] {
                let source = format!(
                    r"
                    module app;

                    const MAXIMUM: u64 = 18_446_744_073_709_551_615;

                    const func selected(pos value: u64) -> u64
                        executes(pure, total)
                    {{
                        return value;
                    }}

                    func multiply(pos left: u64, pos right: u64) -> (bool, u64)
                        ensures(!result.0 || result.1 == {factor} * right)
                    {{
                        let factor: u64 = {factor};

                        if factor != 0 && right > {limit}
                        {{
                            return (false, 0);
                        }}

                        return (true, factor * right);
                    }}
                "
                );

                let checked = compilation(&source);

                assert_eq!(
                    !checked.check_diagnostics().has_errors(),
                    valid,
                    "{source}: {:#?}",
                    checked.check_diagnostics()
                );

                if !valid {
                    bray_testing::assert_goal_state_diagnostic_kind(
                        checked.check_diagnostics(),
                        DiagnosticKind::CheckingExecutionGuaranteeNotProven,
                    );
                }
            }
        }
    }

    #[test]
    fn completed_short_circuit_conditions_do_not_reinspect_mutated_inputs() {
        let checked = compilation(
            r"
            module app;

            const MAXIMUM: u64 = ~0;

            func multiply(pos mut left: u64, pos right: u64) -> (bool, u64)
                ensures(!result.0 || result.1 == left * right)
            {
                if left != 0 && ({
                    let rejected = right > MAXIMUM / left;
                    left = MAXIMUM;

                    yield rejected;
                })
                {
                    return (false, 0);
                }

                return (true, left * right);
            }
        ",
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            checked.check_diagnostics(),
            DiagnosticKind::CheckingExecutionGuaranteeNotProven,
        );
    }

    #[test]
    fn arithmetic_postconditions_require_bounds_for_every_input() {
        for (body, valid) in [("(kind - 1) as usize", true), ("kind as usize", false)] {
            let source = format!(
                r#"
                module app;

                func selected(pos kind: u32) -> usize
                    requires(kind >= 1 && kind <= 4)
                    ensures(result < 4)
                {{
                    return {body};
                }}
            "#
            );

            let checked = compilation(&source);

            assert_eq!(
                !checked.check_diagnostics().has_errors(),
                valid,
                "{source}: {:#?}",
                checked.check_diagnostics()
            );
        }
    }

    #[test]
    fn callable_headers_keep_their_own_clause_context() {
        for declaration in [
            r#"
                callable Checked = func(pos value: usize) -> bool
                    requires(value > 0)
                    ensures(positive(value) || true);
            "#,
            r#"
                func caller()
                {
                    let chosen = lambda(pos value: usize) -> bool
                        requires(value > 0)
                        ensures(positive(value) || true)
                    {
                        return true;
                    };
                }
            "#,
        ] {
            let source = format!(
                r#"
                module app;

                const func positive(pos value: usize) -> bool
                    requires(value > 0)
                    ensures(result)
                {{
                    return true;
                }}

                {declaration}
            "#
            );

            let checked = compilation(&source);

            assert!(
                !checked.check_diagnostics().has_errors(),
                "{source}: {:#?}",
                checked.check_diagnostics()
            );
        }
    }

    #[test]
    fn clause_context_preserves_only_unchanged_value_inputs() {
        for (parameter, argument, body, valid) in [
            ("value: usize", "value", "", true),
            ("mut value: usize", "value", "value = 0;", false),
            (
                "value: &mut Record",
                "value.number",
                "value.number = 0;",
                false,
            ),
        ] {
            let source = format!(
                r#"
                module app;

                struct Record
                {{
                    mut number: usize;
                }}

                const func positive(pos value: usize) -> bool
                    requires(value > 0)
                    ensures(result)
                {{
                    return true;
                }}

                func selected(pos {parameter})
                    requires({argument} > 0)
                    ensures(positive({argument}) || true)
                {{
                    {body}
                }}
            "#
            );

            let checked = compilation(&source);

            assert_eq!(
                !checked.check_diagnostics().has_errors(),
                valid,
                "{source}: {:#?}",
                checked.check_diagnostics()
            );

            if !valid {
                bray_testing::assert_goal_state_diagnostic_kind(
                    checked.check_diagnostics(),
                    DiagnosticKind::CheckingExecutionGuaranteeNotProven,
                );
            }
        }
    }

    #[test]
    fn later_requirements_use_only_successful_earlier_conditions() {
        for (guard, valid) in [("value > 0", true), ("value >= 0", false)] {
            let source = format!(
                r#"
                module app;

                const func positive(pos value: usize) -> bool
                    requires(value > 0)
                    ensures(result)
                {{
                    return true;
                }}

                func selected(pos value: usize)
                    requires({guard}, positive(value))
                {{
                }}
            "#
            );

            let checked = compilation(&source);

            assert_eq!(
                !checked.check_diagnostics().has_errors(),
                valid,
                "{source}: {:#?}",
                checked.check_diagnostics()
            );
        }
    }

    #[test]
    fn predicate_calls_require_explicit_const_without_annotating_runtime_callers() {
        for (modifier, promises, valid) in [
            ("const ", "", true),
            ("const ", "executes(pure, total)", true),
            ("", "", false),
            ("", "executes(pure, total)", false),
        ] {
            let source = format!(
                r#"
                module app;

                {modifier}func observe(pos value: u8) -> bool
                    {promises}
                {{
                    return value > 0;
                }}

                predicate positive(value: u8) = observe(value);

                func demand(pos value: u8) requires(positive(value))
                {{
                }}

                func ordinary(pos value: u8) -> bool
                {{
                    return observe(value);
                }}
            "#
            );

            let checked = compilation(&source);

            assert_eq!(
                !checked.check_diagnostics().has_errors(),
                valid,
                "{source}: {:#?}",
                checked.check_diagnostics()
            );

            if !valid {
                bray_testing::assert_goal_state_diagnostic_kind(
                    checked.check_diagnostics(),
                    DiagnosticKind::CheckingExecutionGuaranteeNotProven,
                );
            }
        }
    }

    #[test]
    fn predicate_conditionals_admit_their_yields_but_forbid_standalone_blocks() {
        for (expression, valid) in [
            ("if ready { yield true; } else { yield false; }", true),
            ("{ yield ready; }", false),
            ("{ let observed = ready; yield observed; }", false),
        ] {
            let source = format!(
                r#"
                module app;

                predicate selected(ready: bool) = {expression};

                func demand(pos ready: bool) requires(selected(ready))
                {{
                }}
            "#
            );

            let checked = compilation(&source);

            assert_eq!(
                !checked.check_diagnostics().has_errors(),
                valid,
                "{source}: {:#?}",
                checked.check_diagnostics()
            );
        }
    }

    #[test]
    fn const_modifiers_do_not_prove_external_mutation_recursion_or_unbounded_execution() {
        for (parameters, body) in [
            (
                "pos state: &mut State",
                "state.value = true; return state.value;",
            ),
            ("", "return selected();"),
            ("", "loop {}"),
        ] {
            let source = format!(
                r#"
                module app;

                struct State
                {{
                    mut value: bool;
                }}

                const func selected({parameters}) -> bool
                {{
                    {body}
                }}
            "#
            );

            let checked = compilation(&source);

            assert!(
                checked
                    .check_diagnostics()
                    .iter()
                    .any(|diagnostic| matches!(
                        diagnostic.kind(),
                        DiagnosticKind::CheckingExecutionGuaranteeNotProven
                            | DiagnosticKind::CheckingCircularExecutionGuarantee
                    )),
                "{source}: {:#?}",
                checked.check_diagnostics()
            );
        }
    }

    #[test]
    fn predicate_calls_use_actual_short_circuit_entry_evidence() {
        for (condition, valid) in [
            ("value > 0 && nonzero(value) > 0", true),
            ("value == 0 || nonzero(value) > 0", true),
            ("nonzero(value) > 0", false),
            ("value == 0 && nonzero(value) > 0", false),
        ] {
            let source = format!(
                r#"
                module app;

                const func nonzero(pos value: u8) -> u8
                    requires(value > 0)
                {{
                    return value;
                }}

                predicate selected(value: u8) = {condition};

                func demand(pos value: u8) requires(selected(value))
                {{
                }}
            "#
            );

            let checked = compilation(&source);

            assert_eq!(
                !checked.check_diagnostics().has_errors(),
                valid,
                "{source}: {:#?}",
                checked.check_diagnostics()
            );
        }
    }

    #[test]
    fn const_templates_exclude_paths_rejected_by_the_owning_entry_domain() {
        let checked = compilation(
            r#"
            module app;

            const func nonzero(pos value: u8) -> u8
                requires(value > 0)
            {
                if value == 0
                {
                    panic("excluded");
                }

                return value;
            }

            const answer: u8 = nonzero(3);
        "#,
        );

        assert!(
            !checked.check_diagnostics().has_errors(),
            "{:#?}",
            checked.check_diagnostics()
        );
    }

    #[test]
    fn compound_execution_requirements_use_ordinary_caller_evidence() {
        for requirement in [
            "ready || blocking_execution()",
            "if ready { yield true; } else { yield blocking_execution(); }",
        ] {
            for (body, valid) in [
                ("if ready { selected(ready); }", true),
                ("selected(true);", true),
                ("selected(ready);", false),
                ("trusted selected(ready);", false),
            ] {
                let source = format!(
                    r#"
                module app;

                func selected(pos ready: bool)
                    requires({requirement}) {{}}

                async func caller(pos ready: bool)
                {{
                    {body}
                }}
            "#
                );

                let checked = compilation(&source);

                assert_eq!(
                    !checked.check_diagnostics().has_errors(),
                    valid,
                    "{source}: {:#?}",
                    checked.check_diagnostics()
                );

                if !valid {
                    bray_testing::assert_goal_state_diagnostic_kind(
                        checked.check_diagnostics(),
                        DiagnosticKind::CheckingExecutionRequirementNotProven,
                    );
                }
            }
        }
    }

    #[test]
    fn constant_initializers_establish_actual_callee_requirements_before_evaluation() {
        for (argument, valid) in [("3", true), ("0", false)] {
            let source = format!(
                r#"
                module app;

                const func nonzero(pos value: u8) -> u8
                    requires(value > 0)
                {{
                    return value;
                }}

                const answer: u8 = nonzero({argument});
            "#
            );

            let checked = compilation(&source);

            assert_eq!(
                !checked.check_diagnostics().has_errors(),
                valid,
                "{source}: {:#?}",
                checked.check_diagnostics()
            );
        }
    }

    #[test]
    fn known_anonymous_and_aliased_const_implementations_preserve_selected_identity() {
        for body in [
            "let selected: const func(pos value: u8) -> bool = observe; return selected(value);",
            "let selected = const lambda(pos item: u8) -> bool { return item > 0; }; return selected(value);",
        ] {
            let source = format!(
                r#"
                module app;

                const func observe(pos value: u8) -> bool
                {{
                    return value > 0;
                }}

                const func wrapper(pos value: u8) -> bool
                {{
                    {body}
                }}

                predicate accepted(value: u8) = wrapper(value);

                func demand(pos value: u8) requires(accepted(value))
                {{
                }}
            "#
            );

            let checked = compilation(&source);

            assert!(
                !checked.check_diagnostics().has_errors(),
                "{source}: {:#?}",
                checked.check_diagnostics()
            );
        }
    }

    #[test]
    fn constant_construction_uses_cleanup_admission_without_invoking_the_destructor() {
        let checked = compilation(
            r#"
            module app;

            struct Owner
            {
                value: bool;
                destruct() executes(total)
                {
                }
            }

            const func make(pos value: bool) -> Owner
            {
                return { value = value };
            }

            func caller()
            {
                let owner = make(true);
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
    fn predicate_inputs_observe_owners_without_acquiring_cleanup() {
        let checked = compilation(
            r#"
            module app;

            struct Owner
            {
                value: bool;
                destruct()
                {
                    panic("owner cleanup");
                }
            }

            predicate observed(owner: Owner) = owner.value;

            func consume_owner(pos owner: Owner)
                requires(observed(owner))
            {}
        "#,
        );

        assert!(
            !checked.check_diagnostics().has_errors(),
            "{:#?}",
            checked.check_diagnostics()
        );
    }

    #[test]
    fn callable_type_clauses_establish_the_same_helper_eligibility_as_named_clauses() {
        for (modifier, valid) in [("const ", true), ("", false)] {
            let source = format!(
                r#"
                module app;

                {modifier}func observe(pos value: u8) -> bool
                {{
                    return value > 0;
                }}

                callable Action = func(pos value: u8) requires(observe(value));

                func accept(pos action: Action)
                {{
                }}
            "#
            );

            let checked = compilation(&source);

            assert_eq!(
                !checked.check_diagnostics().has_errors(),
                valid,
                "{source}: {:#?}",
                checked.check_diagnostics()
            );
        }
    }

    #[test]
    fn constant_requirements_execute_their_complete_checked_expression() {
        for (argument, valid) in [("3", true), ("0", false)] {
            let source = format!(
                r#"
                module app;

                struct Flag
                {{
                    valid: bool;
                }}

                const func selected(pos value: u8) -> u8
                    requires((Flag {{ valid = value > 0 }}).valid)
                {{
                    return value;
                }}

                const answer: u8 = selected({argument});
            "#
            );

            let checked = compilation(&source);

            assert_eq!(
                !checked.check_diagnostics().has_errors(),
                valid,
                "{source}: {:#?}",
                checked.check_diagnostics()
            );

            if !valid {
                bray_testing::assert_goal_state_diagnostic_kind(
                    checked.check_diagnostics(),
                    DiagnosticKind::CheckingConstantCallRequirementNotProven,
                );
            }
        }
    }

    #[test]
    fn fixed_array_and_slice_bounds_are_proven_from_their_checked_extents() {
        for (projection, valid) in [
            ("value[0]", true),
            ("value[1]", true),
            ("value[..][1]", true),
            ("value[1..2][0]", true),
            ("value[2]", false),
            ("value[1..3][0]", false),
            ("value[1..2][1]", false),
        ] {
            let source = format!(
                r#"
                module app;

                predicate selected(value: &[u8; 2]) = {projection} == 0;
            "#
            );

            let checked = compilation(&source);

            assert_eq!(
                !checked.check_diagnostics().has_errors(),
                valid,
                "{source}: {:#?}",
                checked.check_diagnostics()
            );
        }
    }

    #[test]
    fn slice_bounds_keep_their_captured_values_across_later_argument_evaluation() {
        let checked = compilation(
            r#"
            module app;

            func selected(pos flag: bool) executes(total)
            {
                let values: [u8; 1] = [0];
                let mut start: usize = 1;
                let result = values[start..1][if flag
                {
                    start = 0;
                    yield 0;
                }
                else
                {
                    start = 0;
                    yield 0;
                }];
            }
        "#,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            checked.check_diagnostics(),
            DiagnosticKind::CheckingExecutionGuaranteeNotProven,
        );

        let checked = compilation(
            r#"
            module app;

            const func zero() -> usize
                ensures(result == 0)
            {
                return 0;
            }

            const func selected(pos values: &[u8; 2], pos start: usize, pos end: usize) -> u8
                requires(start < end, end <= 2)
            {
                return values[start..end][zero()];
            }
        "#,
        );

        assert!(
            checked.check_diagnostics().is_empty(),
            "{:#?}",
            checked.check_diagnostics()
        );
    }

    #[test]
    fn joined_index_bounds_require_each_predecessors_actual_value() {
        for (assigned, valid) in [("start", true), ("2", false)] {
            let source = format!(
                r#"
                module app;

                func selected(pos values: &[u8; 2], pos flag: bool, pos start: usize) -> u8
                    requires(start < 2)
                    executes(total)
                {{
                    let mut index: usize = 0;
                    if flag {{ index = {assigned}; }}
                    return values[index];
                }}
            "#
            );

            let checked = compilation(&source);

            assert_eq!(
                !checked.check_diagnostics().has_errors(),
                valid,
                "{source}: {:#?}",
                checked.check_diagnostics()
            );
        }
    }

    #[test]
    fn conditional_completion_preserves_entry_evidence_for_implicit_unit() {
        for (error_body, valid) in [("panic(\"failed\");", true), ("", false)] {
            let source = r#"
            module app;

            func require_ok(pos value: Result<unit, bool>)
                when(value matches Error(_))
                {
                    ensures(false)
                }
            {
                match consume value
                {
                    case Ok(_) {}
                    case Error(_) { ERROR_BODY }
                }
            }
        "#
            .replace("ERROR_BODY", error_body);

            let checked = compilation(&source);

            assert_eq!(
                !checked.check_diagnostics().has_errors(),
                valid,
                "{source}: {:#?}",
                checked.check_diagnostics()
            );
        }
    }

    #[test]
    fn predicate_folds_require_finite_progress_instead_of_merely_total_next() {
        for (step, valid) in [
            ("self.remaining -= 1; return true;", true),
            ("return true;", false),
            ("self.remaining = 3; return true;", false),
            ("self.remaining += 1; return true;", false),
        ] {
            let source = crate::test_support::finite_iterator_source(step);
            let checked = compilation(&source);

            assert_eq!(
                !checked.check_diagnostics().has_errors(),
                valid,
                "{step}: {:#?}",
                checked.check_diagnostics()
            );

            if !valid {
                bray_testing::assert_goal_state_diagnostic_kind(
                    checked.check_diagnostics(),
                    DiagnosticKind::CheckingExecutionGuaranteeNotProven,
                );
            }
        }
    }

    #[test]
    fn proof_depth_does_not_limit_independent_callees() {
        let count = bray_checker::ConstantEvaluationLimits::default().call_depth() + 1;
        let mut source = String::from("module app;");
        let mut calls = String::new();

        for index in 0..count {
            source.push_str(&format!(
                r#"
                func sample_{index}() -> u32
                    executes(pure, total)
                {{
                    return {index};
                }}
            "#
            ));

            calls.push_str(&format!(
                r#"
                    let _: u32 = sample_{index}();
            "#
            ));
        }

        source.push_str(&format!(
            r#"
            func selected()
                executes(pure, total)
            {{
                {calls}
            }}
        "#
        ));

        let checked = compilation(&source);

        assert!(
            !checked.check_diagnostics().has_errors(),
            "{:#?}",
            checked.check_diagnostics()
        );
    }

    #[test]
    fn production_clauses_resolve_their_owners_through_shared_symbol_identity() {
        let checked = crate::test_support::production_standard_library_compilation(
            crate::SelectedTarget::default(),
            "module app;",
        );

        let body = crate::test_support::source_function_body_key(&checked, "scalar_slice_data");
        let declaration = checked.execution_unit_declaration(&body).unwrap();

        for anchor in declaration
            .value()
            .domains()
            .iter()
            .flat_map(|domain| &domain.postconditions)
        {
            let key = bray_bound_tree::BoundUnitKey::contract_clause(
                body.declared_owner().clone(),
                checked.bound_source(*anchor).unwrap(),
            )
            .expect("production postcondition has a clause key");

            let diagnostics = checked
                .check_predicate_eligibility(&key, &checked.state.cancellation)
                .unwrap();

            assert!(!diagnostics.has_errors(), "{diagnostics:#?}");
        }
    }

    #[test]
    fn completed_borrowed_checks_certify_the_entry_without_preserving_mutated_contents() {
        for (execution, postcondition, mutation, valid) in [
            ("", "true", "", true),
            ("executes(total)", "true", "", false),
            ("", "record.limit >= count", "record.limit = 0;", false),
        ] {
            let source = format!(
                r#"
                module app;

                struct Record
                {{
                    mut limit: usize;
                }}

                func selected(pos mut record: Record, pos count: usize)
                    requires(count > 0)
                    ensures({postcondition})
                    {execution}
                {{
                    let check = lambda(pos record: &Record, pos count: usize)
                        requires(count <= record.limit)
                        executes(pure, total)
                    {{
                    }};

                    check(&record, count);
                    {mutation}
                }}
            "#
            );

            let checked = compilation(&source);

            assert_eq!(
                !checked.check_diagnostics().has_errors(),
                valid,
                "{source}: {:#?}",
                checked.check_diagnostics()
            );
        }
    }
}
