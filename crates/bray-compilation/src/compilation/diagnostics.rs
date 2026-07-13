use std::collections::BTreeSet;
use std::sync::Arc;

use bray_bound_tree::{
    BoundSourceAnchor, BoundUnit, BoundUnitKey, BoundUnitKind, CheckedControlFlowFacts,
};
use bray_declarations::{DeclarationKind, DeclarationRecord, SyntaxAnchor};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{AnySymbolId, SymbolGraph, SymbolKey};
use bray_syntax::{CallableBodyBlockExpressionSyntax, ExpressionSyntax, SyntaxTree};

use super::Compilation;
use crate::fact::{CompilationFactKey, FactQueryError};

impl Compilation {
    /// Returns diagnostics produced by binding and semantic analysis of this package.
    pub fn semantic_diagnostics(&self) -> &DiagnosticBag {
        self.fact(
            CompilationFactKey::SemanticDiagnostics,
            &self.state.semantic_diagnostics,
            || match self.compute_semantic_diagnostics() {
                Ok(diagnostics) => diagnostics,
                Err(FactQueryError::Cancelled) => {
                    panic!("uncancellable semantic diagnostics were unexpectedly cancelled")
                }
                Err(FactQueryError::Cycle(cycle)) => {
                    panic!("semantic diagnostic dependencies formed a cycle: {cycle:?}")
                }
                Err(FactQueryError::InfrastructureFailure) => {
                    panic!("semantic diagnostic infrastructure failed")
                }
            },
        )
    }

    /// Returns diagnostics for the current whole-package check request.
    pub fn check_diagnostics(&self) -> &DiagnosticBag {
        self.fact(
            CompilationFactKey::CheckDiagnostics,
            &self.state.check_diagnostics,
            || {
                DiagnosticBag::merged_all([
                    self.source_diagnostics(),
                    self.syntax_tree_result().diagnostics(),
                    self.declaration_diagnostics(),
                    self.semantic_diagnostics(),
                ])
            },
        )
    }

    fn compute_semantic_diagnostics(&self) -> Result<DiagnosticBag, FactQueryError> {
        let mut pending = BTreeSet::new();

        for key in self.declared_unit_keys()? {
            pending.insert(unit_order_key(key));
        }

        let mut facts = Vec::new();

        while let Some((_, _, _, key)) = pending.pop_first() {
            let bound = self.bound_unit(key.clone())?;

            for nested in bound.value().nested_units() {
                // Nested unit keys are Arc-backed immutable identities shared with their owner.
                pending.insert(unit_order_key(nested.clone()));
            }

            let control_flow = self.checked_control_flow(key)?;

            facts.push(SemanticDiagnosticFact::Bound(bound));
            facts.push(SemanticDiagnosticFact::ControlFlow(control_flow));
        }

        Ok(DiagnosticBag::merged_all(
            facts.iter().map(SemanticDiagnosticFact::diagnostics),
        ))
    }

    fn declared_unit_keys(&self) -> Result<Vec<BoundUnitKey>, FactQueryError> {
        let symbols = self.symbol_graph()?;
        let syntax = self.syntax_tree();
        let mut keys = Vec::new();

        for declaration in self.declaration_table().declarations() {
            let Some(symbol) = symbols.symbol_for_declaration(declaration.id()) else {
                continue;
            };

            // Stable symbol keys are Arc-backed and retained by each bound-unit key.
            let owner = symbols
                .symbol_key(symbol)
                .cloned()
                .ok_or(FactQueryError::InfrastructureFailure)?;

            self.push_primary_unit_key(&mut keys, declaration, owner.clone(), syntax)?;
            self.push_surface_unit_keys(&mut keys, declaration, symbol, owner, symbols, syntax)?;
        }

        Ok(keys)
    }

