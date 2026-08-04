use std::collections::BTreeSet;

use bray_binder::SymbolFactProvider;
use bray_diagnostics::{DiagnosticBag, DiagnosticKind, DiagnosticResult};
use bray_symbols::{
    AnySymbolId, DeclarationDirectivesFact, DirectiveArgumentName, DirectiveKind,
    DirectiveTemplate, ProductKind, ProductSemanticFacts, ProductTestEntry, SymbolFactRequest,
    TestExecutionConstraint,
};

use super::dependency::validate_public_expression_dependencies;
use super::entry::{ProductEntryKind, select_executable_entrypoint, validate_entry};
use super::visibility::{symbol_is_publicly_reachable, validate_public_surface};
use crate::compilation::Compilation;
use crate::compilation::binder::binder_fact_error;
use crate::compilation::diagnostics::source_diagnostic;
use crate::compilation::directive::{bare_directive_argument_name, first_directive};
use crate::fact::{CancellationToken, CompilationFactKey, FactQueryError};

impl Compilation {
    /// Returns semantic roots and public declarations for the selected product.
    pub fn product_semantic_facts(
        &self,
    ) -> Result<&DiagnosticResult<ProductSemanticFacts>, FactQueryError> {
        self.product_semantic_facts_with_cancellation(&self.state.cancellation)
    }

    pub(in crate::compilation) fn product_semantic_facts_with_cancellation(
        &self,
        cancellation: &CancellationToken,
    ) -> Result<&DiagnosticResult<ProductSemanticFacts>, FactQueryError> {
        self.query_fact_with_cancellation(
            CompilationFactKey::ProductSemantics,
            &self.state.product_semantics,
            cancellation,
            |cancellation| self.compute_product_semantic_facts(cancellation),
        )
    }

