use bray_bound_tree::BoundUnitKey;
use bray_checker::{ExecutionObligation, ExecutionProperty};
use bray_diagnostics::DiagnosticBag;
use bray_source::SourceSpan;

use crate::compilation::Compilation;
use crate::fact::{CancellationToken, FactQueryError};

impl Compilation {
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

                func demand(pos value: u8) requires(positive(value)) {{}}

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

                func demand(pos ready: bool) requires(selected(ready)) {{}}
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
    fn const_modifiers_do_not_prove_mutation_recursion_or_unbounded_execution() {
        for body in [
            "let mut changed = false; changed = true; return changed;",
            "return selected();",
            "loop {}",
        ] {
            let source = format!(
                r#"
                module app;

                const func selected() -> bool
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

                func demand(pos value: u8) requires(selected(value)) {{}}
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

                func demand(pos value: u8) requires(accepted(value)) {{}}
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
    fn predicate_inputs_observe_owners_without_acquiring_cleanup() {
        let checked = compilation(
            r#"
            module app;

            struct Owner
            {
                value: bool;
                destruct() { panic("owner cleanup"); }
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

                func accept(pos action: Action) {{}}
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
}
