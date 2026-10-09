use bray_bound_tree::BoundUnitKey;
use bray_checker::ExecutionCondition;
use bray_declarations::SyntaxAnchor;
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_source::SourceSpan;

use crate::compilation::{
    Compilation, SemanticDataKind, SemanticQueryContext, SemanticQueryFailure,
    SemanticQueryViolation,
};
use crate::fact::{CancellationToken, FactQueryError};

impl Compilation {
    pub(in crate::compilation) fn execution_constant_inputs(
        &self,
        bound: &bray_bound_tree::BoundUnit,
        semantics: &bray_bound_tree::CheckedExpressionSemantics,
        cancellation: &CancellationToken,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<
        std::collections::BTreeMap<bray_checker::ExecutionPlace, ExecutionCondition>,
        FactQueryError,
    > {
        let mut constants = std::collections::BTreeMap::new();
        let mut visited = std::collections::BTreeSet::new();

        for (expression, node) in bound.tree().expressions() {
            let symbol = match node {
                bray_bound_tree::BoundExpression::Name(name) => match name.target() {
                    bray_bound_tree::BoundReferenceTarget::Surface(symbol) => symbol,
                    _ => continue,
                },
                bray_bound_tree::BoundExpression::MemberAccess(_) => {
                    let Some(bray_bound_tree::SemanticSelection::Operation(
                        bray_bound_tree::SelectedOperation::Member(member),
                    )) = semantics.selections().expression(expression)
                    else {
                        continue;
                    };

                    member.member()
                }
                _ => continue,
            };

            if !visited.insert(symbol) {
                continue;
            }

            let Some((_, resolution)) =
                self.resolve_surface_constant(symbol, cancellation, diagnostics)?
            else {
                continue;
            };

            let value = match resolution {
                bray_checker::ConstantReferenceResolution::Value(value) => value,
                bray_checker::ConstantReferenceResolution::Evaluated(value) => value.value(),
                _ => continue,
            };

            let value = self.semantic_value_store()?.constant_value_data(value);

            let condition = match value.kind() {
                bray_symbols::ConstantValueKind::Boolean(value) => {
                    ExecutionCondition::Boolean(*value)
                }
                _ => ExecutionCondition::Literal(value),
            };

            constants.insert(
                bray_bound_tree::BoundReferenceTarget::Surface(symbol).into(),
                condition,
            );
        }

        Ok(constants)
    }

    pub(super) fn execution_condition_inputs(
        &self,
        owner: &BoundUnitKey,
        anchors: &[SyntaxAnchor],
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<Vec<(ExecutionCondition, SourceSpan)>>, FactQueryError> {
        let (conditions, diagnostics) = self
            .predicate_condition_inputs(owner, anchors, cancellation)?
            .into_parts();

        Ok(DiagnosticResult::new(
            conditions
                .into_iter()
                .map(|(condition, _, source)| (condition, source))
                .collect(),
            diagnostics,
        ))
    }

    fn instance_execution_condition_inputs(
        &self,
        owner: &BoundUnitKey,
        callable: bray_symbols::CallableInstanceData,
        anchors: &[SyntaxAnchor],
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<Vec<(ExecutionCondition, SourceSpan)>>, FactQueryError> {
        let (conditions, diagnostics) = self
            .execution_condition_inputs(owner, anchors, cancellation)?
            .into_parts();

        let values = self.semantic_value_store()?;

        let conditions = conditions
            .into_iter()
            .map(|(condition, source)| {
                let condition = bray_checker::map_execution_condition_substitutions(
                    &condition,
                    &|substitution| {
                        values
                            .substitute_generic_substitution(substitution, callable.substitution())
                    },
                )?;

                Ok((condition, source))
            })
            .collect::<Result<_, FactQueryError>>()?;

        Ok(DiagnosticResult::new(conditions, diagnostics))
    }

    pub(in crate::compilation) fn predicate_condition_inputs(
        &self,
        owner: &BoundUnitKey,
        anchors: &[SyntaxAnchor],
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<Vec<(ExecutionCondition, bool, SourceSpan)>>, FactQueryError> {
        let mut conditions = Vec::new();
        let mut diagnostics = DiagnosticBag::new();

        for anchor in anchors {
            let source = self.bound_source(*anchor)?;

            // A clause query owns the shared declaration identity independently of its caller.
            let key = BoundUnitKey::contract_clause(owner.declared_owner().clone(), source)
                .ok_or_else(|| {
                    SemanticQueryFailure::contract(
                        SemanticQueryContext::Source(anchor.source_id()),
                        SemanticQueryViolation::Missing(SemanticDataKind::Syntax),
                    )
                })?;

            let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;

            let semantics =
                self.expression_semantics_with_cancellation(key.clone(), cancellation)?;

            let storage = self.storage_plan_with_cancellation(key.clone(), cancellation)?;
            let body = self.body_semantics_with_cancellation(key, cancellation)?;

            let clause_diagnostics = DiagnosticBag::merged_all([
                bound.result().diagnostics(),
                semantics.result().diagnostics(),
                storage.result().diagnostics(),
                body.result().diagnostics(),
            ]);

            let span = SourceSpan::new(anchor.source_id(), anchor.full_range());

            let constants = self.execution_constant_inputs(
                bound.result().value(),
                semantics.result().value(),
                cancellation,
                &mut diagnostics,
            )?;

            let normalized = bray_checker::predicate_conditions_with_inputs(
                bound.result().value(),
                semantics.result().value(),
                self.semantic_value_store()?,
                &constants,
            )?;

            if clause_diagnostics.has_errors() {
                conditions.push((ExecutionCondition::Unknown, true, span));
            } else {
                conditions.extend(
                    normalized
                        .into_iter()
                        .map(|(_, condition, trusted)| (condition, trusted, span)),
                );
            }

            diagnostics.add_range(clause_diagnostics);
        }

        Ok(DiagnosticResult::new(conditions, diagnostics))
    }
}

#[cfg(test)]
mod tests {
    use crate::test_support::compilation;
    use bray_diagnostics::DiagnosticKind;

    #[test]
    fn projection_preserves_completion_until_mutation() {
        for (body, valid) in [
            (
                "let projected: &mut Flags = &mut value; let _: bool = projected.ready;",
                true,
            ),
            ("observe(&mut value);", true),
            ("mutate(&mut value);", false),
            (
                "let projected = project(&mut value); let _: bool = projected.ready;",
                true,
            ),
            (
                "let projected = project(&mut value); projected.ready = false;",
                false,
            ),
            (
                "let projected: &mut Flags = &mut value; projected.ready = false;",
                false,
            ),
        ] {
            let source = format!(
                r#"
                module app;

                struct Flags {{ mut ready: bool; }}

                func observe(pos value: &mut Flags) executes(pure, total) {{}}

                func mutate(pos value: &mut Flags) {{ value.ready = false; }}

                func project(pos value: &mut Flags) -> &mut Flags executes(pure, total)
                {{
                    return &mut value;
                }}

                func root(pos value: &mut Flags)
                    when(value.ready) {{ ensures(value.ready) }}
                {{
                    {body}
                }}
            "#
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert_eq!(
                !diagnostics.has_errors(),
                valid,
                "{source}: {diagnostics:?}"
            );

            if !valid {
                bray_testing::assert_goal_state_diagnostic_kind(
                    diagnostics,
                    DiagnosticKind::CheckingExecutionGuaranteeNotProven,
                );
            }
        }
    }

    #[test]
    fn whole_value_replacement_installs_the_moved_field_evidence() {
        for (requirement, valid) in [("other.ready", true), ("!other.ready", false)] {
            let source = r#"
                module app;

                struct Flags
                {
                    ready: bool;
                }

                func root(pos mut value: Flags, other: Flags)
                    requires(REQUIREMENT)
                    when(value.ready)
                    {
                        ensures(value.ready)
                    }
                {
                    value = other;
                }
            "#
            .replace("REQUIREMENT", requirement);

            let compilation = compilation(&source);

            assert_eq!(
                compilation.check_diagnostics().is_empty(),
                valid,
                "{source}: {:?}",
                compilation.check_diagnostics()
            );
        }
    }

    #[test]
    fn predicate_guards_compare_selected_identity_without_name_special_cases() {
        for (predicate, valid) in [("complete", true), ("other", false)] {
            let source = r#"
                module app;

                predicate complete(value: bool) = value;

                predicate other(value: bool) = !value;

                func root(pos flag: bool)
                    when(complete(flag))
                    {
                        executes(total)
                        ensures(PREDICATE(flag))
                    } {}
            "#
            .replace("PREDICATE", predicate);

            let compilation = compilation(&source);

            assert_eq!(
                compilation.check_diagnostics().is_empty(),
                valid,
                "{source}: {:?}",
                compilation.check_diagnostics()
            );
        }
    }

    #[test]
    fn error_results_are_normal_exits_and_do_not_invent_completion_values() {
        for (promise, valid) in [("true", true), ("false", false)] {
            let source = r#"
                module app;

                func root() -> Result<unit, unit>
                    when(true)
                    {
                        executes(total)
                        ensures(PROMISE)
                    }
                {
                    return Error(unit);
                }
            "#
            .replace("PROMISE", promise);

            let compilation = compilation(&source);

            assert_eq!(
                compilation.check_diagnostics().is_empty(),
                valid,
                "{source}: {:?}",
                compilation.check_diagnostics()
            );
        }
    }

    #[test]
    fn field_mutation_preserves_only_disjoint_entry_facts() {
        for (field, valid) in [("other", true), ("ready", false)] {
            let source = r#"
                module app;

                struct Flags
                {
                    mut ready: bool;
                    mut other: bool;
                }

                func update(pos mut value: Flags)
                    requires(value.ready)
                    when(true)
                    {
                        ensures(value.ready)
                    }
                {
                    value.FIELD = false;
                }
            "#
            .replace("FIELD", field);

            let compilation = compilation(&source);

            assert_eq!(
                compilation.check_diagnostics().is_empty(),
                valid,
                "{source}: {:?}",
                compilation.check_diagnostics()
            );
        }
    }

    #[test]
    fn numeric_entry_guards_use_checked_values() {
        for (argument, valid) in [("4", true), ("0", false)] {
            let source = r#"
                module app;

                func guarded(pos value: i32)
                    when(value > 0)
                    {
                        executes(total)
                    }
                {
                    if value > 0
                    {
                        return;
                    }

                    panic("outside domain");
                }

                func caller()
                    executes(total)
                {
                    guarded(ARGUMENT);
                }
            "#
            .replace("ARGUMENT", argument);

            let compilation = compilation(&source);

            assert_eq!(
                compilation.check_diagnostics().is_empty(),
                valid,
                "{source}: {:?}",
                compilation.check_diagnostics()
            );
        }
    }

    #[test]
    fn completion_evidence_composes_through_checked_operators() {
        let compilation = compilation(
            r#"
            module app;

            func leaf() -> bool
                when(true)
                {
                    ensures(result)
                }
            {
                return true;
            }

            func caller() -> bool
                when(true)
                {
                    ensures(!result)
                }
            {
                return !leaf();
            }
        "#,
        );

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:?}",
            compilation.check_diagnostics()
        );
    }

    #[test]
    fn completion_evidence_follows_the_returned_value_and_rejects_false_callee_promises() {
        for (returned, valid) in [("true", true), ("false", false)] {
            let source = r#"
                module app;

                func leaf() -> bool
                    when(true)
                    {
                        ensures(result)
                    }
                {
                    return RETURNED;
                }

                func caller() -> bool
                    when(true)
                    {
                        ensures(result)
                    }
                {
                    let first = leaf();
                    let moved = first;

                    return moved;
                }
            "#
            .replace("RETURNED", returned);

            let compilation = compilation(&source);

            assert_eq!(
                compilation.check_diagnostics().is_empty(),
                valid,
                "{source}: {:?}",
                compilation.check_diagnostics()
            );
        }
    }

    #[test]
    fn recursive_completion_claims_cannot_certify_each_other() {
        let compilation = compilation(
            r#"
            module app;

            func first() -> bool
                when(true)
                {
                    ensures(result)
                }
            {
                return second();
            }

            func second() -> bool
                when(true)
                {
                    ensures(result)
                }
            {
                return first();
            }
        "#,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            DiagnosticKind::CheckingCircularExecutionGuarantee,
        );
    }

    #[test]
    fn mutation_invalidates_completion_evidence_at_the_changed_location() {
        let compilation = compilation(
            r#"
            module app;

            func leaf() -> bool
                when(true)
                {
                    ensures(result)
                }
            {
                return true;
            }

            func caller() -> bool
                when(true)
                {
                    ensures(result)
                }
            {
                let mut value = leaf();

                value = false;
                return value;
            }
        "#,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            DiagnosticKind::CheckingExecutionGuaranteeNotProven,
        );
    }

    #[test]
    fn callers_require_proven_callee_entry_guards() {
        for (argument, valid) in [("true", true), ("flag", false), ("false", false)] {
            let source = r#"
                module app;

                func guarded(pos flag: bool)
                    when(flag)
                    {
                        executes(total)
                    }
                {
                    if flag
                    {
                        return;
                    }

                    panic("unguarded input");
                }

                func caller(pos flag: bool)
                    executes(total)
                {
                    guarded(ARGUMENT);
                }
            "#
            .replace("ARGUMENT", argument);

            let compilation = compilation(&source);

            assert_eq!(
                compilation.check_diagnostics().is_empty(),
                valid,
                "{source}: {:?}",
                compilation.check_diagnostics()
            );
        }
    }

    #[test]
    fn flow_joins_keep_common_values_and_discard_conflicting_values() {
        for (alternative, valid) in [("true", true), ("false", false)] {
            let source = r#"
                module app;

                func joined(pos branch: bool) -> bool
                    when(true)
                    {
                        ensures(result)
                    }
                {
                    let mut value: bool = true;

                    if branch
                    {
                        value = true;
                    }
                    else
                    {
                        value = ALTERNATIVE;
                    }

                    return value;
                }
            "#
            .replace("ALTERNATIVE", alternative);

            let compilation = compilation(&source);

            assert_eq!(
                compilation.check_diagnostics().is_empty(),
                valid,
                "{source}: {:?}",
                compilation.check_diagnostics()
            );
        }
    }

    #[test]
    fn nested_and_overlapping_groups_hold_independently_of_order() {
        for clauses in [
            r#"
                when(left)
                {
                    ensures(result)
                    when(right)
                    {
                        executes(pure, total)
                    }
                }
                when(right)
                {
                    ensures(result)
                }
            "#,
            r#"
                when(right)
                {
                    ensures(result)
                }
                when(left)
                {
                    when(right)
                    {
                        executes(total, pure)
                    }
                    ensures(result)
                }
            "#,
        ] {
            let source = r#"
                module app;
                func either(pos left: bool, pos right: bool) -> bool
                    CLAUSES
                {
                    return left || right;
                }
            "#
            .replace("CLAUSES", clauses);

            let compilation = compilation(&source);

            assert!(
                compilation.check_diagnostics().is_empty(),
                "{source}: {:?}",
                compilation.check_diagnostics()
            );
        }
    }

    #[test]
    fn conditional_execution_checks_only_the_promised_entry_domain() {
        let compilation = compilation(
            r#"
            module app;

            func guarded(pos flag: bool) -> bool
                when(flag)
                {
                    executes(pure, total)
                    ensures(result)
                }
            {
                if flag
                {
                    return true;
                }

                panic("outside promised domain");
            }
        "#,
        );

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:?}",
            compilation.check_diagnostics()
        );
    }

    #[test]
    fn guarded_postconditions_observe_exit_values_after_mutating_the_guard_input() {
        for (postcondition, valid) in [("!flag", true), ("flag", false)] {
            let source = r#"
                module app;

                func change(pos mut flag: bool)
                    when(flag)
                    {
                        executes(total)
                        ensures(POSTCONDITION)
                    }
                {
                    flag = false;
                }
            "#
            .replace("POSTCONDITION", postcondition);

            let compilation = compilation(&source);

            assert_eq!(
                compilation.check_diagnostics().is_empty(),
                valid,
                "{source}: {:?}",
                compilation.check_diagnostics()
            );

            if !valid {
                bray_testing::assert_goal_state_diagnostic_kind(
                    compilation.check_diagnostics(),
                    DiagnosticKind::CheckingExecutionGuaranteeNotProven,
                );
            }
        }
    }

    #[test]
    fn guarded_postconditions_without_execution_properties_are_checked() {
        let compilation = compilation(
            r#"
            module app;

            func invalid()
                when(true)
                {
                    ensures(false)
                } {}
        "#,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            DiagnosticKind::CheckingExecutionGuaranteeNotProven,
        );
    }
}

impl Compilation {
    pub(super) fn applicable_execution_obligation(
        &self,
        body: &BoundUnitKey,
        callable: bray_symbols::CallableInstanceData,
        declaration: &bray_checker::ExecutionDeclaration,
        property: bray_checker::ExecutionProperty,
        evidence: Option<&bray_checker::ExecutionCallEvidence>,
        cancellation: &CancellationToken,
    ) -> Result<Option<Option<SourceSpan>>, FactQueryError> {
        let requirements = self.instance_execution_condition_inputs(
            body,
            callable,
            declaration.requirements(),
            cancellation,
        )?;

        if requirements.diagnostics().has_errors() {
            return Ok(None);
        }

        let requirements = requirements
            .into_parts()
            .0
            .into_iter()
            .map(|(condition, _)| condition)
            .collect::<Vec<_>>();

        for domain in declaration.domains() {
            let Some(obligation) = domain
                .properties
                .iter()
                .find(|obligation| obligation.property == property)
            else {
                continue;
            };

            let guards = self.instance_execution_condition_inputs(
                body,
                callable,
                &domain.guards,
                cancellation,
            )?;

            if guards.diagnostics().has_errors() {
                continue;
            }

            // Domains own their conjunction of shared immutable condition terms.
            let mut conditions = requirements.clone();

            conditions.extend(
                guards
                    .into_parts()
                    .0
                    .into_iter()
                    .map(|(condition, _)| condition),
            );

            if evidence
                .unwrap_or(&bray_checker::ExecutionCallEvidence::default())
                .proves(&conditions)
            {
                return Ok(Some(
                    (!domain.guards.is_empty()).then_some(obligation.source),
                ));
            }
        }

        Ok(None)
    }
}

impl Compilation {
    pub(super) fn execution_call_contracts(
        &self,
        bound: &bray_bound_tree::BoundUnit,
        semantics: &bray_bound_tree::CheckedExpressionSemantics,
        cancellation: &CancellationToken,
    ) -> Result<
        DiagnosticResult<
            std::collections::BTreeMap<
                bray_bound_tree::BoundExpressionId,
                Vec<bray_checker::ExecutionCompletionContract>,
            >,
        >,
        FactQueryError,
    > {
        let mut calls = std::collections::BTreeMap::new();
        let mut diagnostics = DiagnosticBag::new();

        for (expression, _) in bound.tree().expressions() {
            let Some(bray_bound_tree::SemanticSelection::Call(call)) =
                semantics.selections().expression(expression)
            else {
                continue;
            };

            let bray_bound_tree::BoundCallableTarget::Declaration(callable) = call.target() else {
                continue;
            };

            let Some(body) = self.callable_body_key(callable.definition())? else {
                if self
                    .symbol_graph()?
                    .declaration_syntax_anchor(callable.definition().symbol())
                    .is_none()
                {
                    let contract =
                        self.imported_execution_contract(callable, &mut diagnostics, cancellation)?;

                    let mut completion = Vec::new();

                    for domain in &*contract.domains {
                        let entry = self.imported_execution_conditions(
                            callable,
                            domain.entry.iter().copied(),
                            cancellation,
                        )?;

                        let mut postconditions = Vec::new();

                        for (ordinal, term) in &*domain.postconditions {
                            let conditions = self.imported_execution_conditions(
                                callable,
                                [*term],
                                cancellation,
                            )?;

                            postconditions.extend(conditions.into_iter().map(|condition| {
                                (
                                    condition,
                                    bray_checker::ExecutionClauseId::Imported(*ordinal),
                                )
                            }));
                        }

                        completion.push(bray_checker::ExecutionCompletionContract {
                            entry,
                            postconditions,
                        });
                    }

                    calls.insert(expression, completion);
                }

                continue;
            };

            let declaration = self.execution_declaration(body.source().syntax())?;
            let mut contracts = Vec::new();

            for domain in declaration
                .value()
                .domains()
                .iter()
                .filter(|domain| !domain.postconditions.is_empty())
            {
                let anchors = declaration
                    .value()
                    .requirements()
                    .iter()
                    .chain(&domain.guards)
                    .copied()
                    .collect::<Vec<_>>();

                let entry = self.instance_execution_condition_inputs(
                    &body,
                    callable,
                    &anchors,
                    cancellation,
                )?;

                let posts = self.instance_execution_condition_inputs(
                    &body,
                    callable,
                    &domain.postconditions,
                    cancellation,
                )?;

                diagnostics.add_range(entry.diagnostics().iter().cloned());
                diagnostics.add_range(posts.diagnostics().iter().cloned());

                contracts.push(bray_checker::ExecutionCompletionContract {
                    entry: entry
                        .into_parts()
                        .0
                        .into_iter()
                        .map(|(condition, _)| condition)
                        .collect(),
                    postconditions: posts
                        .into_parts()
                        .0
                        .into_iter()
                        .map(|(condition, source)| (condition, source.into()))
                        .collect(),
                });
            }

            calls.insert(expression, contracts);
        }

        Ok(DiagnosticResult::new(calls, diagnostics))
    }
}
