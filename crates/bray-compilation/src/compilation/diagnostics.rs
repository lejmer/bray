use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use bray_bound_tree::{
    BoundSourceAnchor, BoundUnit, BoundUnitKey, BoundUnitKind, CheckedControlFlowFacts,
};
use bray_declarations::{DeclarationKind, DeclarationRecord, SyntaxAnchor};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{AnySymbolId, SymbolGraph, SymbolKey};
use bray_syntax::{SyntaxKind, SyntaxTree, SyntaxWalkControl, SyntaxWalkEvent, walk_syntax_tree};

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
                    self.imported_diagnostics(),
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
        let declarations = self.declaration_table();
        let syntax_index = SemanticSyntaxIndex::new(syntax, declarations.declarations());

        let mut keys = Vec::new();

        for declaration in declarations.declarations() {
            let Some(symbol) = symbols.symbol_for_declaration(declaration.id()) else {
                continue;
            };

            // Stable symbol keys are Arc-backed and retained by each bound-unit key.
            let owner = symbols
                .symbol_key(symbol)
                .cloned()
                .ok_or(FactQueryError::InfrastructureFailure)?;

            self.push_primary_unit_key(&mut keys, declaration, owner.clone(), &syntax_index)?;

            self.push_surface_unit_keys(
                &mut keys,
                declaration,
                symbol,
                owner,
                symbols,
                &syntax_index,
            )?;
        }

        Ok(keys)
    }

    fn push_primary_unit_key(
        &self,
        keys: &mut Vec<BoundUnitKey>,
        declaration: &DeclarationRecord,
        owner: SymbolKey,
        syntax: &SemanticSyntaxIndex,
    ) -> Result<(), FactQueryError> {
        let anchor = declaration.syntax_anchor();

        if callable_body_kind(declaration.kind()) && syntax.has_callable_body(anchor) {
            push_key(
                keys,
                BoundUnitKey::callable_body(owner, self.bound_source(anchor)?),
            )?;

            return Ok(());
        }

        let constructor = match declaration.kind() {
            DeclarationKind::Constant | DeclarationKind::TraitConstantMember => {
                BoundUnitKey::constant_template
            }
            DeclarationKind::Predicate | DeclarationKind::TraitPredicateMember => {
                BoundUnitKey::predicate_definition
            }
            _ => return Ok(()),
        };

        let Some(expression) = syntax.first_expression(anchor) else {
            return Ok(());
        };

        push_key(keys, constructor(owner, self.bound_source(expression)?))
    }

    fn push_surface_unit_keys(
        &self,
        keys: &mut Vec<BoundUnitKey>,
        declaration: &DeclarationRecord,
        symbol: AnySymbolId,
        owner: SymbolKey,
        symbols: &SymbolGraph,
        syntax: &SemanticSyntaxIndex,
    ) -> Result<(), FactQueryError> {
        for anchor in declaration.surface().constraints() {
            if syntax.has_expression(*anchor) {
                push_key(
                    keys,
                    BoundUnitKey::constraint(owner.clone(), self.bound_source(*anchor)?),
                )?;
            }
        }

        for anchor in declaration.surface().contract_clauses() {
            if syntax.has_expression(*anchor) {
                push_key(
                    keys,
                    BoundUnitKey::contract_clause(owner.clone(), self.bound_source(*anchor)?),
                )?;
            }
        }

        let Some(default) = declaration.surface().runtime_default() else {
            return Ok(());
        };

        let Some(expression) = syntax.first_expression(default) else {
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

struct SemanticSyntaxIndex {
    entries: HashMap<SyntaxAnchor, SemanticSyntaxEntry>,
}

impl SemanticSyntaxIndex {
    fn new<'declaration>(
        syntax: &SyntaxTree,
        declarations: impl IntoIterator<Item = &'declaration DeclarationRecord>,
    ) -> Self {
        let mut entries: HashMap<SyntaxAnchor, SemanticSyntaxEntry> = HashMap::new();

        for declaration in declarations {
            entries.entry(declaration.syntax_anchor()).or_default();

            for anchor in declaration
                .surface()
                .constraints()
                .iter()
                .chain(declaration.surface().contract_clauses())
                .copied()
                .chain(declaration.surface().runtime_default())
            {
                entries.entry(anchor).or_default();
            }
        }

        let mut active = Vec::new();

        walk_syntax_tree(syntax, |event| {
            let node = match event {
                SyntaxWalkEvent::EnterNode(node) => node,
                SyntaxWalkEvent::ExitNode(node) => {
                    let anchor = SyntaxAnchor::from_node(&node);

                    if active.last() == Some(&anchor) {
                        active.pop();
                    }

                    return SyntaxWalkControl::Continue;
                }
                SyntaxWalkEvent::Token(_) => return SyntaxWalkControl::Continue,
            };

            let anchor = SyntaxAnchor::from_node(&node);

            if entries.contains_key(&anchor) {
                active.push(anchor);
            }

            if node.kind() == SyntaxKind::CallableBodyBlockExpression
                && let Some(active_anchor) = active.last()
                && let Some(entry) = entries.get_mut(active_anchor)
            {
                entry.has_callable_body = true;
            }

            for active_anchor in &active {
                let Some(entry) = entries.get_mut(active_anchor) else {
                    continue;
                };

                if node.kind() == SyntaxKind::Expression && entry.first_expression.is_none() {
                    entry.first_expression = Some(anchor);
                }
            }

            SyntaxWalkControl::Continue
        });

        Self { entries }
    }

    fn has_callable_body(&self, anchor: SyntaxAnchor) -> bool {
        self.entries
            .get(&anchor)
            .is_some_and(|entry| entry.has_callable_body)
    }

    fn has_expression(&self, anchor: SyntaxAnchor) -> bool {
        self.first_expression(anchor).is_some()
    }

    fn first_expression(&self, anchor: SyntaxAnchor) -> Option<SyntaxAnchor> {
        self.entries.get(&anchor)?.first_expression
    }
}

#[derive(Default)]
struct SemanticSyntaxEntry {
    first_expression: Option<SyntaxAnchor>,
    has_callable_body: bool,
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
    use bray_bound_tree::{BoundUnitKind, BoundUnitRoot};
    use bray_diagnostics::DiagnosticKind;

    use crate::WorkerBudget;
    use crate::fact::FactCellTestEvent;
    use crate::test_support::{
        FactTestGate, compilation, compilation_with_sources_and_worker_budget, diagnostic_kinds,
    };

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
            diagnostic_kinds(first),
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
            diagnostic_kinds(compilation.check_diagnostics()),
            [DiagnosticKind::BindingNameAlreadyDefined]
        );
    }

    #[test]
    fn package_diagnostics_bind_every_contract_clause_expression() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func check() requires(true, missing)\n",
            "{\n",
            "}\n",
        ));

        let keys = match compilation.declared_unit_keys() {
            Ok(keys) => keys,
            Err(error) => panic!("contract-clause key must be discoverable: {error:?}"),
        };

        let Some(key) = keys
            .into_iter()
            .find(|key| key.kind() == BoundUnitKind::ContractClause)
        else {
            panic!("test source must produce a contract-clause key");
        };

        assert_eq!(
            diagnostic_kinds(compilation.check_diagnostics()),
            [DiagnosticKind::BindingUnresolvedName]
        );

        let bound = match compilation.bound_unit(key) {
            Ok(bound) => bound,
            Err(error) => panic!("contract clause must bind: {error:?}"),
        };

        let BoundUnitRoot::ExpressionSequence(root) = bound.value().root() else {
            panic!("contract clause must publish an expression sequence");
        };

        let Some(sequence) = bound.value().tree().block(root) else {
            panic!("contract-clause sequence root must resolve");
        };

        assert_eq!(sequence.items().len(), 2);
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

    #[test]
    fn malformed_bodies_publish_recovered_semantic_facts_without_panicking() {
        let cases = [
            concat!(
                "module app;\n",
                "func missing_initializer()\n",
                "{\n",
                "    let value = ;\n",
                "}\n",
            ),
            concat!(
                "module app;\n",
                "func malformed_pattern()\n",
                "{\n",
                "    let (first, second = 1;\n",
                "}\n",
            ),
        ];

        for source in cases {
            let compilation = compilation(source);
            let keys = match compilation.declared_unit_keys() {
                Ok(keys) => keys,
                Err(error) => panic!("recovered unit keys must be discoverable: {error:?}"),
            };
            let mut recovered = false;

            assert!(!keys.is_empty(), "{source}");

            for key in keys {
                let checked = match compilation.checked_control_flow(key) {
                    Ok(checked) => checked,
                    Err(error) => panic!("recovered semantic check failed: {source}: {error:?}"),
                };

                recovered |= checked.value().is_recovered();
            }

            assert!(recovered, "{source}");
            assert!(!compilation.check_diagnostics().is_empty(), "{source}");
        }
    }

    #[test]
    fn concurrent_package_diagnostic_requests_publish_one_cached_result() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func main()\n",
            "{\n",
            "    missing;\n",
            "}\n",
        ));

        let gate = FactTestGate::holding(FactCellTestEvent::Computing);

        if let Err(error) = compilation
            .state
            .check_diagnostics
            .set_test_observer(gate.observer())
        {
            panic!("package diagnostic fact must accept a test observer: {error:?}");
        }

        let diagnostics = std::thread::scope(|scope| {
            let owner = scope.spawn(|| compilation.check_diagnostics());

            gate.wait_until_observed(FactCellTestEvent::Computing, 1);

            let waiter = scope.spawn(|| compilation.check_diagnostics());

            gate.wait_until_observed(FactCellTestEvent::Waiting, 1);
            gate.release();

            [owner, waiter].map(|handle| match handle.join() {
                Ok(diagnostics) => diagnostics,
                Err(_) => panic!("package diagnostic request panicked"),
            })
        });

        assert!(
            diagnostics
                .iter()
                .skip(1)
                .all(|result| std::ptr::eq(diagnostics[0], *result))
        );
        assert_eq!(
            diagnostic_kinds(diagnostics[0]),
            [DiagnosticKind::BindingUnresolvedName]
        );
    }

    #[test]
    fn semantic_diagnostics_ignore_serial_parallel_and_reversed_demand_order() {
        let sources = [
            concat!(
                "module app;\n",
                "func first()\n",
                "{\n",
                "    let value = 1;\n",
                "    let value = 2;\n",
                "}\n",
            ),
            concat!(
                "module app;\n",
                "func second()\n",
                "{\n",
                "    missing;\n",
                "}\n",
            ),
        ];

        let serial = compilation_with_sources_and_worker_budget(&sources, WorkerBudget::serial());
        let serial_keys = match serial.declared_unit_keys() {
            Ok(keys) => keys,
            Err(error) => panic!("serial unit keys must be discoverable: {error:?}"),
        };

        for key in serial_keys {
            if let Err(error) = serial.checked_control_flow(key) {
                panic!("serial semantic demand failed: {error:?}");
            }
        }

        let parallel_budget = match WorkerBudget::new(4) {
            Ok(budget) => budget,
            Err(error) => panic!("parallel test budget must be valid: {error:?}"),
        };

        let parallel = compilation_with_sources_and_worker_budget(&sources, parallel_budget);
        let mut parallel_keys = match parallel.declared_unit_keys() {
            Ok(keys) => keys,
            Err(error) => panic!("parallel unit keys must be discoverable: {error:?}"),
        };

        parallel_keys.reverse();

        assert_eq!(parallel_keys.len(), 2);

        let gates = parallel_keys
            .iter()
            .map(|key| {
                let gate = FactTestGate::holding(FactCellTestEvent::Computing);

                if let Err(error) = parallel
                    .state
                    .checked_control_flow
                    .set_test_observer(key, gate.observer())
                {
                    panic!("control-flow fact must accept a test observer: {error:?}");
                }

                gate
            })
            .collect::<Vec<_>>();

        // Bound-unit keys share immutable identity storage across worker requests.
        std::thread::scope(|scope| {
            let parallel = &parallel;
            let handles = parallel_keys
                .into_iter()
                .map(|key| scope.spawn(move || parallel.checked_control_flow(key)))
                .collect::<Vec<_>>();

            for gate in &gates {
                gate.wait_until_observed(FactCellTestEvent::Computing, 1);
            }

            for (gate, handle) in gates.iter().zip(handles) {
                gate.release();

                match handle.join() {
                    Ok(Ok(_)) => {}
                    Ok(Err(error)) => panic!("parallel semantic demand failed: {error:?}"),
                    Err(_) => panic!("parallel semantic demand panicked"),
                }
            }
        });

        assert_eq!(serial.check_diagnostics(), parallel.check_diagnostics());
        assert_eq!(
            diagnostic_kinds(serial.check_diagnostics()),
            [
                DiagnosticKind::BindingNameAlreadyDefined,
                DiagnosticKind::BindingUnresolvedName,
            ]
        );
    }
}
