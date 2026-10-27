use bray_declarations::{DeclarationKind, SyntaxAnchor};
use bray_diagnostics::DiagnosticBag;
use bray_syntax::{SyntaxWalkControl, SyntaxWalkEvent};

use crate::compilation::{Compilation, ProductSourceGraph};

impl Compilation {
    pub(in crate::compilation) fn execution_guarantee_diagnostics(
        &self,
    ) -> Result<&DiagnosticBag, crate::fact::FactQueryError> {
        self.state
            .execution_guarantee_diagnostics
            .get_or_compute_requested(
                &self.state.fact_runtime,
                crate::fact::CompilationFactKey::ExecutionGuaranteeDiagnostics,
                &self.state.cancellation,
                |cancellation| {
                    let source_graph = self.product_source_graph()?;

                    self.compute_execution_guarantee_diagnostics(source_graph, cancellation)
                },
            )
    }

    fn compute_execution_guarantee_diagnostics(
        &self,
        source_graph: &ProductSourceGraph,
        cancellation: &crate::fact::CancellationToken,
    ) -> Result<DiagnosticBag, crate::fact::FactQueryError> {
        if self.syntax_tree_result().diagnostics().has_errors() {
            return Ok(DiagnosticBag::new());
        }

        let mut diagnostics = DiagnosticBag::new();

        for root in self.declared_unit_keys()? {
            if root.is_static_initializer()
                || matches!(
                    root.kind(),
                    bray_bound_tree::BoundUnitKind::PredicateDefinition
                        | bray_bound_tree::BoundUnitKind::ContractClause
                        | bray_bound_tree::BoundUnitKind::Constraint
                )
            {
                let checked =
                    self.certified_execution_with_cancellation(root.clone(), cancellation)?;

                diagnostics.add_range(checked.result().diagnostics().iter().cloned());
                continue;
            }

            if !matches!(
                root.kind(),
                bray_bound_tree::BoundUnitKind::CallableBody
                    | bray_bound_tree::BoundUnitKind::AnonymousCallable
            ) {
                continue;
            }

            let mut pending = vec![root.clone()];

            while let Some(key) = pending.pop() {
                cancellation.check()?;

                let nested = match self.published_source_output(&key, cancellation)? {
                    Some(output) => output
                        .nested
                        .iter()
                        .map(|(_, _, _, key)| key.clone())
                        .collect(),
                    None => self
                        .bound_unit_with_cancellation(key.clone(), cancellation)?
                        .result()
                        .value()
                        .nested_units()
                        .to_vec(),
                };

                pending.extend(nested.into_iter().rev());

                if !matches!(
                    key.kind(),
                    bray_bound_tree::BoundUnitKind::CallableBody
                        | bray_bound_tree::BoundUnitKind::AnonymousCallable
                ) {
                    continue;
                }

                let declaration = self.execution_unit_declaration(&key)?;

                if key.kind() == bray_bound_tree::BoundUnitKind::CallableBody {
                    diagnostics.add_range(self.check_execution_declaration_conditions(
                        &key,
                        declaration.value(),
                        cancellation,
                    )?);
                }

                let declared_const = self.unit_requires_constant_evaluation(&key, cancellation)?;

                if !declaration.value().clauses().is_empty()
                    || declared_const
                    || declaration
                        .value()
                        .domains()
                        .iter()
                        .any(|domain| !domain.postconditions.is_empty())
                {
                    // Aggregate diagnostics and retain the immutable key beyond this bound-unit borrow.
                    diagnostics.add_range(self.execution_properties(key)?.diagnostics().clone());
                }
            }
        }

        for declaration in source_graph.declarations().declarations() {
            if let Some(symbol) = self
                .symbol_graph()?
                .symbol_for_declaration(declaration.id())
                && matches!(
                    self.symbol_graph()?.containing_symbol(symbol),
                    Some(bray_symbols::AnySymbolId::Trait(_))
                )
                && let Some(owner) = self.execution_contract_owner(symbol)?
            {
                let declared = self.execution_declaration(declaration.syntax_anchor())?;

                diagnostics.add_range(declared.diagnostics().iter().cloned());

                diagnostics.add_range(self.check_execution_declaration_conditions(
                    &owner,
                    declared.value(),
                    cancellation,
                )?);
            }

            let Some(bray_symbols::AnySymbolId::Function(function)) = self
                .symbol_graph()?
                .symbol_for_declaration(declaration.id())
            else {
                continue;
            };

            let foreign =
                self.foreign_callable_contract_with_cancellation(function, cancellation)?;

            let declared = self.execution_declaration(declaration.syntax_anchor())?;

            if self.intrinsic_projection_declaration(
                function.into(),
                declared.value(),
                cancellation,
            )? {
                diagnostics.add_range(declared.into_parts().1);
                continue;
            }

            if foreign.value().is_some() && !foreign.diagnostics().has_errors() {
                if let Some(owner) = self.execution_contract_owner(function.into())? {
                    diagnostics.add_range(self.check_execution_declaration_conditions(
                        &owner,
                        declared.value(),
                        cancellation,
                    )?);
                }

                diagnostics.add_range(declared.into_parts().1);
            }
        }

        diagnostics
            .add_range(self.callable_type_guarantee_diagnostics(source_graph, cancellation)?);

        Ok(diagnostics)
    }