    fn push_primary_unit_key(
        &self,
        keys: &mut Vec<BoundUnitKey>,
        declaration: &DeclarationRecord,
        owner: SymbolKey,
        syntax: &SyntaxTree,
    ) -> Result<(), FactQueryError> {
        let anchor = declaration.syntax_anchor();

        if callable_body_kind(declaration.kind())
            && anchor
                .find_descendant::<CallableBodyBlockExpressionSyntax>(syntax)
                .is_some()
        {
            push_key(
                keys,
                BoundUnitKey::callable_body(owner, self.bound_source(anchor)?),
            )?;

            return Ok(());
        }

        let Some(expression) = expression_anchor(anchor, syntax) else {
            return Ok(());
        };

        let source = self.bound_source(expression)?;
        let key = match declaration.kind() {
            DeclarationKind::Constant | DeclarationKind::TraitConstantMember => {
                BoundUnitKey::constant_template(owner, source)
            }
            DeclarationKind::Predicate | DeclarationKind::TraitPredicateMember => {
                BoundUnitKey::predicate_definition(owner, source)
            }
            _ => return Ok(()),
        };

        push_key(keys, key)
    }

    fn push_surface_unit_keys(
        &self,
        keys: &mut Vec<BoundUnitKey>,
        declaration: &DeclarationRecord,
        symbol: AnySymbolId,
        owner: SymbolKey,
        symbols: &SymbolGraph,
        syntax: &SyntaxTree,
    ) -> Result<(), FactQueryError> {
        for anchor in declaration.surface().constraints() {
            if expression_anchor(*anchor, syntax).is_some() {
                push_key(
                    keys,
                    BoundUnitKey::constraint(owner.clone(), self.bound_source(*anchor)?),
                )?;
            }
        }

        for anchor in declaration.surface().contract_clauses() {
            if expression_anchor(*anchor, syntax).is_some() {
                push_key(
                    keys,
                    BoundUnitKey::contract_clause(owner.clone(), self.bound_source(*anchor)?),
                )?;
            }
        }

        let Some(default) = declaration.surface().runtime_default() else {
            return Ok(());
        };

        let Some(expression) = expression_anchor(default, syntax) else {
            return Ok(());
        };

        let provider = symbols
            .runtime_default_provider(symbol)
            .ok_or(FactQueryError::InfrastructureFailure)?;

        // Synthesized provider keys are Arc-backed and retained by the runtime-default unit key.
        let provider = symbols
            .symbol_key(provider)
            .cloned()
            .ok_or(FactQueryError::InfrastructureFailure)?;

        push_key(
            keys,
            BoundUnitKey::runtime_default(provider, self.bound_source(expression)?),
        )
    }

    fn bound_source(&self, anchor: SyntaxAnchor) -> Result<BoundSourceAnchor, FactQueryError> {
        let source = self
            .source(anchor.source_id())
            .ok_or(FactQueryError::InfrastructureFailure)?;

        Ok(BoundSourceAnchor::new(anchor, source.version()))
    }
}

enum SemanticDiagnosticFact {
    Bound(Arc<DiagnosticResult<BoundUnit>>),
    ControlFlow(Arc<DiagnosticResult<CheckedControlFlowFacts>>),
}

impl SemanticDiagnosticFact {
    fn diagnostics(&self) -> &DiagnosticBag {
        match self {
            Self::Bound(result) => result.diagnostics(),
            Self::ControlFlow(result) => result.diagnostics(),
        }
    }
}

fn callable_body_kind(kind: DeclarationKind) -> bool {
    matches!(
        kind,
        DeclarationKind::Function
            | DeclarationKind::TraitCallableMember
            | DeclarationKind::TypeConstructorMember
            | DeclarationKind::FinalizerMember
            | DeclarationKind::DestructorMember
            | DeclarationKind::ScopeEnterMember
            | DeclarationKind::ScopeExitMember
            | DeclarationKind::TypeCallableMember
    )
}

fn expression_anchor(anchor: SyntaxAnchor, syntax: &SyntaxTree) -> Option<SyntaxAnchor> {
    anchor
        .find_descendant::<ExpressionSyntax>(syntax)
        .as_ref()
        .map(SyntaxAnchor::from_node)
}

fn push_key(keys: &mut Vec<BoundUnitKey>, key: Option<BoundUnitKey>) -> Result<(), FactQueryError> {
    let key = key.ok_or(FactQueryError::InfrastructureFailure)?;

    keys.push(key);

    Ok(())
}

