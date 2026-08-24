use std::collections::BTreeMap;

use bray_declarations::{
    DeclarationId, DeclarationKind, DeclarationTable, ModulePartId, SyntaxAnchor,
    merge_selected_declaration_chunks,
};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{AnySymbolId, ModuleContributionGate, ProductKind, SymbolGraph, SymbolKey};

use super::Compilation;
use crate::fact::{CompilationFactKey, FactQueryError};

/// Enabled source declarations for one selected package product and target.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ProductSourceGraph {
    product_kind: ProductKind,
    declarations: DeclarationTable,
    contribution_gates: BTreeMap<SyntaxAnchor, ModuleContributionGate>,
    diagnostics: DiagnosticBag,
}

impl ProductSourceGraph {
    fn new(
        product_kind: ProductKind,
        declarations: DeclarationTable,
        contribution_gates: BTreeMap<SyntaxAnchor, ModuleContributionGate>,
        diagnostics: DiagnosticBag,
    ) -> Self {
        Self {
            product_kind,
            declarations,
            contribution_gates,
            diagnostics,
        }
    }

    /// Returns the selected package-product category.
    pub const fn product_kind(&self) -> ProductKind {
        self.product_kind
    }

    /// Returns declarations contributed by enabled module parts.
    pub const fn declarations(&self) -> &DeclarationTable {
        &self.declarations
    }

    /// Returns diagnostics produced while selecting and merging contributions.
    pub const fn diagnostics(&self) -> &DiagnosticBag {
        &self.diagnostics
    }

    pub(super) fn contribution_gate(
        &self,
        syntax: SyntaxAnchor,
    ) -> Option<&ModuleContributionGate> {
        self.contribution_gates.get(&syntax)
    }
}

pub(super) fn source_declaration_module_parts(
    declarations: &DeclarationTable,
) -> BTreeMap<DeclarationId, ModulePartId> {
    declarations
        .module_parts()
        .iter()
        .flat_map(|part| {
            part.declarations()
                .iter()
                .copied()
                .map(move |declaration| (declaration, part.id()))
        })
        .collect()
}

pub(super) fn source_symbol_contribution_gate<'graph>(
    source_graph: &'graph ProductSourceGraph,
    symbols: &SymbolGraph,
    module_parts: &BTreeMap<DeclarationId, ModulePartId>,
    symbol: AnySymbolId,
) -> Option<&'graph ModuleContributionGate> {
    symbols
        .symbol_key(symbol)
        .and_then(SymbolKey::source_declaration_id)
        .and_then(|declaration| module_parts.get(&declaration).copied())
        .and_then(|part| source_graph.declarations().module_part(part))
        .and_then(|part| source_graph.contribution_gate(part.syntax_anchor()))
}

impl Compilation {
    /// Returns enabled source declarations for the selected product and target.
    pub fn product_source_graph(&self) -> Result<&ProductSourceGraph, FactQueryError> {
        self.evaluate_frozen_query(
            CompilationFactKey::ProductSourceGraph,
            &self.state.product_source_graph,
            || self.compute_product_source_graph(),
        )
        .as_ref()
        .map_err(Clone::clone)
    }