    fn compute_product_semantic_facts(
        &self,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<ProductSemanticFacts>, FactQueryError> {
        let source_graph = self.product_source_graph()?;
        let symbols = self.symbol_graph()?;
        let binder = self.binder_facts(cancellation)?;
        let semantic_values = self.semantic_value_store()?;
        let available = self.available_compiler_known_symbols();
        let kind = source_graph.product_kind();

        let mut diagnostics = DiagnosticBag::new();
        let mut functions = Vec::new();
        let mut public_symbols = BTreeSet::new();

        for declaration in source_graph.declarations().declarations() {
            cancellation.check()?;

            let Some(symbol) = symbols.symbol_for_declaration(declaration.id()) else {
                continue;
            };

            if symbol_is_publicly_reachable(source_graph.declarations(), symbols, symbol) {
                public_symbols.insert(symbol);
            }

            let AnySymbolId::Function(function) = symbol else {
                continue;
            };

            functions.push(function);
        }

        let mut explicit_entrypoints = Vec::new();
        let mut test_entries = Vec::new();
        let mut test_identities = BTreeSet::new();
        let mut requires_async_runtime = false;
        let mut is_recovered = false;

        for function in functions.iter().copied() {
            let directives = binder
                .symbol_fact(SymbolFactRequest::<DeclarationDirectivesFact>::new(
                    function.into(),
                ))
                .map_err(binder_fact_error)?;

            diagnostics.add_range(directives.diagnostics().iter().cloned());

            is_recovered |= directives.diagnostics().has_errors();

            if let Some(entrypoint) = first_directive(directives.value(), DirectiveKind::Entrypoint)
            {
                if kind == ProductKind::Executable {
                    explicit_entrypoints.push((function, entrypoint.syntax()));
                } else {
                    diagnostics.add(source_diagnostic(
                        entrypoint.syntax(),
                        DiagnosticKind::CheckingEntrypointNotAllowed,
                    ));

                    is_recovered = true;
                }
            }

            if kind == ProductKind::Test
                && let Some(directive) = first_directive(directives.value(), DirectiveKind::Test)
            {
                let Some(constraint) =
                    self.test_execution_constraint(directive, &mut diagnostics)?
                else {
                    is_recovered = true;

                    continue;
                };

                let validation = validate_entry(
                    &binder,
                    semantic_values,
                    available,
                    symbols,
                    function,
                    ProductEntryKind::Test,
                    &mut diagnostics,
                )?;

                if let Some(validation) = validation {
                    let result = validation
                        .test_result()
                        .ok_or(FactQueryError::InfrastructureFailure)?;

                    let module = symbols
                        .containing_module(function.into())
                        .ok_or(FactQueryError::InfrastructureFailure)?;

                    let name = symbols
                        .member_name(function.into())
                        .cloned()
                        .ok_or(FactQueryError::InfrastructureFailure)?;

                    let identity = (module.path().clone(), name.clone());

                    if !test_identities.insert(identity) {
                        diagnostics.add(source_diagnostic(
                            directive.syntax(),
                            DiagnosticKind::CheckingDuplicateTestIdentity,
                        ));

                        is_recovered = true;

                        continue;
                    }

                    test_entries.push(ProductTestEntry::new(
                        function,
                        module.path().clone(),
                        name,
                        validation.execution(),
                        constraint,
                        result,
                    ));

                    requires_async_runtime |= validation.is_async();
                } else {
                    is_recovered = true;
                }
            }
        }

        let entrypoint = if kind == ProductKind::Executable {
            let selection = select_executable_entrypoint(
                &binder,
                semantic_values,
                available,
                symbols,
                &functions,
                &explicit_entrypoints,
                &mut diagnostics,
            )?;

            requires_async_runtime |= selection.is_async();
            is_recovered |= selection.is_recovered();

            selection.function()
        } else {
            None
        };

        is_recovered |= validate_public_surface(
            &binder,
            semantic_values,
            symbols,
            source_graph.declarations(),
            &public_symbols,
            &mut diagnostics,
        )?;

        is_recovered |= validate_public_expression_dependencies(
            self,
            cancellation,
            semantic_values,
            symbols,
            source_graph.declarations(),
            &public_symbols,
            &mut diagnostics,
        )?;

        let facts = ProductSemanticFacts::new(
            kind,
            entrypoint,
            test_entries,
            public_symbols,
            requires_async_runtime,
            is_recovered,
        );

        Ok(DiagnosticResult::new(facts, diagnostics))
    }

    fn test_execution_constraint(
        &self,
        directive: &DirectiveTemplate,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<TestExecutionConstraint>, FactQueryError> {
        let constraint = match directive.arguments() {
            [] => Some(TestExecutionConstraint::Parallel),
            [argument] if matches!(argument.name(), DirectiveArgumentName::Positional) => {
                let syntax = argument.expression().syntax();

                let source = self
                    .source(syntax.source_id())
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                let name = bare_directive_argument_name(
                    self.syntax_tree_result().syntax_tree(),
                    source,
                    syntax,
                );

                (name.as_ref().map(bray_symbols::SymbolName::as_str) == Some("serial"))
                    .then_some(TestExecutionConstraint::Serial)
            }
            [_] | [_, ..] => None,
        };

        if constraint.is_none() {
            diagnostics.add(source_diagnostic(
                directive.syntax(),
                DiagnosticKind::CheckingInvalidTestEntryDirective,
            ));
        }

        Ok(constraint)
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticKind;
    use bray_symbols::ProductKind;

    use crate::test_support::{compilation_with_product, diagnostic_kinds};

    #[test]
    fn explicit_async_entrypoint_is_selected_and_cached() {
        let compilation = compilation_with_product(
            concat!(
                "module app;\n",
                "\n",
                "@entrypoint\n",
                "async func start()\n",
                "{\n",
                "}\n",
            ),
            ProductKind::Executable,
        );

        let first = product_facts(&compilation);
        let second = product_facts(&compilation);
        let symbols = symbol_graph(&compilation);

        let entrypoint = first
            .value()
            .entrypoint()
            .unwrap_or_else(|| panic!("explicit entrypoint must be selected"));

        assert!(std::ptr::eq(first, second));
        assert!(first.diagnostics().is_empty());
        assert!(first.value().requires_async_runtime());

        assert_eq!(
            symbols
                .member_name(entrypoint.into())
                .map(bray_symbols::SymbolName::as_str),
            Some("start")
        );
    }

    #[test]
    fn unique_valid_main_is_the_executable_fallback() {
        let compilation = compilation_with_product(
            "module app;

func main()
{
}
",
            ProductKind::Executable,
        );

        let facts = product_facts(&compilation);

        assert!(facts.diagnostics().is_empty());
        assert!(facts.value().entrypoint().is_some());
        assert!(!facts.value().requires_async_runtime());
    }

    #[test]
    fn executable_entrypoint_selection_reports_missing_and_duplicate_roots() {
        let missing = compilation_with_product("module app;\n", ProductKind::Executable);

        let duplicate = compilation_with_product(
            concat!(
                "module app;\n",
                "\n",
                "@entrypoint\n",
                "func first()\n",
                "{\n",
                "}\n",
                "\n",
                "@entrypoint\n",
                "func second()\n",
                "{\n",
                "}\n",
            ),
            ProductKind::Executable,
        );

        assert_eq!(
            diagnostic_kinds(product_facts(&missing).diagnostics()),
            [DiagnosticKind::CheckingMissingEntrypoint]
        );

        assert_eq!(
            diagnostic_kinds(product_facts(&duplicate).diagnostics()),
            [DiagnosticKind::CheckingDuplicateEntrypoint]
        );
    }

    #[test]
    fn check_diagnostics_demand_product_validation() {
        let compilation = compilation_with_product("module app;\n", ProductKind::Executable);

        assert!(
            diagnostic_kinds(compilation.check_diagnostics())
                .contains(&DiagnosticKind::CheckingMissingEntrypoint)
        );
    }

    #[test]
    fn non_executable_products_reject_entrypoint_directives() {
        let source = concat!(
            "module app;\n",
            "\n",
            "@entrypoint\n",
            "func start()\n",
            "{\n",
            "}\n",
        );

        for kind in [ProductKind::Library, ProductKind::Test] {
            let compilation = compilation_with_product(source, kind);

            assert!(
                diagnostic_kinds(product_facts(&compilation).diagnostics())
                    .contains(&DiagnosticKind::CheckingEntrypointNotAllowed)
            );
        }
    }

    #[test]
    fn test_products_publish_valid_async_test_entries() {
        let compilation = compilation_with_product(
            concat!(
                "module app;\n",
                "\n",
                "@test\n",
                "async func runs()\n",
                "{\n",
                "}\n",
            ),
            ProductKind::Test,
        );

        let facts = product_facts(&compilation);

        assert!(facts.diagnostics().is_empty());
        assert_eq!(facts.value().test_entries().len(), 1);
        assert!(facts.value().requires_async_runtime());
    }

    #[test]
    fn test_products_retain_serial_entry_constraints() {
        let compilation = compilation_with_product(
            "module app;\n\n@test(serial)\nfunc runs_alone()\n{\n}\n",
            ProductKind::Test,
        );

        let facts = product_facts(&compilation);

        assert!(facts.diagnostics().is_empty(), "{:?}", facts.diagnostics());
        assert_eq!(facts.value().test_entries().len(), 1);

        assert_eq!(
            facts.value().test_entries()[0].constraint(),
            bray_symbols::TestExecutionConstraint::Serial
        );
    }

    #[test]
    fn test_products_reject_unknown_test_directive_arguments() {
        let compilation = compilation_with_product(
            "module app;\n\n@test(other)\nfunc invalid()\n{\n}\n",
            ProductKind::Test,
        );

        let facts = product_facts(&compilation);

        assert_eq!(
            diagnostic_kinds(facts.diagnostics()),
            [DiagnosticKind::CheckingInvalidTestEntryDirective]
        );

        assert!(facts.value().test_entries().is_empty());
        assert!(facts.value().is_recovered());
    }

    #[test]
    fn test_products_reject_duplicate_test_identities() {
        let compilation = compilation_with_product(
            concat!(
                "module app;\n",
                "\n",
                "@test\n",
                "func repeated()\n",
                "{\n",
                "}\n",
                "\n",
                "@test\n",
                "func repeated()\n",
                "{\n",
                "}\n",
            ),
            ProductKind::Test,
        );

        let facts = product_facts(&compilation);

        assert_eq!(
            diagnostic_kinds(facts.diagnostics()),
            [DiagnosticKind::CheckingDuplicateTestIdentity]
        );

        assert_eq!(facts.value().test_entries().len(), 1);
        assert!(facts.value().is_recovered());
    }

    #[test]
    fn test_products_reject_invalid_test_results() {
        let compilation = compilation_with_product(
            concat!(
                "module app;\n",
                "\n",
                "@test\n",
                "func returns_value() -> i32\n",
                "{\n",
                "    1\n",
                "}\n",
            ),
            ProductKind::Test,
        );

        let facts = product_facts(&compilation);

        assert!(
            diagnostic_kinds(facts.diagnostics())
                .contains(&DiagnosticKind::CheckingInvalidTestResult)
        );

        assert!(facts.value().test_entries().is_empty());
        assert!(facts.value().is_recovered());
    }

    #[test]
    fn invalid_entry_contracts_remain_diagnostic_product_facts() {
        let compilation = compilation_with_product(
            concat!(
                "trusted module app;\n",
                "\n",
                "trusted predicate permitted();\n",
                "\n",
                "@entrypoint\n",
                "const func start<T>(pos value: i32) -> bool\n",
                "    requires(trusted permitted())\n",
                "{\n",
                "    false\n",
                "}\n",
            ),
            ProductKind::Executable,
        );

        let facts = product_facts(&compilation);
        let kinds = diagnostic_kinds(facts.diagnostics());

        assert!(kinds.contains(&DiagnosticKind::CheckingEntryCannotBeGeneric));
        assert!(kinds.contains(&DiagnosticKind::CheckingEntryCannotTakeParameters));
        assert!(kinds.contains(&DiagnosticKind::CheckingInvalidEntrypointResult));
        assert!(kinds.contains(&DiagnosticKind::CheckingEntryCannotBeConstant));
        assert!(kinds.contains(&DiagnosticKind::CheckingEntryCannotRequireTrust));
        assert!(facts.value().entrypoint().is_none());
        assert!(facts.value().is_recovered());
    }

    #[test]
    fn trusted_implementation_without_caller_obligations_is_a_valid_entry() {
        let compilation = compilation_with_product(
            concat!(
                "trusted module app;\n",
                "\n",
                "@entrypoint\n",
                "trusted func start()\n",
                "{\n",
                "}\n",
            ),
            ProductKind::Executable,
        );

        let facts = product_facts(&compilation);

        assert!(facts.diagnostics().is_empty());
        assert!(facts.value().entrypoint().is_some());
    }

    #[test]
    fn public_signatures_cannot_expose_internal_source_types() {
        let compilation = compilation_with_product(
            concat!(
                "module app;\n",
                "\n",
                "internal struct Hidden\n",
                "{\n",
                "}\n",
                "\n",
                "public struct Exposed\n",
                "{\n",
                "    value: Hidden;\n",
                "}\n",
                "\n",
                "public func expose(pos value: Hidden)\n",
                "{\n",
                "}\n",
            ),
            ProductKind::Library,
        );

        let facts = product_facts(&compilation);
        let kinds = diagnostic_kinds(facts.diagnostics());

        assert_eq!(
            kinds
                .iter()
                .filter(|kind| {
                    **kind == DiagnosticKind::CheckingExportDependsOnInternalDeclaration
                })
                .count(),
            2
        );

        assert!(facts.value().is_recovered());
    }

    #[test]
    fn public_trait_views_cannot_expose_internal_traits() {
        let compilation = compilation_with_product(
            concat!(
                "module app;\n",
                "\n",
                "internal trait Hidden\n",
                "{\n",
                "}\n",
                "\n",
                "public struct Exposed\n",
                "{\n",
                "    value: view Hidden;\n",
                "}\n",
            ),
            ProductKind::Library,
        );

        let facts = product_facts(&compilation);

        assert!(
            diagnostic_kinds(facts.diagnostics())
                .contains(&DiagnosticKind::CheckingExportDependsOnInternalDeclaration)
        );
    }

    #[test]
    fn public_trait_predicates_cannot_expose_internal_types() {
        let compilation = compilation_with_product(
            concat!(
                "module app;\n",
                "\n",
                "internal struct Hidden\n",
                "{\n",
                "}\n",
                "\n",
                "public trait Visible\n",
                "{\n",
                "    predicate accepts(value: Hidden);\n",
                "}\n",
            ),
            ProductKind::Library,
        );

        let facts = product_facts(&compilation);

        assert!(
            diagnostic_kinds(facts.diagnostics())
                .contains(&DiagnosticKind::CheckingExportDependsOnInternalDeclaration)
        );
    }

    #[test]
    fn public_constraints_and_contracts_cannot_reference_internal_declarations() {
        let compilation = compilation_with_product(
            concat!(
                "module app;\n",
                "\n",
                "internal const hidden_value: bool = true;\n",
                "\n",
                "internal func hidden_call() -> bool\n",
                "{\n",
                "    return true;\n",
                "}\n",
                "\n",
                "public struct Exposed with(hidden_value)\n",
                "{\n",
                "}\n",
                "\n",
                "public func expose() requires(hidden_call())\n",
                "{\n",
                "}\n",
            ),
            ProductKind::Library,
        );

        let facts = product_facts(&compilation);

        assert_eq!(
            diagnostic_kinds(facts.diagnostics())
                .iter()
                .filter(|kind| {
                    **kind == DiagnosticKind::CheckingExportDependsOnInternalDeclaration
                })
                .count(),
            2
        );

        assert!(facts.value().is_recovered());
    }

    #[test]
    fn public_contracts_cannot_use_internal_generic_arguments() {
        let compilation = compilation_with_product(
            concat!(
                "module app;\n",
                "\n",
                "internal struct Hidden\n",
                "{\n",
                "}\n",
                "\n",
                "public func accepts<T>() -> bool\n",
                "{\n",
                "    return true;\n",
                "}\n",
                "\n",
                "public func expose() requires(accepts<Hidden>())\n",
                "{\n",
                "}\n",
            ),
            ProductKind::Library,
        );

        let facts = product_facts(&compilation);

        assert!(
            diagnostic_kinds(facts.diagnostics())
                .contains(&DiagnosticKind::CheckingExportDependsOnInternalDeclaration)
        );
    }

    #[test]
    fn public_surfaces_cannot_reach_declarations_through_internal_modules() {
        let compilation = compilation_with_product(
            concat!(
                "internal module hidden\n",
                "{\n",
                "    public struct Hidden\n",
                "    {\n",
                "    }\n",
                "}\n",
            ),
            ProductKind::Library,
        );

        let source_graph = compilation
            .product_source_graph()
            .unwrap_or_else(|error| panic!("test source graph must build: {error:?}"));

        let symbols = symbol_graph(&compilation);

        let hidden = symbols
            .structures()
            .iter()
            .find(|structure| {
                structure
                    .declaration()
                    .and_then(|declaration| source_graph.declarations().declaration(declaration))
                    .and_then(bray_declarations::DeclarationRecord::name)
                    .and_then(bray_declarations::DeclarationName::as_identifier)
                    == Some("Hidden")
            })
            .unwrap_or_else(|| panic!("test structure must have a symbol"));

        assert!(!super::symbol_is_publicly_reachable(
            source_graph.declarations(),
            symbols,
            hidden.id().into(),
        ));
    }

    fn product_facts(
        compilation: &crate::Compilation,
    ) -> &bray_diagnostics::DiagnosticResult<bray_symbols::ProductSemanticFacts> {
        compilation
            .product_semantic_facts()
            .unwrap_or_else(|error| panic!("product semantic facts must build: {error:?}"))
    }

    fn symbol_graph(compilation: &crate::Compilation) -> &bray_symbols::SymbolGraph {
        compilation
            .symbol_graph()
            .unwrap_or_else(|error| panic!("test symbol graph must build: {error:?}"))
    }
}