fn unit_order_key(
    key: BoundUnitKey,
) -> (
    bray_source::SourceId,
    bray_source::TextRange,
    BoundUnitKind,
    BoundUnitKey,
) {
    let source = key.source().syntax();

    (source.source_id(), source.full_range(), key.kind(), key)
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::BoundUnitKind;
    use bray_diagnostics::DiagnosticKind;

    use super::Compilation;
    use crate::test_support::{package_identity, source_input};

    #[test]
    fn package_semantic_diagnostics_request_all_declared_unit_categories() {
        let compilation = compilation(concat!(
            "module app;\n",
            "const size: i32 = 1;\n",
            "predicate valid() = true;\n",
            "struct value with(true)\n",
            "{\n",
            "}\n",
            "func check(value: i32 = 1) requires(true) ensures(true) with(true)\n",
            "{\n",
            "}\n",
        ));

        let mut kinds = match compilation.declared_unit_keys() {
            Ok(keys) => keys.into_iter().map(|key| key.kind()).collect::<Vec<_>>(),
            Err(error) => panic!("semantic unit keys must be discoverable: {error:?}"),
        };

        kinds.sort_unstable();

        assert_eq!(
            kinds,
            [
                BoundUnitKind::CallableBody,
                BoundUnitKind::RuntimeDefault,
                BoundUnitKind::ConstantTemplate,
                BoundUnitKind::PredicateDefinition,
                BoundUnitKind::Constraint,
                BoundUnitKind::ContractClause,
                BoundUnitKind::ContractClause,
                BoundUnitKind::ContractClause,
            ]
        );
    }

    #[test]
    fn check_diagnostics_lazily_request_and_cache_semantic_facts() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func main()\n",
            "{\n",
            "    let value = 1;\n",
            "    let value = 2;\n",
            "}\n",
        ));

        let keys = match compilation.declared_unit_keys() {
            Ok(keys) => keys,
            Err(error) => panic!("callable key must be discoverable: {error:?}"),
        };

        let [key] = keys.as_slice() else {
            panic!("test package must contain one semantic unit");
        };

        assert_eq!(compilation.state.bound_units.is_published(key), Ok(false));
        assert_eq!(
            compilation.state.checked_control_flow.is_published(key),
            Ok(false)
        );

        let first = compilation.check_diagnostics();
        let second = compilation.check_diagnostics();

        assert!(std::ptr::eq(first, second));
        assert_eq!(compilation.state.bound_units.is_published(key), Ok(true));
        assert_eq!(
            compilation.state.checked_control_flow.is_published(key),
            Ok(true)
        );

        assert_eq!(
            first
                .iter()
                .map(bray_diagnostics::Diagnostic::kind)
                .collect::<Vec<_>>(),
            [DiagnosticKind::BindingNameAlreadyDefined]
        );
    }

    #[test]
    fn package_diagnostics_include_nested_unit_diagnostics() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func main()\n",
            "{\n",
            "    let callable = lambda()\n",
            "    {\n",
            "        let value = 1;\n",
            "        let value = 2;\n",
            "    };\n",
            "}\n",
        ));

        assert_eq!(
            compilation
                .check_diagnostics()
                .iter()
                .map(bray_diagnostics::Diagnostic::kind)
                .collect::<Vec<_>>(),
            [DiagnosticKind::BindingNameAlreadyDefined]
        );
    }

    #[test]
    fn declarations_without_semantic_units_do_not_force_binding() {
        let compilation = compilation(concat!(
            "module app;\n",
            "extern func write(value: i32);\n",
            "trait Writer\n",
            "{\n",
            "    func flush();\n",
            "}\n",
        ));

        assert!(
            compilation
                .declared_unit_keys()
                .is_ok_and(|keys| keys.is_empty())
        );
        assert!(compilation.check_diagnostics().is_empty());
    }

    fn compilation(source: &str) -> Compilation {
        match Compilation::load_sources(package_identity(), vec![source_input(source, 0)]) {
            Ok(compilation) => compilation,
            Err(error) => panic!("test compilation must load: {error:?}"),
        }
    }
}