    fn check_execution_declaration_conditions(
        &self,
        owner: &bray_bound_tree::BoundUnitKey,
        declared: &bray_checker::ExecutionDeclaration,
        cancellation: &crate::fact::CancellationToken,
    ) -> Result<DiagnosticBag, crate::fact::FactQueryError> {
        let mut diagnostics = DiagnosticBag::new();

        let anchors = declared
            .requirements()
            .iter()
            .copied()
            .chain(
                declared
                    .domains()
                    .iter()
                    .flat_map(|domain| domain.guards.iter().chain(&domain.postconditions).copied()),
            )
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();

        diagnostics.add_range(
            self.execution_condition_inputs(owner, &anchors, cancellation)?
                .into_parts()
                .1,
        );

        for anchor in anchors {
            let key = bray_bound_tree::BoundUnitKey::contract_clause(
                owner.declared_owner().clone(),
                self.bound_source(anchor)?,
            )
            .expect("declaration conditions retain contract clause keys");

            diagnostics.add_range(self.check_predicate_eligibility(&key, cancellation)?);
        }

        Ok(diagnostics)
    }

    fn callable_type_guarantee_diagnostics(
        &self,
        source_graph: &ProductSourceGraph,
        cancellation: &crate::fact::CancellationToken,
    ) -> Result<DiagnosticBag, crate::fact::FactQueryError> {
        let context = self.binding_context(cancellation)?;
        let symbols = self.symbol_graph()?;

        let anchors = source_graph
            .declarations()
            .declarations()
            .iter()
            .map(|declaration| declaration.syntax_anchor())
            .collect::<std::collections::BTreeSet<_>>();

        let mut diagnostics = DiagnosticBag::new();

        for declaration in source_graph
            .declarations()
            .declarations()
            .iter()
            .filter(|declaration| declaration.kind() != DeclarationKind::Module)
        {
            cancellation.check()?;

            let Some(owner) = symbols.symbol_for_declaration(declaration.id()) else {
                continue;
            };

            let owner = if matches!(
                declaration.kind(),
                DeclarationKind::CallableParameter | DeclarationKind::PredicateParameter
            ) {
                symbols
                    .containing_symbol(owner)
                    .expect("signature parameter retains its callable owner")
            } else {
                owner
            };

            let root = declaration.syntax_anchor();
            let node = self.execution_declaration_node(root)?;
            let mut failure = None;

            bray_syntax::walk_syntax_node(&node, |event| {
                let SyntaxWalkEvent::EnterNode(node) = event else {
                    return SyntaxWalkControl::Continue;
                };

                let anchor = SyntaxAnchor::from_node(&node);

                if anchor != root && anchors.contains(&anchor) {
                    return SyntaxWalkControl::SkipChildren;
                }

                if node.kind() == bray_syntax::SyntaxKind::TypeExpression
                    && node
                        .tokens()
                        .any(|token| token.kind() == bray_syntax::SyntaxKind::FuncKeyword)
                {
                    let syntax = node
                        .cast::<bray_syntax::TypeExpressionSyntax>()
                        .expect("callable type has its typed syntax");

                    let result = crate::compilation::binder::type_binder(&context, owner)
                        .and_then(|binder| binder.bind_type_expression(&syntax));

                    match result {
                        Ok(checked) => diagnostics.add_range(checked.into_parts().1),
                        Err(error) => {
                            failure = Some(crate::compilation::binder::binding_query_error(error));

                            return SyntaxWalkControl::Stop;
                        }
                    }
                }

                SyntaxWalkControl::Continue
            });

            if let Some(error) = failure {
                return Err(error);
            }
        }

        Ok(diagnostics)
    }

}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticKind;
    use bray_testing::assert_goal_state_diagnostic_kind;

    use crate::WorkerBudget;
    use crate::test_support::{compilation, compilation_with_sources_and_worker_budget};

    #[test]
    fn conditional_guarantees_never_gain_trust_from_declaration_modifiers() {
        for modifier in ["", "const ", "trusted ", "async "] {
            let source = format!(
                r#"
                trusted module app;

                {modifier}func check()
                    when(true)
                    {{
                        ensures(false)
                        executes(total)
                    }} {{}}
            "#
            );

            let compilation = compilation(&source);

            assert_goal_state_diagnostic_kind(
                compilation.check_diagnostics(),
                DiagnosticKind::CheckingExecutionGuaranteeNotProven,
            );
        }
    }

    #[test]
    fn execution_guarantees_respect_source_and_product_selection() {
        let sources = [
            r#"
                module app;

                func ordinary() {}
            "#,
            r#"
                @test
                module app.tests;

                func test_only()
                    executes(total) {}
            "#,
            r#"
                @target(false)
                module app.disabled;

                func disabled()
                    when(true)
                    {
                    } {}
            "#,
        ];

        let compilation =
            compilation_with_sources_and_worker_budget(&sources, WorkerBudget::serial());

        let diagnostics = compilation.check_diagnostics();

        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn callable_types_check_requirements_and_execution_guarantees() {
        for declaration in [
            r#"
                callable Action = func() requires(true)
                    executes(total);
            "#,
            r#"
                func higher_order(action: func() requires(true)
                        executes(pure)
                    ) {}
            "#,
        ] {
            let source = format!(
                r#"
                    module app;

                    {declaration}
                "#
            );

            let compilation = compilation(&source);

            assert!(compilation.syntax_tree_result().diagnostics().is_empty());

            let diagnostics = compilation.check_diagnostics();

            assert!(!diagnostics.has_errors(), "{declaration}: {diagnostics:?}");
        }
    }

    #[test]
    fn anonymous_bodies_check_their_guarded_execution_guarantees() {
        let checked = compilation(
            r#"
            module app;

            func outer()
            {
                let action = lambda(flag: bool)
                    when(flag)
                    {
                        executes(total)
                    }
                {
                };
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
    fn declaration_guards_can_call_const_functions_without_signature_query_cycles() {
        for declarations in [
            r#"
                const func ready() -> bool when(ready()) { executes(pure, total) }
                { return true; }
                callable Action = func() when(ready()) { executes(pure) };
            "#,
            r#"
                const func first() -> bool when(second()) { executes(pure, total) }
                { return true; }
                const func second() -> bool when(first()) { executes(pure, total) }
                { return true; }
            "#,
            r#"
                struct Ready {
                    const func ready() -> bool when(self.ready()) { executes(pure, total) }
                    { return true; }
                }
            "#,
        ] {
            let source = format!("module app; {declarations}");
            let checked = compilation(&source);

            assert!(
                !checked.check_diagnostics().has_errors(),
                "{source}: {:?}",
                checked.check_diagnostics()
            );
        }

        let checked = compilation(
            r#"
            module app;
            func ready() -> bool when(ready()) { executes(pure, total) }
            { return true; }
        "#,
        );

        assert!(checked.check_diagnostics().has_errors());
    }

    #[test]
    fn callable_entry_guards_do_not_expose_the_completion_result() {
        for contract in [
            "requires(result) executes(pure)",
            "when(result) { executes(pure) }",
            "when(result) { executes(pure) ensures(result) }",
            "when(result) { ensures(result) }",
        ] {
            let source = format!("module app; callable Action = func() -> bool {contract};");

            assert_goal_state_diagnostic_kind(
                compilation(&source).check_diagnostics(),
                DiagnosticKind::BindingUnresolvedName,
            );
        }
    }

    #[test]
    fn callable_guards_use_their_own_parameters_and_completion_result() {
        let checked = compilation(
            r#"
            module app;

            callable Action = func(pos flag: bool) -> bool
                requires(flag)
                when(flag)
                {
                    executes(pure, total)
                    ensures(result)
                };

            struct Holder
            {
                action: func(pos flag: bool) -> bool
                    when(flag) { executes(pure) ensures(result) };
            }

            func outer(pos flag: i32, action: func(pos flag: bool) -> bool
                when(flag) { executes(pure) ensures(result) }) {}
        "#,
        );

        assert!(
            !checked.check_diagnostics().has_errors(),
            "{:?}",
            checked.check_diagnostics()
        );
    }

    #[test]
    fn callable_guards_reject_invalid_names_and_property_names_even_when_unused() {
        for (clause, kind) in [
            (
                "when(missing) { executes(pure) }",
                DiagnosticKind::BindingUnresolvedName,
            ),
            (
                "executes(magic)",
                DiagnosticKind::CheckingUnknownExecutionProperty,
            ),
        ] {
            for declaration in [
                "callable Action = func(pos flag: bool) -> bool CLAUSE;",
                "struct Holder { action: func(pos flag: bool) -> bool CLAUSE; }",
                "func outer(pos flag: i32, action: func(pos input: bool) -> bool CLAUSE) {}",
            ] {
                let source = format!("module app;\n{}", declaration.replace("CLAUSE", clause));
                let checked = compilation(&source);

                assert_goal_state_diagnostic_kind(checked.check_diagnostics(), kind);
            }
        }
    }

    #[test]
    fn callable_and_foreign_guards_reject_non_predicate_expressions() {
        for guard in ["42", "runtime_value()"] {
            let callable = format!(
                r#"
                module app;
                func runtime_value() -> bool {{ return true; }}
                callable Action = func() when({guard}) {{ executes(pure) }};
            "#
            );

            assert!(
                compilation(&callable).check_diagnostics().has_errors(),
                "{callable}"
            );

            let foreign = format!(
                r#"
                trusted module app;
                func runtime_value() -> bool {{ return true; }}
                @link(name = "c") @abi(c) @symbol(name = "native_value")
                extern trusted func native_value() -> bool uses(foreign_call)
                    when({guard}) {{ executes(total) }};
            "#
            );

            assert!(
                foreign_compilation(&foreign)
                    .check_diagnostics()
                    .has_errors(),
                "{foreign}"
            );
        }
    }

    #[test]
    fn callable_const_guard_substitution_stays_conservative() {
        for (generic, argument, entry, valid) in [
            ("", "true", "executes(pure)", true),
            ("", "false", "executes(pure)", false),
            ("<const enabled: bool>", "enabled", "executes(pure)", false),
            (
                "<const enabled: bool>",
                "enabled",
                "when(enabled) { executes(pure) }",
                true,
            ),
        ] {
            let source = format!(
                r#"
                module app;
                callable Action<const enabled: bool> = func() when(enabled) {{ executes(pure) }};
                func root{generic}(pos action: Action<{argument}>) {entry} {{ action(); }}
            "#
            );

            let checked = compilation(&source);

            assert_eq!(
                !checked.check_diagnostics().has_errors(),
                valid,
                "{source}: {:?}",
                checked.check_diagnostics()
            );
        }
    }

    #[test]
    fn indirect_purity_requires_a_proven_callable_guard() {
        for (entry, argument, valid) in [
            ("", "true", true),
            ("", "false", false),
            ("", "flag", false),
            ("requires(flag)", "flag", true),
        ] {
            let source = r#"
                module app;
                callable Action = func(pos flag: bool) -> bool
                    when(flag) { executes(pure) };

                func root(pos action: Action, pos flag: bool) -> bool
                    ENTRY
                    executes(pure)
                {
                    return action(ARGUMENT);
                }
            "#
            .replace("ENTRY", entry)
            .replace("ARGUMENT", argument);

            let checked = compilation(&source);

            assert_eq!(
                !checked.check_diagnostics().has_errors(),
                valid,
                "{source}: {:?}",
                checked.check_diagnostics()
            );

            if !valid {
                assert_goal_state_diagnostic_kind(
                    checked.check_diagnostics(),
                    DiagnosticKind::CheckingExecutionGuaranteeNotProven,
                );
            }
        }
    }

    #[test]
    fn indirect_guarded_postconditions_follow_call_entry() {
        for (argument, valid) in [("true", true), ("false", false), ("flag", false)] {
            let source = format!(
                r#"
                module app;
                callable Action = func(pos flag: bool) -> bool
                    when(flag) {{ executes(pure) ensures(result) }};
                func root(pos flag: bool, pos action: Action) -> bool
                    executes(pure) ensures(result)
                {{ return action({argument}); }}
            "#
            );

            let checked = compilation(&source);

            assert_eq!(
                !checked.check_diagnostics().has_errors(),
                valid,
                "{source}: {:?}",
                checked.check_diagnostics()
            );
        }
    }

    #[test]
    fn guarded_callable_conversions_forget_but_cannot_invent_promises() {
        for (source_contract, target_contract, valid) in [
            ("executes(pure)", "when(flag) { executes(pure) }", true),
            (
                "executes(pure) ensures(result)",
                "when(flag) { executes(pure) ensures(result) }",
                true,
            ),
            (
                "when(flag) { executes(pure) ensures(result) }",
                "executes(pure) ensures(result)",
                false,
            ),
            (
                "when(flag) { executes(pure) ensures(result) }",
                "when(flag) { executes(pure) ensures(result) }",
                true,
            ),
            ("ensures(result && flag)", "ensures(result)", true),
            ("ensures(result)", "ensures(result && flag)", false),
            ("when(flag) { executes(pure) }", "", true),
            (
                "when(flag) { executes(pure) }",
                "when(flag && other) { executes(pure) }",
                true,
            ),
            (
                "when(flag) { executes(pure) }",
                "requires(flag) executes(pure)",
                true,
            ),
            ("when(true) { executes(pure) }", "executes(pure)", true),
            (
                "when(flag && other) { executes(pure) }",
                "when(flag) { executes(pure) }",
                false,
            ),
            ("when(flag) { executes(pure) }", "executes(pure)", false),
            ("", "when(flag) { executes(pure) }", false),
            ("requires(flag) executes(pure)", "executes(pure)", false),
        ] {
            let source = r#"
                module app;
                callable Source = func(pos flag: bool, pos other: bool) -> bool SOURCE;
                callable Target = func(pos flag: bool, pos other: bool) -> bool TARGET;

                func convert(pos action: Source) -> Target
                {
                    return action as Target;
                }
            "#
            .replace("SOURCE", source_contract)
            .replace("TARGET", target_contract);

            let checked = compilation(&source);

            assert_eq!(
                !checked.check_diagnostics().has_errors(),
                valid,
                "{source}: {:?}",
                checked.check_diagnostics()
            );
        }
    }

    #[test]
    fn named_callable_values_preserve_declared_guard_domains() {
        for (guard, valid) in [("flag", true), ("true", true), ("!flag", false)] {
            let source = r#"
                module app;
                callable Action = func(pos flag: bool) -> bool
                    when(flag) { executes(pure) };

                func supplied(pos flag: bool) -> bool
                    when(GUARD) { executes(pure) }
                {
                    return flag;
                }

                func root() -> bool executes(pure)
                {
                    let action: Action = supplied;
                    return action(true);
                }
            "#
            .replace("GUARD", guard);

            let checked = compilation(&source);

            assert_eq!(
                !checked.check_diagnostics().has_errors(),
                valid,
                "{source}: {:?}",
                checked.check_diagnostics()
            );
        }
    }

    fn foreign_compilation(source: &str) -> crate::Compilation {
        let options = crate::CompilationOptions::new(
            crate::WorkerBudget::serial(),
            bray_symbols::ProductKind::Library,
            crate::SelectedTarget::baseline(),
        )
        .with_native_link_inputs([bray_symbols::NativeLinkRequirement::new(
            bray_base::NonEmptySharedStr::try_new("c").expect("fixture native link has a name"),
            bray_symbols::NativeLinkKind::Dynamic,
        )]);

        crate::test_support::compilation_with_options(source, options)
    }

    #[test]
    fn foreign_guards_require_entry_evidence_and_preserve_assertion_provenance() {
        for (argument, valid) in [("true", true), ("false", false), ("flag", false)] {
            let source = r#"
                trusted module app;

                @link(name = "c")
                @symbol(name = "native_value")
                @abi(c)
                extern trusted func native_value(pos flag: bool) -> bool
                    uses(foreign_call)
                    when(flag) { executes(total) ensures(result) };

                trusted func root(pos flag: bool) -> bool
                    uses(foreign_call)
                    executes(total)
                    ensures(result)
                {
                    return native_value(ARGUMENT);
                }
            "#
            .replace("ARGUMENT", argument);

            let checked = foreign_compilation(&source);

            assert_eq!(
                !checked.check_diagnostics().has_errors(),
                valid,
                "{source}: {:?}",
                checked.check_diagnostics()
            );

            if valid {
                let key = crate::test_support::source_function_body_key(&checked, "root");

                let evidence = checked
                    .execution_properties(key)
                    .expect("root execution evidence is available");

                assert_eq!(evidence.value().foreign_assertions.len(), 1);
            } else {
                assert_goal_state_diagnostic_kind(
                    checked.check_diagnostics(),
                    DiagnosticKind::CheckingExecutionGuaranteeNotProven,
                );
            }
        }
    }

    #[test]
    fn unused_foreign_guard_domains_are_checked_in_their_declaration_context() {
        let checked = foreign_compilation(
            r#"
            trusted module app;

            @link(name = "c")
            @symbol(name = "native_value")
            @abi(c)
            extern trusted func native_value(pos flag: bool) -> bool
                uses(foreign_call)
                when(missing) { executes(total) };
        "#,
        );

        assert_goal_state_diagnostic_kind(
            checked.check_diagnostics(),
            DiagnosticKind::BindingUnresolvedName,
        );
    }
}