    fn compute_product_source_graph(&self) -> Result<ProductSourceGraph, FactQueryError> {
        let mut contribution_gates = BTreeMap::new();
        let mut gate_diagnostics = DiagnosticBag::new();

        for part in self.declaration_table().module_parts() {
            let gate = self.module_contribution_gate(part.id())?;

            gate_diagnostics.add_range(gate.diagnostics().iter().cloned());

            // The graph retains each gate after releasing its Arc-backed query result.
            contribution_gates.insert(part.syntax_anchor(), gate.value().clone());
        }

        let mut chunks = Vec::with_capacity(self.source_count());

        for source in self.sources().iter() {
            let Some(chunk) = self.declaration_chunk(source.source_id()) else {
                return Err(FactQueryError::InfrastructureFailure);
            };

            chunks.push(chunk);
        }

        let selected = merge_selected_declaration_chunks(
            chunks,
            |part| {
                contribution_gates
                    .get(&part.syntax_anchor())
                    .is_some_and(ModuleContributionGate::is_enabled)
            },
            |declaration| {
                self.product_kind() == ProductKind::Test
                    || declaration.kind() != DeclarationKind::Function
                    || !declaration.surface().directives().iter().any(|directive| {
                        directive.syntax_kind() == bray_syntax::SyntaxKind::TestDirective
                    })
            },
        );

        let (declarations, declaration_diagnostics) = selected.into_parts();

        let diagnostics = gate_diagnostics.merged(&declaration_diagnostics);

        Ok(ProductSourceGraph::new(
            self.product_kind(),
            declarations,
            contribution_gates,
            diagnostics,
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::ptr;

    use bray_declarations::DeclarationName;
    use bray_diagnostics::{DiagnosticArg, DiagnosticKind};
    use bray_symbols::{ProductKind, SymbolOrigin};
    use bray_syntax::SyntaxKind;

    use super::ProductSourceGraph;
    use crate::WorkerBudget;
    use crate::test_support::compilation_with_sources_product_and_worker_budget;

    #[test]
    fn product_source_graphs_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<ProductSourceGraph>();
    }

    #[test]
    fn repeated_and_concurrent_product_source_graph_demand_is_stable() {
        let compilation = compilation(&["module app;"], ProductKind::Library);

        let graphs = std::thread::scope(|scope| {
            let first = scope.spawn(|| compilation.product_source_graph());
            let second = scope.spawn(|| compilation.product_source_graph());

            [first, second].map(|thread| {
                thread
                    .join()
                    .unwrap_or_else(|_| panic!("source graph demand must not panic"))
                    .unwrap_or_else(|error| panic!("source graph demand must complete: {error:?}"))
            })
        });

        assert!(ptr::eq(graphs[0], graphs[1]));
        assert_eq!(graphs[0], graphs[1]);
    }

    #[test]
    fn test_only_source_units_contribute_only_to_test_products() {
        let sources = [
            concat!(
                "@test\n",
                "module app.tests;\n",
                "\n",
                "func only_test()\n",
                "{\n",
                "}\n",
            ),
            concat!("module app;\n", "\n", "func ordinary()\n", "{\n", "}\n"),
        ];

        let library = compilation(&sources, ProductKind::Library);
        let test = compilation(&sources, ProductKind::Test);

        assert!(!has_declaration(source_graph(&library), "only_test"));
        assert!(has_discovered_declaration(&library, "only_test"));
        assert_eq!(library.symbol_graph().map(source_function_count), Ok(1));

        assert!(has_declaration(source_graph(&test), "only_test"));
        assert_eq!(test.symbol_graph().map(source_function_count), Ok(2));
    }

    #[test]
    fn disabled_block_module_suffixes_do_not_remove_source_unit_prefixes() {
        let source = concat!(
            "module app;\n",
            "\n",
            "func ordinary()\n",
            "{\n",
            "}\n",
            "\n",
            "@test\n",
            "module app.tests\n",
            "{\n",
            "    func only_test()\n",
            "    {\n",
            "    }\n",
            "}\n",
            "\n",
            "@target(false)\n",
            "module app.disabled\n",
            "{\n",
            "    func unavailable()\n",
            "    {\n",
            "    }\n",
            "}\n",
        );

        let library = compilation(&[source], ProductKind::Library);
        let test = compilation(&[source], ProductKind::Test);

        let library_graph = source_graph(&library);
        let test_graph = source_graph(&test);

        assert!(has_declaration(library_graph, "ordinary"));
        assert!(!has_declaration(library_graph, "only_test"));
        assert!(!has_declaration(library_graph, "unavailable"));
        assert_eq!(library_graph.declarations().module_parts().len(), 1);

        assert!(has_declaration(test_graph, "ordinary"));
        assert!(has_declaration(test_graph, "only_test"));
        assert!(!has_declaration(test_graph, "unavailable"));
        assert_eq!(test_graph.declarations().module_parts().len(), 2);
    }

    #[test]
    fn disabled_source_unit_prefixes_do_not_remove_block_module_suffixes() {
        let source = concat!(
            "@test\n",
            "module app;\n",
            "\n",
            "func only_test()\n",
            "{\n",
            "}\n",
            "\n",
            "module app.production\n",
            "{\n",
            "    func ordinary()\n",
            "    {\n",
            "    }\n",
            "}\n",
        );

        let library = compilation(&[source], ProductKind::Library);
        let test = compilation(&[source], ProductKind::Test);

        let library_graph = source_graph(&library);
        let test_graph = source_graph(&test);

        assert!(!has_declaration(library_graph, "only_test"));
        assert!(has_declaration(library_graph, "ordinary"));
        assert_eq!(library_graph.declarations().module_parts().len(), 1);

        assert!(has_declaration(test_graph, "only_test"));
        assert!(has_declaration(test_graph, "ordinary"));
        assert_eq!(test_graph.declarations().module_parts().len(), 2);
    }

    #[test]
    fn test_functions_contribute_only_to_test_products() {
        let source = concat!(
            "module app;\n",
            "\n",
            "@test\n",
            "func only_test()\n",
            "{\n",
            "}\n",
            "\n",
            "func ordinary()\n",
            "{\n",
            "}\n",
        );

        let library = compilation(&[source], ProductKind::Library);
        let test = compilation(&[source], ProductKind::Test);

        assert!(!has_declaration(source_graph(&library), "only_test"));
        assert!(has_discovered_declaration(&library, "only_test"));

        assert!(has_declaration(source_graph(&test), "only_test"));
    }

    #[test]
    fn test_directives_on_other_declarations_remain_available_for_validation() {
        let source = concat!(
            "module app;\n",
            "\n",
            "@test\n",
            "struct InvalidTest\n",
            "{\n",
            "}\n",
        );

        let library = compilation(&[source], ProductKind::Library);

        assert!(has_declaration(source_graph(&library), "InvalidTest"));
    }

    #[test]
    fn target_gates_remove_disabled_contributions_before_symbol_identity() {
        let sources = [
            concat!(
                "@target(false)\n",
                "module app;\n",
                "\n",
                "func disabled()\n",
                "{\n",
                "}\n",
            ),
            concat!("module app;\n", "\n", "func enabled()\n", "{\n", "}\n",),
        ];

        let compilation = compilation(&sources, ProductKind::Library);
        let graph = source_graph(&compilation);

        assert!(!has_declaration(graph, "disabled"));
        assert!(has_declaration(graph, "enabled"));
        assert_eq!(compilation.symbol_graph().map(source_function_count), Ok(1));
    }

    #[test]
    fn inactive_contributions_do_not_create_split_module_conflicts() {
        let sources = [
            concat!("@test\n", "trusted module app;\n"),
            "internal module app {}\n",
        ];

        let library = compilation(&sources, ProductKind::Library);
        let test = compilation(&sources, ProductKind::Test);

        assert!(
            !library
                .product_source_graph()
                .is_ok_and(|graph| graph.diagnostics().iter().any(|diagnostic| {
                    diagnostic.kind() == DiagnosticKind::DeclarationConflictingModuleVisibility
                }))
        );

        let test = source_graph(&test);

        assert_eq!(test.declarations().module_parts().len(), 2);

        assert_eq!(
            test.declarations().module_parts()[0].surface().visibility(),
            None
        );

        assert_eq!(
            test.declarations().module_parts()[1].surface().visibility(),
            Some(SyntaxKind::InternalKeyword)
        );

        assert!(
            test.diagnostics().iter().any(|diagnostic| {
                diagnostic.kind() == DiagnosticKind::DeclarationConflictingModuleVisibility
            }),
            "{:?}",
            test.diagnostics()
        );
    }

    #[test]
    fn duplicate_module_contribution_gates_are_diagnosed_structurally() {
        let source = concat!("@test\n", "@test\n", "@test\n", "module app;\n");
        let compilation = compilation(&[source], ProductKind::Test);

        let graph = compilation
            .product_source_graph()
            .unwrap_or_else(|error| panic!("test source graph must build: {error:?}"));

        let [second, third] = graph.diagnostics().diagnostics() else {
            panic!("each duplicate contribution gate must produce one diagnostic");
        };

        for diagnostic in [second, third] {
            assert_eq!(
                diagnostic.kind(),
                DiagnosticKind::CheckingDuplicateModuleContributionDirective
            );

            assert_eq!(
                diagnostic.args(),
                &[DiagnosticArg::actual_syntax_kind(SyntaxKind::TestDirective)]
            );
        }

        assert_eq!(second.related_locations().len(), 1);
        assert_eq!(third.related_locations().len(), 2);

        bray_testing::assert_goal_state_diagnostic_kind(
            graph.diagnostics(),
            DiagnosticKind::CheckingDuplicateModuleContributionDirective,
        );
    }

    #[test]
    fn invalid_target_gates_disable_their_contributions_with_diagnostics() {
        let compilation = compilation(
            &["@target(missing.value)\nmodule app;\n"],
            ProductKind::Library,
        );

        let graph = source_graph(&compilation);

        assert!(graph.declarations().module_parts().is_empty());
        assert!(graph.diagnostics().has_errors());
    }

    #[test]
    fn source_graph_selection_does_not_bind_unrelated_directive_arguments() {
        let compilation = compilation(
            &["@link(missing.value)\nmodule app;\n"],
            ProductKind::Library,
        );

        let graph = source_graph(&compilation);

        assert!(graph.diagnostics().is_empty(), "{:?}", graph.diagnostics());
        assert_eq!(graph.declarations().module_parts().len(), 1);
    }

    fn compilation(sources: &[&str], product_kind: ProductKind) -> crate::Compilation {
        compilation_with_sources_product_and_worker_budget(
            sources,
            product_kind,
            WorkerBudget::serial(),
        )
    }

    fn has_declaration(graph: &ProductSourceGraph, name: &str) -> bool {
        graph
            .declarations()
            .declarations()
            .iter()
            .any(|declaration| declaration_name(declaration.name()) == Some(name))
    }

    fn has_discovered_declaration(compilation: &crate::Compilation, name: &str) -> bool {
        compilation
            .declaration_table()
            .declarations()
            .iter()
            .any(|declaration| declaration_name(declaration.name()) == Some(name))
    }

    fn declaration_name(name: Option<&DeclarationName>) -> Option<&str> {
        name.and_then(DeclarationName::as_identifier)
    }

    fn source_graph(compilation: &crate::Compilation) -> &ProductSourceGraph {
        compilation
            .product_source_graph()
            .unwrap_or_else(|error| panic!("test source graph must build: {error:?}"))
    }

    fn source_function_count(graph: &bray_symbols::SymbolGraph) -> usize {
        graph
            .functions()
            .iter()
            .filter(|function| function.origin() == SymbolOrigin::Source)
            .count()
    }
}
