use std::collections::HashSet;

use bray_declarations::{ContainerKind, DeclarationKind, SyntaxAnchor};
use bray_diagnostics::DiagnosticBag;
use bray_syntax::{SyntaxWalkControl, SyntaxWalkEvent, walk_syntax_tree};

use super::{Compilation, ProductSourceGraph};

impl Compilation {
    pub(super) fn execution_guarantee_diagnostics(
        &self,
        source_graph: &ProductSourceGraph,
    ) -> DiagnosticBag {
        // TODO: Remove this temporary syntax scan once semantic queries verify execution guarantees.
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

        let mut diagnostics = DiagnosticBag::new();

        walk_syntax_tree(self.syntax_tree(), |event| {
            if let SyntaxWalkEvent::EnterNode(node) = event
                && roots.contains(&SyntaxAnchor::from_node(&node))
            {
                diagnostics.add_range(bray_checker::check_execution_guarantees(&node));

                return SyntaxWalkControl::SkipChildren;
            }

            SyntaxWalkControl::Continue
        });

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{DiagnosticArg, DiagnosticKind};
    use bray_syntax::SyntaxKind;
    use bray_testing::assert_goal_state_diagnostics;

    use crate::WorkerBudget;
    use crate::test_support::{compilation, compilation_with_sources_and_worker_budget};

    #[test]
    fn execution_guarantees_never_gain_trust_from_declaration_modifiers() {
        for modifier in ["", "const ", "trusted ", "async "] {
            for (clause, keyword) in [
                ("executes(pure, total)", SyntaxKind::ExecutesKeyword),
                (
                    "when(true) { ensures(false) executes(total) }",
                    SyntaxKind::WhenKeyword,
                ),
                ("when(false) {}", SyntaxKind::WhenKeyword),
            ] {
                let source = format!("trusted module app; {modifier}func check() {clause} {{}}");

                let compilation = compilation(&source);

                assert!(compilation.syntax_tree_result().diagnostics().is_empty());

                let diagnostics = compilation.check_diagnostics();

                let rejections = diagnostics
                    .iter()
                    .filter(|diagnostic| {
                        diagnostic.kind() == DiagnosticKind::CheckingExecutionGuaranteeUnsupported
                    })
                    .collect::<Vec<_>>();

                assert_eq!(rejections.len(), 1, "{source}: {diagnostics:?}");
                assert_eq!(diagnostics.len(), 1, "{source}: {diagnostics:?}");

                assert_eq!(
                    rejections[0].args(),
                    &[DiagnosticArg::actual_syntax_kind(keyword)]
                );

                assert_goal_state_diagnostics(diagnostics);
            }
        }
    }

    #[test]
    fn execution_guarantees_respect_source_and_product_selection() {
        let sources = [
            "module app; func ordinary() {}",
            "@test module app.tests; func test_only() executes(total) {}",
            "@target(false) module app.disabled; func disabled() when(true) {} {}",
        ];

        let compilation =
            compilation_with_sources_and_worker_budget(&sources, WorkerBudget::serial());

        let diagnostics = compilation.check_diagnostics();

        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn execution_guarantees_in_nested_and_bodyless_callables_are_rejected() {
        for declaration in [
            "callable Action = func() executes(total);",
            "func outer() { let action = lambda() when(true) {} {}; }",
            "trait Resource { func requirement() executes(pure); }",
            "struct Value { func method() executes(total) {} }",
            "struct Value { finalize() when(true) {} {} }",
            "func higher_order(action: func() executes(pure)) {}",
        ] {
            let source = format!("module app; {declaration}");
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
