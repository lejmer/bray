use std::collections::HashSet;

use bray_declarations::{ContainerKind, DeclarationKind, SyntaxAnchor};
use bray_diagnostics::DiagnosticBag;
use bray_syntax::{SyntaxWalkControl, SyntaxWalkEvent, walk_syntax_tree};

use crate::compilation::{Compilation, ProductSourceGraph};

impl Compilation {
    pub(in crate::compilation) fn execution_guarantee_diagnostics(
        &self,
        source_graph: &ProductSourceGraph,
    ) -> Result<DiagnosticBag, crate::fact::FactQueryError> {
        if self.syntax_tree_result().diagnostics().has_errors() {
            return Ok(
                self.unhandled_execution_guarantee_diagnostics(source_graph, &Default::default())
            );
        }

        let cancellation = &self.state.cancellation;
        let mut checked_clauses = std::collections::BTreeSet::new();
        let mut diagnostics = DiagnosticBag::new();

        for root in self.declared_unit_keys()? {
            if !matches!(
                root.kind(),
                bray_bound_tree::BoundUnitKind::CallableBody
                    | bray_bound_tree::BoundUnitKind::AnonymousCallable
            ) {
                continue;
            }

            for bound in self.bound_unit_family_with_cancellation(root, cancellation)? {
                let declaration =
                    self.execution_declaration(bound.value().key().source().syntax())?;

                checked_clauses.extend(declaration.value().clauses().iter().copied());

                if !declaration.value().clauses().is_empty() {
                    // Aggregate diagnostics and retain the immutable key beyond this bound-unit borrow.
                    diagnostics.add_range(
                        self.execution_properties(bound.value().key().clone())?
                            .diagnostics()
                            .clone(),
                    );
                }
            }
        }

        for declaration in source_graph.declarations().declarations() {
            let Some(bray_symbols::AnySymbolId::Function(function)) = self
                .symbol_graph()?
                .symbol_for_declaration(declaration.id())
            else {
                continue;
            };

            let foreign =
                self.foreign_callable_contract_with_cancellation(function, cancellation)?;

            if foreign.value().is_some() && !foreign.diagnostics().has_errors() {
                let declared = self.execution_declaration(declaration.syntax_anchor())?;

                // TODO(BRA-500): Preserve checked guard domains and provenance on opaque foreign assertions.
                if declared
                    .value()
                    .domains()
                    .iter()
                    .any(|domain| !domain.guards.is_empty())
                {
                    continue;
                }

                checked_clauses.extend(declared.value().clauses().iter().copied());
                diagnostics.add_range(declared.into_parts().1);
            }
        }

        // TODO(BRA-500): Preserve certified guarantees in exported interfaces before accepting them.
        if self.state.package_interface_export.is_some() {
            checked_clauses.clear();
        }

        diagnostics.add_range(
            self.unhandled_execution_guarantee_diagnostics(source_graph, &checked_clauses),
        );

        Ok(diagnostics)
    }

    pub(in crate::compilation) fn unhandled_execution_guarantee_diagnostics(
        &self,
        source_graph: &ProductSourceGraph,
        checked_clauses: &std::collections::BTreeSet<SyntaxAnchor>,
    ) -> DiagnosticBag {
        let mut diagnostics = DiagnosticBag::new();
        let declarations = source_graph.declarations();

        let roots: HashSet<_> = declarations
            .declarations()
            .iter()
            .filter(|declaration| declaration.kind() != DeclarationKind::Module)
            .filter(|declaration| {
                declarations
                    .container(declaration.owning_container())
                    .is_some_and(|container| container.kind() == ContainerKind::Module)
            })
            .map(|declaration| declaration.syntax_anchor())
            .collect();

        walk_syntax_tree(self.syntax_tree(), |event| {
            if let SyntaxWalkEvent::EnterNode(node) = event
                && roots.contains(&SyntaxAnchor::from_node(&node))
            {
                diagnostics.add_range(bray_checker::check_execution_guarantees(
                    &node,
                    &checked_clauses,
                ));

                return SyntaxWalkControl::SkipChildren;
            }

            SyntaxWalkControl::Continue
        });

        diagnostics
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
                    when(true) {{ ensures(false) executes(total) }} {{}}
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
    fn execution_guarantees_in_nested_and_bodyless_callables_are_rejected() {
        for declaration in [
            r#"
                func outer(pos flag: bool)
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
            r#"
                callable Action = func()
                    executes(total);
            "#,
            r#"
                trait Resource
                {
                    func requirement()
                        executes(pure);
                }
            "#,
            r#"
                func higher_order(action: func()
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

            assert!(
                diagnostics.iter().any(|diagnostic| {
                    diagnostic.kind() == DiagnosticKind::CheckingExecutionGuaranteeUnsupported
                }),
                "{declaration}: {diagnostics:?}"
            );
        }
    }
}
